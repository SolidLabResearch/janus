export * from './core/types';

// Adapter exports
export * from './adapters/OxigraphAdapter';
export * from './adapters/JenaAdapter';

// Utility exports
export * from './utils/logger';
export * from './utils/validators';

/**
 * Version information
 */
export const VERSION = '0.1.0';

/**
 * Initialize the RDF framework
 * This function should be called before using any RDF operations
 */
export async function initialize(): Promise<void> {
  // Initialization logic can be added here
  // For example, setting up connections, etc.
  console.info('Janus RDF Framework initialized');
}

/**
 * Default configuration
 */
export const defaultConfig = {
  defaultFormat: 'turtle' as const,
  defaultTimeout: 30000,
  logLevel: 'info' as const,
};
