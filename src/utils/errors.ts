/**
 * Error handling utilities for Janus RDF Framework
 */

import { RdfErrorType } from '../core/types';

/**
 * Custom RDF Error class
 */
export class RdfError extends Error {
  public readonly type: RdfErrorType;
  public readonly cause?: Error;

  constructor(type: RdfErrorType, message: string, cause?: Error) {
    super(message);
    this.name = 'RdfError';
    this.type = type;
    this.cause = cause;

    // Maintains proper stack trace for where our error was thrown (only available on V8)
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, RdfError);
    }
  }

  /**
   * Convert error to JSON representation
   */
  toJSON(): Record<string, unknown> {
    return {
      name: this.name,
      type: this.type,
      message: this.message,
      stack: this.stack,
      cause: this.cause?.message,
    };
  }

  /**
   * Get a user-friendly error message
   */
  getUserMessage(): string {
    switch (this.type) {
      case RdfErrorType.ParseError:
        return `Failed to parse RDF data: ${this.message}`;
      case RdfErrorType.QueryError:
        return `SPARQL query error: ${this.message}`;
      case RdfErrorType.SerializationError:
        return `Failed to serialize RDF data: ${this.message}`;
      case RdfErrorType.StoreError:
        return `RDF store error: ${this.message}`;
      case RdfErrorType.HttpError:
        return `HTTP communication error: ${this.message}`;
      case RdfErrorType.InvalidIri:
        return `Invalid IRI: ${this.message}`;
      case RdfErrorType.InvalidFormat:
        return `Invalid format: ${this.message}`;
      case RdfErrorType.IoError:
        return `I/O error: ${this.message}`;
      case RdfErrorType.ConfigError:
        return `Configuration error: ${this.message}`;
      case RdfErrorType.NotFound:
        return `Not found: ${this.message}`;
      default:
        return `Error: ${this.message}`;
    }
  }

  /**
   * Check if error is recoverable
   */
  isRecoverable(): boolean {
    switch (this.type) {
      case RdfErrorType.HttpError:
      case RdfErrorType.IoError:
        return true;
      case RdfErrorType.ConfigError:
      case RdfErrorType.InvalidIri:
      case RdfErrorType.InvalidFormat:
        return false;
      default:
        return false;
    }
  }
}

/**
 * Parse error - thrown when RDF parsing fails
 */
export class ParseError extends RdfError {
  constructor(message: string, cause?: Error) {
    super(RdfErrorType.ParseError, message, cause);
    this.name = 'ParseError';
  }
}

/**
 * Query error - thrown when SPARQL query execution fails
 */
export class QueryError extends RdfError {
  constructor(message: string, cause?: Error) {
    super(RdfErrorType.QueryError, message, cause);
    this.name = 'QueryError';
  }
}

/**
 * Serialization error - thrown when RDF serialization fails
 */
export class SerializationError extends RdfError {
  constructor(message: string, cause?: Error) {
    super(RdfErrorType.SerializationError, message, cause);
    this.name = 'SerializationError';
  }
}

/**
 * Store error - thrown when RDF store operations fail
 */
export class StoreError extends RdfError {
  constructor(message: string, cause?: Error) {
    super(RdfErrorType.StoreError, message, cause);
    this.name = 'StoreError';
  }
}

/**
 * HTTP error - thrown when HTTP requests fail
 */
export class HttpError extends RdfError {
  public readonly statusCode?: number;

  constructor(message: string, statusCode?: number, cause?: Error) {
    super(RdfErrorType.HttpError, message, cause);
    this.name = 'HttpError';
    this.statusCode = statusCode;
  }

  toJSON(): Record<string, unknown> {
    return {
      ...super.toJSON(),
      statusCode: this.statusCode,
    };
  }
}

/**
 * Invalid IRI error
 */
export class InvalidIriError extends RdfError {
  constructor(iri: string, reason?: string) {
    const message = reason ? `Invalid IRI "${iri}": ${reason}` : `Invalid IRI: ${iri}`;
    super(RdfErrorType.InvalidIri, message);
    this.name = 'InvalidIriError';
  }
}

/**
 * Invalid format error
 */
export class InvalidFormatError extends RdfError {
  constructor(format: string, reason?: string) {
    const message = reason ? `Invalid format "${format}": ${reason}` : `Invalid format: ${format}`;
    super(RdfErrorType.InvalidFormat, message);
    this.name = 'InvalidFormatError';
  }
}

/**
 * Configuration error
 */
export class ConfigError extends RdfError {
  constructor(message: string, cause?: Error) {
    super(RdfErrorType.ConfigError, message, cause);
    this.name = 'ConfigError';
  }
}

/**
 * Not found error
 */
export class NotFoundError extends RdfError {
  constructor(resource: string) {
    super(RdfErrorType.NotFound, `Resource not found: ${resource}`);
    this.name = 'NotFoundError';
  }
}

/**
 * Error handler utility functions
 */
export class ErrorHandler {
  /**
   * Wrap an async function with error handling
   */
  static async wrap<T>(fn: () => Promise<T>, errorType: RdfErrorType, message: string): Promise<T> {
    try {
      return await fn();
    } catch (error) {
      if (error instanceof RdfError) {
        throw error;
      }
      throw new RdfError(errorType, message, error instanceof Error ? error : undefined);
    }
  }

  /**
   * Convert unknown error to RdfError
   */
  static toRdfError(error: unknown, defaultType: RdfErrorType = RdfErrorType.Other): RdfError {
    if (error instanceof RdfError) {
      return error;
    }

    if (error instanceof Error) {
      return new RdfError(defaultType, error.message, error);
    }

    return new RdfError(defaultType, String(error));
  }

  /**
   * Handle error with retry logic
   */
  static async withRetry<T>(
    fn: () => Promise<T>,
    maxRetries: number = 3,
    delayMs: number = 1000
  ): Promise<T> {
    let lastError: Error | undefined;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        return await fn();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        // Don't retry if error is not recoverable
        if (error instanceof RdfError && !error.isRecoverable()) {
          throw error;
        }

        // If this was the last attempt, throw the error
        if (attempt === maxRetries) {
          throw lastError;
        }

        // Wait before retrying with exponential backoff
        await new Promise((resolve) => setTimeout(resolve, delayMs * Math.pow(2, attempt)));
      }
    }

    throw lastError || new Error('Retry failed');
  }

  /**
   * Log error with context
   */
  static logError(error: unknown, context?: string): void {
    const rdfError = ErrorHandler.toRdfError(error);
    const prefix = context ? `[${context}] ` : '';
    console.error(`${prefix}${rdfError.getUserMessage()}`);
    if (rdfError.stack) {
      console.error(rdfError.stack);
    }
  }
}

/**
 * Assert that a condition is true, throw error if not
 */
export function assert(
  condition: boolean,
  message: string,
  errorType?: RdfErrorType
): asserts condition {
  if (!condition) {
    throw new RdfError(errorType || RdfErrorType.Other, message);
  }
}

/**
 * Assert that a value is not null or undefined
 */
export function assertDefined<T>(
  value: T | null | undefined,
  message: string,
  errorType?: RdfErrorType
): asserts value is T {
  if (value === null || value === undefined) {
    throw new RdfError(errorType || RdfErrorType.Other, message);
  }
}
