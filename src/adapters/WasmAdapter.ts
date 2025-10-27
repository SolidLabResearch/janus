//! WASM Adapter for Janus RDF
//!
//! This adapter provides direct integration with the Rust WASM module
//! for high-performance, in-browser RDF processing.

import { RdfStore } from '../../pkg/janus_rdf_rust';
import {
  IRdfStoreAdapter,
  RdfFormat,
  RdfTriple,
  QueryResult,
  SelectQueryResult,
  AskQueryResult,
  ConstructQueryResult,
  RdfErrorType,
} from '../core/types';
import { RdfError } from '../utils/errors';

/**
 * WASM-based RDF store adapter
 *
 * Provides direct access to Oxigraph's in-memory store via WebAssembly
 * for maximum performance without HTTP overhead.
 */
export class WasmAdapter implements IRdfStoreAdapter {
  private store: RdfStore | null = null;
  private initialized = false;

  private constructor() {}

  /**
   * Create a new WASM adapter instance
   */
  static async create(): Promise<WasmAdapter> {
    const adapter = new WasmAdapter();
    await adapter.initialize();
    return adapter;
  }

  private async initialize(): Promise<void> {
    try {
      // Import the WASM module
      const wasmModule = await import('../../pkg/janus_rdf_rust');

      // Initialize the WASM module
      wasmModule.init();

      // Create the store
      this.store = new RdfStore();
      this.initialized = true;
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to initialize WASM adapter: ${error}`);
    }
  }

  private ensureInitialized(): void {
    if (!this.initialized || !this.store) {
      throw new RdfError(RdfErrorType.StoreError, 'WASM adapter not initialized');
    }
  }

  async loadData(data: string, format: RdfFormat, graphName?: string): Promise<number> {
    this.ensureInitialized();

    try {
      // Convert format to string
      const formatStr = this.formatToString(format);

      // Load data into the store
      const count = this.store!.loadData(data, formatStr, graphName || null);
      return count;
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to load data: ${error}`);
    }
  }

  async query(sparql: string, _options?: any): Promise<QueryResult> {
    this.ensureInitialized();

    try {
      // Execute the query
      const resultJson = this.store!.query(sparql);

      // Parse the result
      const result = JSON.parse(resultJson);

      // Convert to our QueryResult format
      if (result.head && result.results) {
        // SELECT query result
        const selectResult: SelectQueryResult = {
          head: {
            vars: result.head.vars,
          },
          results: {
            bindings: result.results.bindings.map((binding: any) => {
              const converted: any = {};
              for (const [key, value] of Object.entries(binding)) {
                converted[key] = this.convertTerm(value);
              }
              return converted;
            }),
          },
        };
        return selectResult;
      } else if (result.boolean !== undefined) {
        // ASK query result
        const askResult: AskQueryResult = {
          head: {},
          boolean: result.boolean,
        };
        return askResult;
      } else if (result.triples) {
        // CONSTRUCT/DESCRIBE query result
        const constructResult: ConstructQueryResult = {
          triples: result.triples.map((triple: any) => ({
            subject: this.convertTerm(triple.subject),
            predicate: this.convertTerm(triple.predicate),
            object: this.convertTerm(triple.object),
          })),
        };
        return constructResult;
      } else {
        throw new RdfError(RdfErrorType.InvalidFormat, 'Unknown query result format');
      }
    } catch (error) {
      throw new RdfError(RdfErrorType.QueryError, `Query execution failed: ${error}`);
    }
  }

  async insert(triple: RdfTriple): Promise<void> {
    this.ensureInitialized();

    try {
      const subject = this.termToString(triple.subject);
      const predicate = this.termToString(triple.predicate);
      const object = this.termToString(triple.object);

      this.store!.insertTriple(
        subject,
        predicate,
        object,
        triple.graph ? this.termToString(triple.graph) : null
      );
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to insert triple: ${error}`);
    }
  }

  async remove(triple: RdfTriple): Promise<void> {
    this.ensureInitialized();

    try {
      const subject = this.termToString(triple.subject);
      const predicate = this.termToString(triple.predicate);
      const object = this.termToString(triple.object);

      this.store!.removeTriple(
        subject,
        predicate,
        object,
        triple.graph ? this.termToString(triple.graph) : null
      );
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to remove triple: ${error}`);
    }
  }

  async size(): Promise<number> {
    this.ensureInitialized();

    try {
      return Number(this.store!.size());
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to get store size: ${error}`);
    }
  }

  async clear(): Promise<void> {
    this.ensureInitialized();

    try {
      this.store!.clear();
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to clear store: ${error}`);
    }
  }

  async export(format: RdfFormat): Promise<string> {
    this.ensureInitialized();

    try {
      const formatStr = this.formatToString(format);
      return this.store!.export(formatStr);
    } catch (error) {
      throw new RdfError(RdfErrorType.SerializationError, `Failed to export data: ${error}`);
    }
  }

  private formatToString(format: RdfFormat): string {
    switch (format) {
      case RdfFormat.Turtle:
        return 'turtle';
      case RdfFormat.NTriples:
        return 'ntriples';
      case RdfFormat.RdfXml:
        return 'rdfxml';
      case RdfFormat.NQuads:
        return 'nquads';
      case RdfFormat.TriG:
        return 'trig';
      case RdfFormat.JsonLd:
        return 'jsonld';
      default:
        throw new RdfError(RdfErrorType.InvalidFormat, `Unsupported format: ${format}`);
    }
  }

  private convertTerm(term: any): any {
    switch (term.type) {
      case 'uri':
        return { type: 'uri', value: term.value };
      case 'literal': {
        const result: any = { type: 'literal', value: term.value };
        if (term.datatype) {
          result.datatype = term.datatype;
        }
        if (term.language) {
          result.language = term.language;
        }
        return result;
      }
      case 'bnode':
        return { type: 'bnode', value: term.value };
      default:
        return term;
    }
  }

  async contains(triple: RdfTriple): Promise<boolean> {
    this.ensureInitialized();

    try {
      const subject = this.termToString(triple.subject);
      const predicate = this.termToString(triple.predicate);
      const object = this.termToString(triple.object);

      return this.store!.contains(
        subject,
        predicate,
        object,
        triple.graph ? this.termToString(triple.graph) : null
      );
    } catch (error) {
      throw new RdfError(RdfErrorType.StoreError, `Failed to check triple: ${error}`);
    }
  }

  private termToString(term: any): string {
    switch (term.type) {
      case 'uri':
        return term.value;
      case 'literal':
        if (term.datatype) {
          return `"${term.value}"^^<${term.datatype}>`;
        } else if (term.language) {
          return `"${term.value}"@${term.language}`;
        } else {
          return `"${term.value}"`;
        }
      case 'bnode':
        return `_:${term.value}`;
      default:
        return term.value || term.toString();
    }
  }
}
