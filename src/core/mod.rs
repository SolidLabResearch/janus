//! Core data structures and types for Janus RDF Stream Processing Engine

/// Internal storage event with encoded IDs
#[derive(Clone, Debug)]
pub struct Event {
    pub timestamp: u64,
    pub subject: u64,
    pub predicate: u64,
    pub object: u64,
    pub graph: u64,
}

/// User-facing RDF event with URI strings
#[derive(Debug, Clone)]
pub struct RDFEvent {
    pub timestamp: u64,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub graph: String,
}

impl RDFEvent {
    pub fn new(timestamp: u64, subject: &str, predicate: &str, object: &str, graph: &str) -> Self {
        Self {
            timestamp,
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            graph: graph.to_string(),
        }
    }
}

pub mod encoding;
pub use encoding::*;
