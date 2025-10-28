/**
 * Core type definitions for Janus RDF Framework
 */

/**
 * RDF serialization formats
 */
export enum RdfFormat {
  Turtle = 'turtle',
  NTriples = 'ntriples',
  RdfXml = 'rdfxml',
  JsonLd = 'jsonld',
  NQuads = 'nquads',
  TriG = 'trig',
}

/**
 * SPARQL query result formats
 */
export enum QueryResultFormat {
  Json = 'json',
  Xml = 'xml',
  Csv = 'csv',
  Tsv = 'tsv',
}

/**
 * RDF term types
 */
export enum RdfTermType {
  Uri = 'uri',
  Literal = 'literal',
  BlankNode = 'bnode',
  Triple = 'triple',
}

/**
 * RDF Term interface
 */
export interface RdfTerm {
  type: RdfTermType;
  value: string;
  language?: string;
  datatype?: string;
}

/**
 * RDF Triple/Quad interface
 */
export interface RdfTriple {
  subject: RdfTerm;
  predicate: RdfTerm;
  object: RdfTerm;
  graph?: RdfTerm;
}

/**
 * SPARQL query binding
 */
export interface QueryBinding {
  [variable: string]: RdfTerm;
}

/**
 * SPARQL SELECT query results
 */
export interface SelectQueryResult {
  head: {
    vars: string[];
  };
  results: {
    bindings: QueryBinding[];
  };
}

/**
 * SPARQL ASK query result
 */
export interface AskQueryResult {
  head: Record<string, never>;
  boolean: boolean;
}

/**
 * SPARQL CONSTRUCT query result
 */
export interface ConstructQueryResult {
  triples: RdfTriple[];
}

/**
 * Union type for all query results
 */
export type QueryResult = SelectQueryResult | AskQueryResult | ConstructQueryResult;

/**
 * RDF Store configuration
 */
export interface RdfStoreConfig {
  baseIri?: string;
  limit?: number;
  reasoning?: boolean;
  storePath?: string;
}

/**
 * Query execution options
 */
export interface QueryOptions {
  timeout?: number;
  limit?: number;
  offset?: number;
  reasoning?: boolean;
  defaultGraphUri?: string;
  namedGraphUris?: string[];
}

/**
 * RDF Store endpoint configuration
 */
export interface RdfEndpointConfig {
  url: string;
  storeType: 'jena' | 'oxigraph' | 'kolibrie' | 'other';
  authToken?: string;
  timeoutSecs?: number;
  headers?: Record<string, string>;
}

/**
 * HTTP request options
 */
export interface HttpRequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  headers?: Record<string, string>;
  body?: string | FormData;
  timeout?: number;
  retries?: number;
}

/**
 * Batch operation
 */
export interface BatchOperation {
  operationType: 'query' | 'update' | 'upload' | 'download';
  data: string;
  graphUri?: string;
  format?: RdfFormat;
}

/**
 * RDF Parser options
 */
export interface ParserOptions {
  format: RdfFormat;
  baseIri?: string;
  strict?: boolean;
}

/**
 * RDF Serializer options
 */
export interface SerializerOptions {
  format: RdfFormat;
  baseIri?: string;
  prettyPrint?: boolean;
}

/**
 * Error types
 */
export enum RdfErrorType {
  ParseError = 'ParseError',
  QueryError = 'QueryError',
  SerializationError = 'SerializationError',
  StoreError = 'StoreError',
  HttpError = 'HttpError',
  InvalidIri = 'InvalidIri',
  InvalidFormat = 'InvalidFormat',
  IoError = 'IoError',
  ConfigError = 'ConfigError',
  NotFound = 'NotFound',
  Other = 'Other',
}

/**
 * RDF Error interface
 */
export interface RdfError {
  type: RdfErrorType;
  message: string;
  cause?: Error;
  stack?: string;
}

/**
 * Store statistics
 */
export interface StoreStatistics {
  tripleCount: number;
  graphCount?: number;
  subjectCount?: number;
  predicateCount?: number;
  objectCount?: number;
}

/**
 * Query builder interface
 */
export interface IQueryBuilder {
  addPrefix(prefix: string, iri: string): IQueryBuilder;
  addPattern(subject: string, predicate: string, object: string): IQueryBuilder;
  addFilter(filter: string): IQueryBuilder;
  setLimit(limit: number): IQueryBuilder;
  setOffset(offset: number): IQueryBuilder;
  addOrderBy(variable: string, desc?: boolean): IQueryBuilder;
  build(): string;
}

/**
 * RDF Store adapter interface
 */
export interface IRdfStoreAdapter {
  loadData(data: string, format: RdfFormat, graphName?: string): Promise<number>;
  query(sparql: string, options?: QueryOptions): Promise<QueryResult>;
  insert(triple: RdfTriple): Promise<void>;
  remove(triple: RdfTriple): Promise<void>;
  size(): Promise<number>;
  clear(): Promise<void>;
  export(format: RdfFormat): Promise<string>;
  contains(triple: RdfTriple): Promise<boolean>;
}

/**
 * Logger levels
 */
export enum LogLevel {
  Debug = 'debug',
  Info = 'info',
  Warn = 'warn',
  Error = 'error',
}

/**
 * Logger interface
 */
export interface ILogger {
  debug(message: string, ...args: unknown[]): void;
  info(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
  error(message: string, ...args: unknown[]): void;
}

/**
 * Validation result
 */
export interface ValidationResult {
  valid: boolean;
  errors?: string[];
  warnings?: string[];
  tripleCount?: number;
}

/**
 * Export options
 */
export interface ExportOptions {
  format: RdfFormat;
  graphs?: string[];
  prettyPrint?: boolean;
  includeMetadata?: boolean;
}

/**
 * Import options
 */
export interface ImportOptions {
  format: RdfFormat;
  graphUri?: string;
  baseIri?: string;
  overwrite?: boolean;
  validateBeforeImport?: boolean;
}
