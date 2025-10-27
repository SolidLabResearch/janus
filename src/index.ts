/**
 * Janus RDF Template - Main Entry Point
 * TypeScript + Rust hybrid architecture for RDF data store integration
 */

// Core exports
// Core functionality exports will be added when implemented
// export * from './core/RdfStore';
// export * from './core/QueryExecutor';
// export * from './core/RdfParser';
export * from './core/types';

// Adapter exports
export * from './adapters/OxigraphAdapter';
export * from './adapters/JenaAdapter';
// WASM adapter will be added when Rust integration is complete
export * from './adapters/WasmAdapter';

// Utility exports
export * from './utils/logger';
// Error utilities are already exported from types
// export * from './utils/errors';
export * from './utils/validators';

// Re-export WASM bindings (when available)
// Types are already exported via export * from './core/types'

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
  // For example, loading WASM modules, setting up connections, etc.
  console.info('Janus RDF Framework initialized');
}

/**
 * Default configuration
 */
export const defaultConfig = {
  enableWasm: true,
  defaultFormat: 'turtle' as const,
  defaultTimeout: 30000,
  logLevel: 'info' as const,
};
