//! Janus RDF Rust Library
//!
//! This library provides RDF data store integration with WASM support for use in TypeScript applications.
//! It includes bindings for Oxigraph and utilities for interacting with RDF data stores.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

pub mod error;
pub mod http_client;
pub mod parser;
pub mod query;
pub mod store;

pub use error::{RdfError, Result};
pub use parser::{RdfParser, RdfSerializer};
pub use query::{QueryExecutor, QueryResult};
pub use store::RdfStore;

#[cfg(feature = "http-client")]
pub use http_client::{HttpRdfClient, RdfStoreEndpoint};

/// Initialize the WASM module and set up panic hook for better error messages
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    // WASM logging initialization - disabled for now
    // #[cfg(target_arch = "wasm32")]
    // wasm_logger::init(wasm_logger::Config::default());
}

/// Configuration for RDF operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct RdfConfig {
    base_iri: Option<String>,
    limit: Option<u32>,
    reasoning: bool,
}

#[wasm_bindgen]
impl RdfConfig {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            base_iri: None,
            limit: None,
            reasoning: false,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn base_iri(&self) -> Option<String> {
        self.base_iri.clone()
    }

    #[wasm_bindgen(setter)]
    pub fn set_base_iri(&mut self, iri: Option<String>) {
        self.base_iri = iri;
    }

    #[wasm_bindgen(getter)]
    pub fn limit(&self) -> Option<u32> {
        self.limit
    }

    #[wasm_bindgen(setter)]
    pub fn set_limit(&mut self, limit: Option<u32>) {
        self.limit = limit;
    }

    #[wasm_bindgen(getter)]
    pub fn reasoning(&self) -> bool {
        self.reasoning
    }

    #[wasm_bindgen(setter)]
    pub fn set_reasoning(&mut self, reasoning: bool) {
        self.reasoning = reasoning;
    }
}

impl Default for RdfConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// RDF serialization formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RdfFormat {
    Turtle,
    NTriples,
    RdfXml,
    JsonLd,
    NQuads,
    TriG,
}

impl RdfFormat {
    pub fn from_string(format: &str) -> Option<RdfFormat> {
        match format.to_lowercase().as_str() {
            "turtle" | "ttl" => Some(RdfFormat::Turtle),
            "ntriples" | "nt" => Some(RdfFormat::NTriples),
            "rdfxml" | "rdf" | "xml" => Some(RdfFormat::RdfXml),
            "jsonld" | "json-ld" | "json" => Some(RdfFormat::JsonLd),
            "nquads" | "nq" => Some(RdfFormat::NQuads),
            "trig" => Some(RdfFormat::TriG),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            RdfFormat::Turtle => "turtle".to_string(),
            RdfFormat::NTriples => "ntriples".to_string(),
            RdfFormat::RdfXml => "rdfxml".to_string(),
            RdfFormat::JsonLd => "jsonld".to_string(),
            RdfFormat::NQuads => "nquads".to_string(),
            RdfFormat::TriG => "trig".to_string(),
        }
    }

    pub fn media_type(&self) -> String {
        match self {
            RdfFormat::Turtle => "text/turtle".to_string(),
            RdfFormat::NTriples => "application/n-triples".to_string(),
            RdfFormat::RdfXml => "application/rdf+xml".to_string(),
            RdfFormat::JsonLd => "application/ld+json".to_string(),
            RdfFormat::NQuads => "application/n-quads".to_string(),
            RdfFormat::TriG => "application/trig".to_string(),
        }
    }
}

/// SPARQL query result formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryResultFormat {
    Json,
    Xml,
    Csv,
    Tsv,
}

impl QueryResultFormat {
    pub fn from_string(format: &str) -> Option<QueryResultFormat> {
        match format.to_lowercase().as_str() {
            "json" => Some(QueryResultFormat::Json),
            "xml" => Some(QueryResultFormat::Xml),
            "csv" => Some(QueryResultFormat::Csv),
            "tsv" => Some(QueryResultFormat::Tsv),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            QueryResultFormat::Json => "json".to_string(),
            QueryResultFormat::Xml => "xml".to_string(),
            QueryResultFormat::Csv => "csv".to_string(),
            QueryResultFormat::Tsv => "tsv".to_string(),
        }
    }

    pub fn media_type(&self) -> String {
        match self {
            QueryResultFormat::Json => "application/sparql-results+json".to_string(),
            QueryResultFormat::Xml => "application/sparql-results+xml".to_string(),
            QueryResultFormat::Csv => "text/csv".to_string(),
            QueryResultFormat::Tsv => "text/tab-separated-values".to_string(),
        }
    }
}

/// Utility function to log messages from WASM
#[wasm_bindgen]
pub fn log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&message.into());

    #[cfg(not(target_arch = "wasm32"))]
    println!("{}", message);
}

/// Get version information
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdf_format_conversion() {
        assert_eq!(RdfFormat::from_string("turtle"), Some(RdfFormat::Turtle));
        assert_eq!(RdfFormat::from_string("ttl"), Some(RdfFormat::Turtle));
        assert_eq!(RdfFormat::from_string("json-ld"), Some(RdfFormat::JsonLd));
        assert_eq!(RdfFormat::from_string("invalid"), None);
    }

    #[test]
    fn test_rdf_format_media_type() {
        assert_eq!(RdfFormat::Turtle.media_type(), "text/turtle");
        assert_eq!(RdfFormat::JsonLd.media_type(), "application/ld+json");
    }

    #[test]
    fn test_query_result_format() {
        assert_eq!(
            QueryResultFormat::from_string("json"),
            Some(QueryResultFormat::Json)
        );
        assert_eq!(
            QueryResultFormat::Json.media_type(),
            "application/sparql-results+json"
        );
    }

    #[test]
    fn test_rdf_config_default() {
        let config = RdfConfig::default();
        assert_eq!(config.base_iri, None);
        assert_eq!(config.limit, None);
        assert!(!config.reasoning);
    }
}
