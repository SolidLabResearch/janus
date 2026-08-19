//! Core data structures and types for Janus RDF Stream Processing Engine

/// Internal storage event with encoded IDs
/// Uses u32 for dictionary IDs (4B max) and u64 for timestamp (milliseconds)
/// Total: 24 bytes vs 40 bytes (40% space savings)
#[derive(Clone, Debug)]
pub struct Event {
    pub timestamp: u64, // 8 bytes - milliseconds since epoch
    pub subject: u32,   // 4 bytes - dictionary-encoded (4B max unique strings)
    pub predicate: u32, // 4 bytes - dictionary-encoded (usually <1000 unique)
    pub object: u32,    // 4 bytes - dictionary-encoded (4B max unique strings)
    pub graph: u32,     // 4 bytes - dictionary-encoded (usually <100 unique)
}

/// User-facing RDF event with URI strings which is presented to client requesting for the data.
#[derive(Debug, Clone)]
pub struct RDFEvent {
    pub timestamp: u64,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub graph: String,
    pub object_is_literal: bool,
    pub object_datatype: Option<String>,
}

/// Implement methods for RDFEvent struct.
impl RDFEvent {
    pub fn new(timestamp: u64, subject: &str, predicate: &str, object: &str, graph: &str) -> Self {
        let is_literal = !crate::storage::indexing::dictionary::Dictionary::looks_like_iri(object);
        Self {
            timestamp,
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            graph: graph.to_string(),
            object_is_literal: is_literal,
            object_datatype: None,
        }
    }

    pub fn new_iri_object(
        timestamp: u64,
        subject: &str,
        predicate: &str,
        object_iri: &str,
        graph: &str,
    ) -> Self {
        Self {
            timestamp,
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object_iri.to_string(),
            graph: graph.to_string(),
            object_is_literal: false,
            object_datatype: None,
        }
    }

    pub fn new_literal_object(
        timestamp: u64,
        subject: &str,
        predicate: &str,
        object_value: &str,
        graph: &str,
    ) -> Self {
        Self {
            timestamp,
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object_value.to_string(),
            graph: graph.to_string(),
            object_is_literal: true,
            object_datatype: None,
        }
    }

    pub fn new_typed_literal_object(
        timestamp: u64,
        subject: &str,
        predicate: &str,
        object_value: &str,
        datatype_uri: &str,
        graph: &str,
    ) -> Self {
        Self {
            timestamp,
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object_value.to_string(),
            graph: graph.to_string(),
            object_is_literal: true,
            object_datatype: Some(datatype_uri.to_string()),
        }
    }
}

pub mod encoding;
pub use encoding::*;
