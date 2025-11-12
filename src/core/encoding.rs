//! Binary encoding/decoding utilities for RDF events

use crate::core::{Event, RDFEvent};
use crate::storage::indexing::dictionary::Dictionary;

/// Size of a single encoded record in bytes
/// Reduced from 40 to 24 bytes (40% space savings)
pub const RECORD_SIZE: usize = 24;

/// Encode an RDF event record into a byte buffer
pub fn encode_record(
    buffer: &mut [u8; RECORD_SIZE],
    timestamp: u64,
    subject: u32,
    predicate: u32,
    object: u32,
    graph: u32,
) {
    buffer[0..8].copy_from_slice(&timestamp.to_le_bytes());
    buffer[8..12].copy_from_slice(&subject.to_le_bytes());
    buffer[12..16].copy_from_slice(&predicate.to_le_bytes());
    buffer[16..20].copy_from_slice(&object.to_le_bytes());
    buffer[20..24].copy_from_slice(&graph.to_le_bytes());
}

/// Decode a byte buffer into an RDF event record
pub fn decode_record(buffer: &[u8; RECORD_SIZE]) -> (u64, u32, u32, u32, u32) {
    let timestamp = u64::from_le_bytes(buffer[0..8].try_into().unwrap());
    let subject = u32::from_le_bytes(buffer[8..12].try_into().unwrap());
    let predicate = u32::from_le_bytes(buffer[12..16].try_into().unwrap());
    let object = u32::from_le_bytes(buffer[16..20].try_into().unwrap());
    let graph = u32::from_le_bytes(buffer[20..24].try_into().unwrap());
    (timestamp, subject, predicate, object, graph)
}

impl RDFEvent {
    /// Encode this RDF event to an internal Event using a dictionary
    pub fn encode(&self, dict: &mut Dictionary) -> Event {
        Event {
            timestamp: self.timestamp,
            subject: dict.encode(&self.subject),
            predicate: dict.encode(&self.predicate),
            object: dict.encode(&self.object),
            graph: dict.encode(&self.graph),
        }
    }
}

impl Event {
    /// Decode this internal Event to an RDFEvent using a dictionary
    pub fn decode(&self, dict: &Dictionary) -> RDFEvent {
        RDFEvent {
            timestamp: self.timestamp,
            subject: dict.decode(self.subject).unwrap_or("UNKNOWN").to_string(),
            predicate: dict.decode(self.predicate).unwrap_or("UNKNOWN").to_string(),
            object: dict.decode(self.object).unwrap_or("UNKNOWN").to_string(),
            graph: dict.decode(self.graph).unwrap_or("UNKNOWN").to_string(),
        }
    }

    /// Encode this Event to bytes
    pub fn to_bytes(&self) -> [u8; RECORD_SIZE] {
        let mut buffer = [0u8; RECORD_SIZE];
        encode_record(
            &mut buffer,
            self.timestamp,
            self.subject,
            self.predicate,
            self.object,
            self.graph,
        );
        buffer
    }
}
