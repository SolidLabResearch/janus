//! Query Execution Module
//!
//! This module provides execution engines for both historical and live RDF stream queries.
//!
//! # Components
//!
//! - **HistoricalExecutor** - Executes SPARQL queries over historical data using window operators
//! - **ResultConverter** - Converts execution results to unified QueryResult format
//!
//! # Architecture
//!
//! The execution layer sits between the high-level API (`JanusApi`) and low-level
//! data access primitives (window operators, storage). It orchestrates:
//!
//! 1. Data retrieval via window operators
//! 2. Format conversion (Event → RDFEvent → Quad)
//! 3. SPARQL execution via query engines
//! 4. Result formatting for consumption
//!
//! # Example
//!
//! ```ignore
//! use janus::execution::historical_executor::HistoricalExecutor;
//! use janus::execution::result_converter::ResultConverter;
//!
//! // Create historical executor
//! let executor = HistoricalExecutor::new(storage, sparql_engine);
//!
//! // Execute query
//! let bindings = executor.execute_fixed_window(&window_def, sparql_query)?;
//!
//! // Convert to QueryResult
//! let converter = ResultConverter::new(query_id);
//! let result = converter.from_historical_bindings(bindings, timestamp);
//! ```

pub mod historical_executor;
pub mod result_converter;

// Re-export main types for convenience
pub use historical_executor::HistoricalExecutor;
pub use result_converter::ResultConverter;
