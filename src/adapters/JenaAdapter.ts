/**
 * Apache Jena HTTP Adapter
 * Adapter for connecting to Apache Jena Fuseki via HTTP API
 */
import axios, { AxiosInstance } from 'axios';

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
 * Apache Jena Fuseki adapter for HTTP-based RDF operations
 */
export class JenaAdapter implements IRdfStoreAdapter {
  private client: AxiosInstance;
  private endpoint: RdfEndpointConfig;
  private logger: Logger;
  private datasetName: string;

  constructor(endpoint: RdfEndpointConfig, datasetName: string = 'dataset') {
    this.endpoint = {
      ...endpoint,
      storeType: 'jena',
      timeoutSecs: endpoint.timeoutSecs || 30,
    };

    this.datasetName = datasetName;
    this.logger = new Logger('JenaAdapter');

    // Create axios instance with default configuration
    this.client = axios.create({
      baseURL: this.endpoint.url.replace(/\/$/, ''),
      timeout: (this.endpoint.timeoutSecs || 30) * 1000,
      headers: {
        ...this.endpoint.headers,
      },
    });

    // Add auth interceptor if token is provided
    if (this.endpoint.authToken) {
      this.client.interceptors.request.use((config) => {
        if (this.endpoint.authToken) {
          config.headers.Authorization = `Basic ${Buffer.from(this.endpoint.authToken).toString('base64')}`;
        }
        return config;
      });
    }
  }

  /**
   * Get the base path for dataset endpoints
   */
  private get basePath(): string {
    return this.datasetName ? `/${this.datasetName}` : '';
  }

  /**
   * Load RDF data into the store
   */
  async loadData(data: string, format: RdfFormat, graphName?: string): Promise<number> {
    try {
      const url = graphName
        ? `${this.basePath}/data?graph=${encodeURIComponent(graphName)}`
        : `${this.basePath}/data`;

      const contentType = this.getContentType(format);

      const method = graphName ? 'put' : 'post';

      await this.client.request({
        method,
        url,
        data,
        headers: {
          'Content-Type': contentType,
        },
      });

      this.logger.info(`Data loaded successfully to Jena (format: ${format})`);

      // Jena doesn't return count, so we return -1 to indicate success without count
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
      // Jena Fuseki uses GET with query parameter
      const response = await this.client.get(`${this.basePath}/sparql`, {
        headers: {
          Accept: 'application/sparql-results+json',
        },
        params: {
          query: sparql,
          ...this.buildQueryParams(options),
        },
      });

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
      } else if ('triples' in result) {
        this.logger.debug(`CONSTRUCT query executed successfully`);
        return result as ConstructQueryResult;
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
      const response = await this.client.get(`${this.basePath}/data`, {
        headers: {
          Accept: this.getContentType(format),
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
      await this.client.post(
        `${this.basePath}/update`,
        new URLSearchParams({ update: sparqlUpdate }),
        {
          headers: {
            'Content-Type': 'application/x-www-form-urlencoded',
          },
        }
      );

      this.logger.debug('Update executed successfully');
    } catch (error) {
      this.logger.error('Update execution failed', error);
      throw this.handleError(error);
    }
  }

  /**
   * Ping the Jena server
   */
  async ping(): Promise<boolean> {
    try {
      const response = await this.client.get('/$/ping');
      return response.status === 200;
    } catch (error) {
      this.logger.warn('Ping failed', error);
      return false;
    }
  }

  /**
   * Get server status and statistics
   */
  async getServerStatus(): Promise<Record<string, unknown>> {
    try {
      const response = await this.client.get('/$/server');
      return response.data;
    } catch (error) {
      this.logger.error('Failed to get server status', error);
      throw this.handleError(error);
    }
  }

  /**
   * Get dataset statistics
   */
  async getStatistics(): Promise<{ tripleCount: number }> {
    const tripleCount = await this.size();
    return { tripleCount };
  }

  /**
   * List all graphs in the dataset
   */
  async listGraphs(): Promise<string[]> {
    const query = `
      SELECT DISTINCT ?g WHERE {
        GRAPH ?g { ?s ?p ?o }
      }
    `;

    const result = (await this.query(query)) as SelectQueryResult;
    return result.results.bindings.map((binding) => binding.g?.value || '');
  }

  /**
   * Get statistics for a specific graph
   */
  async getGraphStatistics(graphUri: string): Promise<{ tripleCount: number }> {
    const query = `
      SELECT (COUNT(*) as ?count) WHERE {
        GRAPH <${graphUri}> { ?s ?p ?o }
      }
    `;

    const result = (await this.query(query)) as SelectQueryResult;
    const countValue = result.results.bindings[0]?.count?.value;
    const tripleCount = countValue ? parseInt(countValue, 10) : 0;

    return { tripleCount };
  }

  /**
   * Upload data from file (using Jena's upload endpoint)
   */
  async uploadFile(formData: FormData): Promise<void> {
    try {
      await this.client.post(`${this.basePath}/upload`, formData, {
        headers: {
          'Content-Type': 'multipart/form-data',
        },
      });

      this.logger.info('File uploaded successfully');
    } catch (error) {
      this.logger.error('File upload failed', error);
      throw this.handleError(error);
    }
  }

  /**
   * Execute a GSP (Graph Store Protocol) operation
   */
  async gspGet(graphUri?: string): Promise<string> {
    try {
      const url = graphUri
        ? `${this.basePath}/data?graph=${encodeURIComponent(graphUri)}`
        : `${this.basePath}/data`;

      const response = await this.client.get(url, {
        headers: {
          Accept: 'text/turtle',
        },
      });

      return response.data;
    } catch (error) {
      this.logger.error('GSP GET failed', error);
      throw this.handleError(error);
    }
  }

  /**
   * Build query parameters from options
   */
  private buildQueryParams(options?: QueryOptions): Record<string, string | string[]> {
    const params: Record<string, string | string[]> = {};

    if (options?.defaultGraphUri) {
      params['default-graph-uri'] = options.defaultGraphUri;
    }

    if (options?.namedGraphUris) {
      // Jena supports multiple named-graph-uri parameters
      params['named-graph-uri'] = options.namedGraphUris;
    }

    if (options?.timeout) {
      params.timeout = String(options.timeout);
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
      } else if (status === 401 || status === 403) {
        return new RdfErrorClass(
          RdfErrorType.HttpError,
          `Authentication error (${status}): ${message}`
        );
      }

      return new RdfErrorClass(RdfErrorType.HttpError, `HTTP error (${status}): ${message}`);
    }

    if (error instanceof Error) {
      return new RdfErrorClass(RdfErrorType.Other, error.message, error);
    }

    return new RdfErrorClass(RdfErrorType.Other, String(error));
  }
}
