/**
 * In-Memory RDF Store Adapter
 * Simple implementation for testing and development without external dependencies
 */

import {
  IRdfStoreAdapter,
  RdfFormat,
  QueryResult,
  QueryOptions,
  RdfTriple,
  SelectQueryResult,
  AskQueryResult,
  ConstructQueryResult,
  RdfTerm,
  RdfTermType,
  RdfErrorType,
} from '../core/types';
import { RdfError } from '../utils/errors';
import { Logger } from '../utils/logger';

/**
 * Simple in-memory RDF triple store
 */
interface TripleStore {
  [subject: string]: {
    [predicate: string]: {
      [object: string]: boolean; // Use boolean for fast lookup
    };
  };
}

/**
 * In-memory RDF store adapter for testing and development
 */
export class InMemoryAdapter implements IRdfStoreAdapter {
  private store: TripleStore = {};
  private logger: Logger;

  constructor() {
    this.logger = new Logger('InMemoryAdapter');
  }

  /**
   * Load RDF data into the store
   */
  async loadData(data: string, format: RdfFormat, _graphName?: string): Promise<number> {
    try {
      let count = 0;

      // Simple parsing based on format
      const lines = data.split('\n').filter((line) => line.trim() && !line.trim().startsWith('#'));

      for (const line of lines) {
        const triple = this.parseLine(line.trim(), format);
        if (triple) {
          this.addTriple(triple);
          count++;
        }
      }

      this.logger.info(`Loaded ${count} triples`);
      return count;
    } catch (error) {
      this.logger.error('Failed to load data', error);
      throw new RdfError(RdfErrorType.ParseError, `Failed to parse RDF data: ${error}`);
    }
  }

  /**
   * Execute a SPARQL query
   */
  async query(sparql: string, _options?: QueryOptions): Promise<QueryResult> {
    try {
      const queryType = this.detectQueryType(sparql);

      switch (queryType) {
        case 'SELECT':
          return this.executeSelect(sparql);
        case 'ASK':
          return this.executeAsk(sparql);
        case 'CONSTRUCT':
          return this.executeConstruct(sparql);
        default:
          throw new RdfError(RdfErrorType.QueryError, 'Unsupported query type');
      }
    } catch (error) {
      this.logger.error('Query execution failed', error);
      throw error;
    }
  }

  /**
   * Insert a triple into the store
   */
  async insert(triple: RdfTriple): Promise<void> {
    this.addTriple(triple);
    this.logger.debug('Triple inserted');
  }

  /**
   * Remove a triple from the store
   */
  async remove(triple: RdfTriple): Promise<void> {
    this.removeTriple(triple);
    this.logger.debug('Triple removed');
  }

  /**
   * Get the number of triples in the store
   */
  async size(): Promise<number> {
    let count = 0;
    for (const subject in this.store) {
      for (const predicate in this.store[subject]) {
        count += Object.keys(this.store[subject][predicate] || {}).length;
      }
    }
    return count;
  }

  /**
   * Clear all data from the store
   */
  async clear(): Promise<void> {
    this.store = {};
    this.logger.info('Store cleared');
  }

  /**
   * Export data from the store
   */
  async export(format: RdfFormat): Promise<string> {
    const triples = await this.getAllTriples();

    switch (format) {
      case RdfFormat.Turtle:
        return this.exportAsTurtle(triples);
      case RdfFormat.NTriples:
        return this.exportAsNTriples(triples);
      default:
        throw new RdfError(RdfErrorType.InvalidFormat, `Export format ${format} not supported`);
    }
  }

  /**
   * Check if a triple exists in the store
   */
  async contains(triple: RdfTriple): Promise<boolean> {
    const subjectKey = this.termToKey(triple.subject);
    const predicateKey = this.termToKey(triple.predicate);
    const objectKey = this.termToKey(triple.object);

    return !!(
      this.store[subjectKey] &&
      this.store[subjectKey][predicateKey] &&
      this.store[subjectKey][predicateKey][objectKey]
    );
  }

  // Private helper methods

  private parseLine(line: string, format: RdfFormat): RdfTriple | null {
    try {
      switch (format) {
        case RdfFormat.Turtle:
          return this.parseTurtleLine(line);
        case RdfFormat.NTriples:
          return this.parseNTriplesLine(line);
        default:
          return null;
      }
    } catch {
      return null;
    }
  }

  private parseTurtleLine(line: string): RdfTriple | null {
    // Very basic Turtle parsing - just for testing
    const match = line.match(/^<([^>]+)>\s*<([^>]+)>\s*<([^>]+)>\s*\.$/);
    if (match && match[1] && match[2] && match[3]) {
      return {
        subject: { type: RdfTermType.Uri, value: match[1] },
        predicate: { type: RdfTermType.Uri, value: match[2] },
        object: { type: RdfTermType.Uri, value: match[3] },
      };
    }

    // Handle literals
    const literalMatch = line.match(/^<([^>]+)>\s*<([^>]+)>\s*"([^"]*)"\s*\.$/);
    if (literalMatch && literalMatch[1] && literalMatch[2] && literalMatch[3]) {
      return {
        subject: { type: RdfTermType.Uri, value: literalMatch[1] },
        predicate: { type: RdfTermType.Uri, value: literalMatch[2] },
        object: { type: RdfTermType.Literal, value: literalMatch[3] },
      };
    }

    return null;
  }

  private parseNTriplesLine(line: string): RdfTriple | null {
    const match = line.match(/^<([^>]+)>\s*<([^>]+)>\s*<([^>]+)>\s*\.$/);
    if (match && match[1] && match[2] && match[3]) {
      return {
        subject: { type: RdfTermType.Uri, value: match[1] },
        predicate: { type: RdfTermType.Uri, value: match[2] },
        object: { type: RdfTermType.Uri, value: match[3] },
      };
    }

    const literalMatch = line.match(/^<([^>]+)>\s*<([^>]+)>\s*"([^"]*)"\s*\.$/);
    if (literalMatch && literalMatch[1] && literalMatch[2] && literalMatch[3]) {
      return {
        subject: { type: RdfTermType.Uri, value: literalMatch[1] },
        predicate: { type: RdfTermType.Uri, value: literalMatch[2] },
        object: { type: RdfTermType.Literal, value: literalMatch[3] },
      };
    }

    return null;
  }

  private addTriple(triple: RdfTriple): void {
    const subjectKey = this.termToKey(triple.subject);
    const predicateKey = this.termToKey(triple.predicate);
    const objectKey = this.termToKey(triple.object);

    if (!this.store[subjectKey]) {
      this.store[subjectKey] = {};
    }
    if (!this.store[subjectKey][predicateKey]) {
      this.store[subjectKey][predicateKey] = {};
    }
    this.store[subjectKey][predicateKey][objectKey] = true;
  }

  private removeTriple(triple: RdfTriple): void {
    const subjectKey = this.termToKey(triple.subject);
    const predicateKey = this.termToKey(triple.predicate);
    const objectKey = this.termToKey(triple.object);

    if (this.store[subjectKey]?.[predicateKey]?.[objectKey]) {
      delete this.store[subjectKey][predicateKey][objectKey];
    }
  }

  private termToKey(term: RdfTerm): string {
    switch (term.type) {
      case RdfTermType.Uri:
        return `<${term.value}>`;
      case RdfTermType.BlankNode:
        return `_:${term.value}`;
      case RdfTermType.Literal:
        return `"${term.value}"`;
      default:
        return term.value;
    }
  }

  private detectQueryType(query: string): string {
    const upperQuery = query.trim().toUpperCase();
    if (upperQuery.startsWith('SELECT')) {
      return 'SELECT';
    }
    if (upperQuery.startsWith('ASK')) {
      return 'ASK';
    }
    if (upperQuery.startsWith('CONSTRUCT')) {
      return 'CONSTRUCT';
    }
    return 'UNKNOWN';
  }

  private executeSelect(_query: string): SelectQueryResult {
    // Very basic SELECT implementation - just return all triples
    const triples = this.getAllTriplesSync();

    const bindings = triples.map((triple, index) => ({
      subject: triple.subject,
      predicate: triple.predicate,
      object: triple.object,
      id: { type: RdfTermType.Literal, value: index.toString() },
    }));

    return {
      head: {
        vars: ['subject', 'predicate', 'object', 'id'],
      },
      results: {
        bindings,
      },
    };
  }

  private executeAsk(_query: string): AskQueryResult {
    // Simple ASK - return true if store has any triples
    const hasTriples = Object.keys(this.store).length > 0;
    return {
      head: {},
      boolean: hasTriples,
    };
  }

  private executeConstruct(_query: string): ConstructQueryResult {
    // Simple CONSTRUCT - return all triples
    const triples = this.getAllTriplesSync();
    return {
      triples,
    };
  }

  private async getAllTriples(): Promise<RdfTriple[]> {
    return this.getAllTriplesSync();
  }

  private getAllTriplesSync(): RdfTriple[] {
    const triples: RdfTriple[] = [];

    for (const subjectKey in this.store) {
      for (const predicateKey in this.store[subjectKey]) {
        for (const objectKey in this.store[subjectKey][predicateKey]) {
          triples.push({
            subject: this.keyToTerm(subjectKey),
            predicate: this.keyToTerm(predicateKey),
            object: this.keyToTerm(objectKey),
          });
        }
      }
    }

    return triples;
  }

  private keyToTerm(key: string): RdfTerm {
    if (key.startsWith('<') && key.endsWith('>')) {
      return { type: RdfTermType.Uri, value: key.slice(1, -1) };
    }
    if (key.startsWith('"') && key.endsWith('"')) {
      return { type: RdfTermType.Literal, value: key.slice(1, -1) };
    }
    if (key.startsWith('_:')) {
      return { type: RdfTermType.BlankNode, value: key.slice(2) };
    }
    return { type: RdfTermType.Uri, value: key };
  }

  private exportAsTurtle(triples: RdfTriple[]): string {
    let result = '@prefix ex: <http://example.org/> .\n\n';
    triples.forEach((triple) => {
      result += `${this.termToString(triple.subject)} ${this.termToString(triple.predicate)} ${this.termToString(triple.object)} .\n`;
    });
    return result;
  }

  private exportAsNTriples(triples: RdfTriple[]): string {
    let result = '';
    triples.forEach((triple) => {
      result += `${this.termToString(triple.subject)} ${this.termToString(triple.predicate)} ${this.termToString(triple.object)} .\n`;
    });
    return result;
  }

  private termToString(term: RdfTerm): string {
    switch (term.type) {
      case RdfTermType.Uri:
        return `<${term.value}>`;
      case RdfTermType.BlankNode:
        return `_:${term.value}`;
      case RdfTermType.Literal:
        return `"${term.value}"`;
      default:
        return term.value;
    }
  }
}
