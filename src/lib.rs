//! # Janus
//!
//! Janus is a hybrid engine for unified Live and Historical RDF Stream Processing.
//!
//! The name "Janus" is inspired by the Roman deity Janus who is the guardian of
//! doorways and transitions, and looks towards both the past and the future
//! simultaneously. This dual perspective reflects Janus's capability to process
//! both Historical and Live RDF streams in a unified manner utilizing a single
//! query language and engine.
//!
//! ## Features
//!
//! - Support for RDF stream processing
//! - Integration with multiple RDF stores
//! - Unified query interface for historical and live data
//!
//! ## Example
//!
//! ```rust
//! use janus::Result;
//!
//! fn example() -> Result<()> {
//!     println!("Janus RDF Stream Processing Engine");
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(clippy::empty_docs)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::new_without_default)]
#![allow(clippy::mixed_attributes_style)]
#![allow(clippy::empty_line_after_outer_attr)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::unused_self)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::case_sensitive_file_extension_comparisons)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::if_not_else)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::identity_op)]
#![allow(clippy::needless_update)]
#![allow(missing_docs)]

/// Core data structures and types
pub mod core;

/// Module for handling RDF stores
pub mod store {
    //! RDF store implementations and interfaces
}

/// Module for stream processing
pub mod stream {
    //! RDF stream processing functionality
}

/// Module for query parsing and execution
pub mod query {
    //! Query language parser and executor
}

/// Module for configuration management
pub mod config {
    //! Configuration structures and utilities
}

/// Module for parsing JanusQL queries
pub mod parsing;

pub mod storage;

pub mod error {
    //! Error types and result definitions

    use std::fmt;

    /// Result type alias for Janus operations
    pub type Result<T> = std::result::Result<T, Error>;

    /// Main error type for Janus
    #[derive(Debug)]
    pub enum Error {
        /// Configuration error
        Config(String),
        /// Store error
        Store(String),
        /// Stream error
        Stream(String),
        /// Query error
        Query(String),
        /// IO error
        Io(std::io::Error),
        /// Other error
        Other(String),
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Error::Config(msg) => write!(f, "Configuration error: {}", msg),
                Error::Store(msg) => write!(f, "Store error: {}", msg),
                Error::Stream(msg) => write!(f, "Stream error: {}", msg),
                Error::Query(msg) => write!(f, "Query error: {}", msg),
                Error::Io(err) => write!(f, "IO error: {}", err),
                Error::Other(msg) => write!(f, "Error: {}", msg),
            }
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Error::Io(err) => Some(err),
                _ => None,
            }
        }
    }

    impl From<std::io::Error> for Error {
        fn from(err: std::io::Error) -> Self {
            Error::Io(err)
        }
    }
}

// Re-export commonly used types
pub use error::{Error, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::Config("test error".to_string());
        assert_eq!(format!("{}", err), "Configuration error: test error");
    }
}
