/**
 * Oxigraph HTTP Adapter
 * Adapter for connecting to Oxigraph RDF store via HTTP API
 */

import axios, { AxiosInstance } from 'axios';
import * as N3 from 'n3';
import {
  IRdfStoreAdapter,
  RdfFormat,
  QueryResult,
  QueryOptions,
  RdfTriple,
  SelectQueryResult,
  AskQueryResult,
  ConstructQueryResult,
  RdfEndpointConfig,
  RdfTerm,
  RdfErrorType,
} from '../core/types';
import { RdfError as RdfErrorClass } from '../utils/errors';
import { Logger } from '../utils/logger';

/**
 * Oxigraph adapter for HTTP-based RDF operations
 */
export class OxigraphAdapter implements IRdfStoreAdapter {
  private client: AxiosInstance;
  private endpoint: RdfEndpointConfig;
  private logger: Logger;

  constructor(endpoint: RdfEndpointConfig) {
    this.endpoint = {
      ...endpoint,
      storeType: 'oxigraph',
      timeoutSecs: endpoint.timeoutSecs || 30,
    };

    this.logger = new Logger('OxigraphAdapter');

    // Create axios instance with default configuration
    this.client = axios.create({
      baseURL: this.endpoint.url.replace(/\/$/, ''),
      timeout: (this.endpoint.timeoutSecs || 30) * 1000,
      headers: {
        'Content-Type': 'application/sparql-query',
        ...this.endpoint.headers,
      },
    });

    // Add auth interceptor if token is provided
    if (this.endpoint.authToken) {
      this.client.interceptors.request.use((config) => {
        if (this.endpoint.authToken) {
          config.headers.Authorization = `Bearer ${this.endpoint.authToken}`;
        }
        return config;
      });
    }
  }

  /**
   * Load RDF data into the store
   */
  async loadData(data: string, format: RdfFormat, graphName?: string): Promise<number> {
    try {
      const url = graphName ? `/store?graph=${encodeURIComponent(graphName)}` : '/store';
      const contentType = this.getContentType(format);

      await this.client.post(url, data, {
        headers: {
          'Content-Type': contentType,
        },
      });

      this.logger.info(`Data loaded successfully to Oxigraph (format: ${format})`);

      // Oxigraph doesn't return count, so we return -1 to indicate success without count
      return -1;
    } catch (error) {
      this.logger.error('Failed to load data', error);
      throw this.handleError(error);
    }
  }

  /**
   * Execute a SPARQL query
   */
  async query(sparql: string, options?: QueryOptions): Promise<QueryResult> {
    try {
      const isConstruct = sparql.trim().toUpperCase().startsWith('CONSTRUCT');
      const acceptHeader = isConstruct
        ? 'application/n-triples'
        : 'application/sparql-results+json';

      const response = await this.client.post('/query', sparql, {
        headers: {
          'Content-Type': 'application/sparql-query',
          Accept: acceptHeader,
        },
        params: this.buildQueryParams(options),
      });

      if (isConstruct) {
        // Parse N-Triples response for CONSTRUCT query
        const triples: RdfTriple[] = [];
        const parser = new N3.Parser({ format: 'N-Triples' });
        parser.parse(response.data, (error, quad) => {
          if (error) {
            throw new RdfErrorClass(
              RdfErrorType.QueryError,
              `Failed to parse CONSTRUCT result: ${error.message}`
            );
          }
          if (quad) {
            triples.push({
              subject: this.n3TermToRdfTerm(quad.subject),
              predicate: this.n3TermToRdfTerm(quad.predicate),
              object: this.n3TermToRdfTerm(quad.object),
            });
          }
        });

        this.logger.debug(`CONSTRUCT query executed successfully (${triples.length} triples)`);
        return { triples } as ConstructQueryResult;
      }

      const result = response.data as QueryResult;

      // Determine query type from result structure
      if ('boolean' in result) {
        this.logger.debug('ASK query executed successfully');
        return result as AskQueryResult;
      } else if ('results' in result && 'bindings' in result.results) {
        this.logger.debug(
          `SELECT query executed successfully (${result.results.bindings.length} results)`
        );
        return result as SelectQueryResult;
      }

      return result;
    } catch (error) {
      this.logger.error('Query execution failed', error);
      throw this.handleError(error);
    }
  }

  /**
   * Insert a triple into the store
   */
  async insert(triple: RdfTriple): Promise<void> {
    const sparqlUpdate = this.buildInsertQuery(triple);
    await this.executeUpdate(sparqlUpdate);
  }

  /**
   * Remove a triple from the store
   */
  async remove(triple: RdfTriple): Promise<void> {
    const sparqlUpdate = this.buildDeleteQuery(triple);
    await this.executeUpdate(sparqlUpdate);
  }

  /**
   * Get the number of triples in the store
   */
  async size(): Promise<number> {
    const query = 'SELECT (COUNT(*) as ?count) WHERE { ?s ?p ?o }';
    const result = (await this.query(query)) as SelectQueryResult;

    if (result.results.bindings.length > 0) {
      const countValue = result.results.bindings[0]?.count?.value;
      return countValue ? parseInt(countValue, 10) : 0;
    }

    return 0;
  }

  /**
   * Clear all data from the store
   */
  async clear(): Promise<void> {
    const sparqlUpdate = 'CLEAR ALL';
    await this.executeUpdate(sparqlUpdate);
    this.logger.info('Store cleared successfully');
  }

  /**
   * Export data from the store
   */
  async export(format: RdfFormat): Promise<string> {
    try {
      const response = await this.client.get('/store', {
        params: {
          format: this.getFormatParam(format),
        },
      });

      return response.data;
    } catch (error) {
      this.logger.error('Export failed', error);
      throw this.handleError(error);
    }
  }

  /**
   * Check if a triple exists in the store
   */
  async contains(triple: RdfTriple): Promise<boolean> {
    const query = this.buildAskQuery(triple);
    const result = (await this.query(query)) as AskQueryResult;
    return result.boolean;
  }

  /**
   * Execute a SPARQL UPDATE operation
   */
  async executeUpdate(sparqlUpdate: string): Promise<void> {
    try {
      await this.client.post('/update', sparqlUpdate, {
        headers: {
          'Content-Type': 'application/sparql-update',
        },
      });

      this.logger.debug('Update executed successfully');
    } catch (error) {
      this.logger.error('Update execution failed', error);
      throw this.handleError(error);
    }
  }

  /**
   * Ping the Oxigraph server
   */
  async ping(): Promise<boolean> {
    try {
      const response = await this.client.get('/');
      return response.status === 200;
    } catch (error) {
      this.logger.warn('Ping failed', error);
      return false;
    }
  }

  /**
   * Get store statistics
   */
  async getStatistics(): Promise<{ tripleCount: number }> {
    const tripleCount = await this.size();
    return { tripleCount };
  }

  /**
   * Build query parameters from options
   */
  private buildQueryParams(options?: QueryOptions): Record<string, string> {
    const params: Record<string, string> = {};

    if (options?.defaultGraphUri) {
      params['default-graph-uri'] = options.defaultGraphUri;
    }

    if (options?.namedGraphUris) {
      params['named-graph-uri'] = options.namedGraphUris.join(',');
    }

    return params;
  }

  /**
   * Build SPARQL INSERT query from triple
   */
  private buildInsertQuery(triple: RdfTriple): string {
    const subject = this.termToSparql(triple.subject);
    const predicate = this.termToSparql(triple.predicate);
    const object = this.termToSparql(triple.object);

    if (triple.graph) {
      const graph = this.termToSparql(triple.graph);
      return `INSERT DATA { GRAPH ${graph} { ${subject} ${predicate} ${object} } }`;
    }

    return `INSERT DATA { ${subject} ${predicate} ${object} }`;
  }

  /**
   * Build SPARQL DELETE query from triple
   */
  private buildDeleteQuery(triple: RdfTriple): string {
    const subject = this.termToSparql(triple.subject);
    const predicate = this.termToSparql(triple.predicate);
    const object = this.termToSparql(triple.object);

    if (triple.graph) {
      const graph = this.termToSparql(triple.graph);
      return `DELETE DATA { GRAPH ${graph} { ${subject} ${predicate} ${object} } }`;
    }

    return `DELETE DATA { ${subject} ${predicate} ${object} }`;
  }

  /**
   * Build SPARQL ASK query from triple
   */
  private buildAskQuery(triple: RdfTriple): string {
    const subject = this.termToSparql(triple.subject);
    const predicate = this.termToSparql(triple.predicate);
    const object = this.termToSparql(triple.object);

    if (triple.graph) {
      const graph = this.termToSparql(triple.graph);
      return `ASK { GRAPH ${graph} { ${subject} ${predicate} ${object} } }`;
    }

    return `ASK { ${subject} ${predicate} ${object} }`;
  }

  /**
   * Convert RDF term to SPARQL syntax
   */
  private termToSparql(term: RdfTerm): string {
    switch (term.type) {
      case 'uri':
        return `<${term.value}>`;
      case 'bnode':
        return `_:${term.value}`;
      case 'literal':
        if (term.language) {
          return `"${this.escapeLiteral(term.value)}"@${term.language}`;
        } else if (term.datatype && term.datatype !== 'http://www.w3.org/2001/XMLSchema#string') {
          return `"${this.escapeLiteral(term.value)}"^^<${term.datatype}>`;
        }
        return `"${this.escapeLiteral(term.value)}"`;
      default:
        throw new RdfErrorClass(RdfErrorType.InvalidFormat, `Unknown term type: ${term.type}`);
    }
  }

  /**
   * Escape special characters in literal values
   */
  private escapeLiteral(value: string): string {
    return value
      .replace(/\\/g, '\\\\')
      .replace(/"/g, '\\"')
      .replace(/\n/g, '\\n')
      .replace(/\r/g, '\\r')
      .replace(/\t/g, '\\t');
  }

  /**
   * Get format param for RDF format
   */
  private getFormatParam(format: RdfFormat): string {
    switch (format) {
      case RdfFormat.Turtle:
        return 'ttl';
      case RdfFormat.NTriples:
        return 'nt';
      case RdfFormat.RdfXml:
        return 'xml';
      case RdfFormat.JsonLd:
        return 'jsonld';
      case RdfFormat.NQuads:
        return 'nq';
      case RdfFormat.TriG:
        return 'trig';
      default:
        throw new RdfErrorClass(RdfErrorType.InvalidFormat, `Unsupported format: ${format}`);
    }
  }

  /**
   * Get content type for RDF format
   */
  private getContentType(format: RdfFormat): string {
    switch (format) {
      case RdfFormat.Turtle:
        return 'text/turtle';
      case RdfFormat.NTriples:
        return 'application/n-triples';
      case RdfFormat.RdfXml:
        return 'application/rdf+xml';
      case RdfFormat.JsonLd:
        return 'application/ld+json';
      case RdfFormat.NQuads:
        return 'application/n-quads';
      case RdfFormat.TriG:
        return 'application/trig';
      default:
        throw new RdfErrorClass(RdfErrorType.InvalidFormat, `Unsupported format: ${format}`);
    }
  }

  /**
   * Convert N3 term to RdfTerm
   */
  private n3TermToRdfTerm(term: N3.Term): RdfTerm {
    if (term.termType === 'NamedNode') {
      return { type: 'uri', value: term.value };
    } else if (term.termType === 'BlankNode') {
      return { type: 'bnode', value: term.value };
    } else if (term.termType === 'Literal') {
      const literalTerm = term as N3.Literal;
      return {
        type: 'literal',
        value: literalTerm.value,
        datatype: literalTerm.datatype?.value,
        language: literalTerm.language,
      };
    }
    throw new RdfErrorClass(RdfErrorType.InvalidFormat, `Unknown N3 term type: ${term.termType}`);
  }

  /**
   * Handle errors and convert to RdfError
   */
  private handleError(error: unknown): RdfErrorClass {
    if (axios.isAxiosError(error)) {
      const status = error.response?.status;
      const message = error.response?.data?.message || error.message;

      if (status === 404) {
        return new RdfErrorClass(RdfErrorType.NotFound, `Resource not found: ${message}`);
      } else if (status === 400) {
        return new RdfErrorClass(RdfErrorType.QueryError, `Bad request: ${message}`);
      } else if (status === 500) {
        return new RdfErrorClass(RdfErrorType.StoreError, `Server error: ${message}`);
      }

      return new RdfErrorClass(RdfErrorType.HttpError, `HTTP error (${status}): ${message}`);
    }

    if (error instanceof Error) {
      return new RdfErrorClass(RdfErrorType.Other, error.message, error);
    }

    return new RdfErrorClass(RdfErrorType.Other, String(error));
  }
}
