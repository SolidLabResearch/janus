/**
 * Validation utilities for Janus RDF Framework
 */

import { RdfFormat, RdfTerm, RdfTriple, QueryOptions, RdfErrorType } from '../core/types';
import { InvalidIriError, InvalidFormatError, RdfError } from './errors';

/**
 * IRI validation regex patterns
 */
const IRI_PATTERN = /^(https?|ftp|file):\/\/[^\s/$.?#].[^\s]*$/i;
const RELATIVE_IRI_PATTERN = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
const BLANK_NODE_PATTERN = /^_:[a-zA-Z0-9_-]+$/;

/**
 * SPARQL keyword patterns
 */
const SPARQL_KEYWORDS = [
  'SELECT',
  'CONSTRUCT',
  'ASK',
  'DESCRIBE',
  'INSERT',
  'DELETE',
  'WHERE',
  'FILTER',
  'OPTIONAL',
  'UNION',
  'GRAPH',
  'PREFIX',
  'BASE',
];

/**
 * Validate an IRI (Internationalized Resource Identifier)
 */
export function validateIri(iri: string, allowRelative: boolean = false): boolean {
  if (!iri || typeof iri !== 'string') {
    return false;
  }

  // Check for blank nodes
  if (iri.startsWith('_:')) {
    return BLANK_NODE_PATTERN.test(iri);
  }

  // Check for absolute IRI
  if (IRI_PATTERN.test(iri)) {
    return true;
  }

  // Check for relative IRI if allowed
  if (allowRelative && RELATIVE_IRI_PATTERN.test(iri)) {
    return true;
  }

  return false;
}

/**
 * Assert that an IRI is valid, throw error if not
 */
export function assertValidIri(iri: string, allowRelative: boolean = false): void {
  if (!validateIri(iri, allowRelative)) {
    throw new InvalidIriError(iri, 'IRI format is invalid');
  }
}

/**
 * Validate an RDF term
 */
export function validateRdfTerm(term: RdfTerm): boolean {
  if (!term || typeof term !== 'object') {
    return false;
  }

  if (!term.type || !term.value) {
    return false;
  }

  switch (term.type) {
    case 'uri':
      return validateIri(term.value);
    case 'bnode':
      return BLANK_NODE_PATTERN.test(term.value) || /^[a-zA-Z0-9_-]+$/.test(term.value);
    case 'literal':
      if (term.language && term.datatype) {
        return false; // Cannot have both language and datatype
      }
      if (term.language && !/^[a-z]{2,3}(-[A-Z]{2})?$/.test(term.language)) {
        return false; // Invalid language tag
      }
      if (term.datatype && !validateIri(term.datatype)) {
        return false; // Invalid datatype IRI
      }
      return true;
    default:
      return false;
  }
}

/**
 * Assert that an RDF term is valid
 */
export function assertValidRdfTerm(term: RdfTerm): void {
  if (!validateRdfTerm(term)) {
    throw new RdfError(RdfErrorType.InvalidFormat, `Invalid RDF term: ${JSON.stringify(term)}`);
  }
}

/**
 * Validate an RDF triple
 */
export function validateRdfTriple(triple: RdfTriple): boolean {
  if (!triple || typeof triple !== 'object') {
    return false;
  }

  // Subject must be URI or blank node
  if (!triple.subject || !['uri', 'bnode'].includes(triple.subject.type)) {
    return false;
  }

  // Predicate must be URI
  if (!triple.predicate || triple.predicate.type !== 'uri') {
    return false;
  }

  // Object can be any term type
  if (!triple.object) {
    return false;
  }

  return (
    validateRdfTerm(triple.subject) &&
    validateRdfTerm(triple.predicate) &&
    validateRdfTerm(triple.object) &&
    (!triple.graph || validateRdfTerm(triple.graph))
  );
}

/**
 * Assert that an RDF triple is valid
 */
export function assertValidRdfTriple(triple: RdfTriple): void {
  if (!validateRdfTriple(triple)) {
    throw new RdfError(RdfErrorType.InvalidFormat, `Invalid RDF triple: ${JSON.stringify(triple)}`);
  }
}

/**
 * Validate a SPARQL query
 */
export function validateSparqlQuery(query: string): boolean {
  if (!query || typeof query !== 'string') {
    return false;
  }

  const upperQuery = query.toUpperCase();
  return SPARQL_KEYWORDS.some((keyword) => upperQuery.includes(keyword));
}

/**
 * Assert that a SPARQL query is valid (basic validation)
 */
export function assertValidSparqlQuery(query: string): void {
  if (!validateSparqlQuery(query)) {
    throw new RdfError(RdfErrorType.QueryError, 'Query does not appear to be valid SPARQL');
  }
}

/**
 * Validate RDF format
 */
export function validateRdfFormat(format: string): format is RdfFormat {
  return Object.values(RdfFormat).includes(format as RdfFormat);
}

/**
 * Assert that an RDF format is valid
 */
export function assertValidRdfFormat(format: string): asserts format is RdfFormat {
  if (!validateRdfFormat(format)) {
    throw new InvalidFormatError(format, 'Unknown RDF format');
  }
}

/**
 * Validate query options
 */
export function validateQueryOptions(options: QueryOptions): boolean {
  if (!options || typeof options !== 'object') {
    return false;
  }

  if (
    options.timeout !== undefined &&
    (typeof options.timeout !== 'number' || options.timeout <= 0)
  ) {
    return false;
  }

  if (options.limit !== undefined && (typeof options.limit !== 'number' || options.limit <= 0)) {
    return false;
  }

  if (options.offset !== undefined && (typeof options.offset !== 'number' || options.offset < 0)) {
    return false;
  }

  if (options.defaultGraphUri && !validateIri(options.defaultGraphUri)) {
    return false;
  }

  if (options.namedGraphUris) {
    if (!Array.isArray(options.namedGraphUris)) {
      return false;
    }
    return options.namedGraphUris.every((uri) => validateIri(uri));
  }

  return true;
}

/**
 * Sanitize a literal value for use in SPARQL
 */
export function sanitizeLiteral(value: string): string {
  return value
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t');
}

/**
 * Validate and normalize an IRI
 */
export function normalizeIri(iri: string): string {
  if (!validateIri(iri)) {
    throw new InvalidIriError(iri);
  }

  // Remove angle brackets if present
  if (iri.startsWith('<') && iri.endsWith('>')) {
    return iri.slice(1, -1);
  }

  return iri;
}

/**
 * Check if a string is a valid blank node identifier
 */
export function isBlankNode(value: string): boolean {
  return BLANK_NODE_PATTERN.test(value) || value.startsWith('_:');
}

/**
 * Check if a string is a valid IRI
 */
export function isIri(value: string): boolean {
  return validateIri(value);
}

/**
 * Validate URL for HTTP endpoints
 */
export function validateEndpointUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

/**
 * Assert that an endpoint URL is valid
 */
export function assertValidEndpointUrl(url: string): void {
  if (!validateEndpointUrl(url)) {
    throw new RdfError(RdfErrorType.ConfigError, `Invalid endpoint URL: ${url}`);
  }
}

/**
 * Validate a prefix declaration
 */
export function validatePrefix(prefix: string, iri: string): boolean {
  if (!prefix || typeof prefix !== 'string') {
    return false;
  }

  // Prefix should be alphanumeric (and possibly contain hyphens/underscores)
  if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(prefix)) {
    return false;
  }

  return validateIri(iri);
}

/**
 * Extract query type from SPARQL query
 */
export function getQueryType(
  query: string
): 'SELECT' | 'CONSTRUCT' | 'ASK' | 'DESCRIBE' | 'UPDATE' | 'UNKNOWN' {
  const upperQuery = query.trim().toUpperCase();

  if (upperQuery.startsWith('SELECT')) {
    return 'SELECT';
  }
  if (upperQuery.startsWith('CONSTRUCT')) {
    return 'CONSTRUCT';
  }
  if (upperQuery.startsWith('ASK')) {
    return 'ASK';
  }
  if (upperQuery.startsWith('DESCRIBE')) {
    return 'DESCRIBE';
  }
  if (upperQuery.includes('INSERT') || upperQuery.includes('DELETE')) {
    return 'UPDATE';
  }

  return 'UNKNOWN';
}

/**
 * Check if a query is a read-only query
 */
export function isReadOnlyQuery(query: string): boolean {
  const queryType = getQueryType(query);
  return ['SELECT', 'CONSTRUCT', 'ASK', 'DESCRIBE'].includes(queryType);
}

/**
 * Validate a graph URI
 */
export function validateGraphUri(uri: string | undefined): boolean {
  if (uri === undefined) {
    return true; // undefined is valid (means default graph)
  }
  return validateIri(uri);
}

/**
 * Check if a string contains potential SPARQL injection
 */
export function detectSparqlInjection(input: string): boolean {
  const suspiciousPatterns = [
    /;\s*(INSERT|DELETE|DROP|CLEAR)/i,
    /}\s*;\s*{/i,
    /UNION\s*{\s*{\s*}/i,
  ];

  return suspiciousPatterns.some((pattern) => pattern.test(input));
}

/**
 * Sanitize user input for use in SPARQL queries
 */
export function sanitizeSparqlInput(input: string): string {
  // Remove or escape potentially dangerous characters
  return input.replace(/[{}]/g, '').replace(/;/g, '').trim();
}
