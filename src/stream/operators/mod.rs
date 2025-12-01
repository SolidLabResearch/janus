//! Stream Operators Module
//!
//! This module provides operators for processing RDF streams, including
//! window operators for historical data access.
//!
//! # Window Operators
//!
//! - **HistoricalFixedWindowOperator** - Queries a single fixed time range
//! - **HistoricalSlidingWindowOperator** - Queries multiple sliding windows
//!
//! # Example
//!
//! ```ignore
//! use janus::stream::operators::historical_fixed_window::HistoricalFixedWindowOperator;
//!
//! let operator = HistoricalFixedWindowOperator::new(storage, window_def);
//!
//! // Get events from the fixed window
//! if let Some(events) = operator.into_iter().next() {
//!     println!("Retrieved {} events", events.len());
//! }
//! ```

pub mod historical_fixed_window;
pub mod historical_sliding_window;
pub mod hs2r;

// Re-export main types for convenience
pub use historical_fixed_window::HistoricalFixedWindowOperator;
pub use historical_sliding_window::HistoricalSlidingWindowOperator;
