//! Error types for RDF operations

use oxigraph::store::LoaderError;
use oxrdfio;
use std::fmt;
use thiserror::Error;
use wasm_bindgen::prelude::*;

/// Result type alias for RDF operations
pub type Result<T> = std::result::Result<T, RdfError>;

/// Main error type for RDF operations
#[derive(Error, Debug)]
pub enum RdfError {
    /// Parse error when reading RDF data
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Query execution error
    #[error("Query error: {0}")]
    QueryError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Store error
    #[error("Store error: {0}")]
    StoreError(String),

    /// HTTP client error
    #[cfg(feature = "http-client")]
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// Invalid IRI
    #[error("Invalid IRI: {0}")]
    InvalidIri(String),

    /// Invalid format
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),

    /// Other error
    #[error("Error: {0}")]
    Other(String),
}

impl From<oxigraph::store::StorageError> for RdfError {
    fn from(err: oxigraph::store::StorageError) -> Self {
        RdfError::StoreError(err.to_string())
    }
}

impl From<oxigraph::sparql::EvaluationError> for RdfError {
    fn from(err: oxigraph::sparql::EvaluationError) -> Self {
        RdfError::QueryError(err.to_string())
    }
}

impl From<oxrdfio::RdfParseError> for RdfError {
    fn from(err: oxrdfio::RdfParseError) -> Self {
        RdfError::ParseError(err.to_string())
    }
}

impl From<LoaderError> for RdfError {
    fn from(err: LoaderError) -> Self {
        RdfError::ParseError(err.to_string())
    }
}

impl From<oxrdf::IriParseError> for RdfError {
    fn from(err: oxrdf::IriParseError) -> Self {
        RdfError::InvalidIri(err.to_string())
    }
}

impl From<std::io::Error> for RdfError {
    fn from(err: std::io::Error) -> Self {
        RdfError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for RdfError {
    fn from(err: serde_json::Error) -> Self {
        RdfError::SerializationError(err.to_string())
    }
}

#[cfg(feature = "http-client")]
impl From<reqwest::Error> for RdfError {
    fn from(err: reqwest::Error) -> Self {
        RdfError::HttpError(err.to_string())
    }
}

impl From<anyhow::Error> for RdfError {
    fn from(err: anyhow::Error) -> Self {
        RdfError::Other(err.to_string())
    }
}

// WASM-compatible error wrapper
#[wasm_bindgen]
pub struct WasmRdfError {
    message: String,
    kind: String,
}

#[wasm_bindgen]
impl WasmRdfError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn to_string(&self) -> String {
        format!("{}: {}", self.kind, self.message)
    }
}

impl From<RdfError> for WasmRdfError {
    fn from(err: RdfError) -> Self {
        let kind = match &err {
            RdfError::ParseError(_) => "ParseError",
            RdfError::QueryError(_) => "QueryError",
            RdfError::SerializationError(_) => "SerializationError",
            RdfError::StoreError(_) => "StoreError",
            #[cfg(feature = "http-client")]
            RdfError::HttpError(_) => "HttpError",
            RdfError::InvalidIri(_) => "InvalidIri",
            RdfError::InvalidFormat(_) => "InvalidFormat",
            RdfError::IoError(_) => "IoError",
            RdfError::ConfigError(_) => "ConfigError",
            RdfError::NotFound(_) => "NotFound",
            RdfError::Other(_) => "Other",
        };

        WasmRdfError {
            message: err.to_string(),
            kind: kind.to_string(),
        }
    }
}

impl From<RdfError> for JsValue {
    fn from(err: RdfError) -> Self {
        let wasm_err: WasmRdfError = err.into();
        JsValue::from_str(&wasm_err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = RdfError::ParseError("test error".to_string());
        assert_eq!(err.to_string(), "Parse error: test error");
    }

    #[test]
    fn test_wasm_error_conversion() {
        let err = RdfError::QueryError("test query error".to_string());
        let wasm_err: WasmRdfError = err.into();
        assert_eq!(wasm_err.kind(), "QueryError");
        assert!(wasm_err.message().contains("test query error"));
    }
}
