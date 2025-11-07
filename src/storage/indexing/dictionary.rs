use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use bincode;
use serde::{Deserialize, Serialize};

use crate::core::Event;

#[derive(Debug, Serialize, Deserialize)]
pub struct Dictionary {
    pub string_to_id: HashMap<String, u32>,
    pub id_to_uri: HashMap<u32, String>,
    pub next_id: u32,
}

impl Dictionary {
    pub fn new() -> Self {
        Dictionary { string_to_id: HashMap::new(), id_to_uri: HashMap::new(), next_id: 0 }
    }

    pub fn encode(&mut self, value: &str) -> u32 {
        if let Some(&id) = self.string_to_id.get(value) {
            id
        } else {
            let id = self.next_id;
            self.string_to_id.insert(value.to_string(), id);
            self.id_to_uri.insert(id, value.to_string());
            self.next_id += 1;
            id
        }
    }

    pub fn decode(&self, id: u32) -> Option<&str> {
        self.id_to_uri.get(&id).map(|s| s.as_str())
    }

    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let encoded = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = File::create(path)?;
        file.write_all(&encoded)?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let dict: Dictionary = bincode::deserialize(&buffer)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(dict)
    }

    pub fn decode_graph(&self, event: &Event) -> String {
        let subject = self.decode(event.subject).unwrap_or("unknown");
        let predicate = self.decode(event.predicate).unwrap_or("unknown");
        let object = self.decode(event.object).unwrap_or("unknown");
        let graph = self.decode(event.graph).unwrap_or("unknown");

        format!(
            "<(<{}>, <{}>, <{}>, <{}>), {}>",
            subject, predicate, object, graph, event.timestamp
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Event;

    #[test]
    fn test_dictionary_encoding_decoding() {
        let mut dict = Dictionary::new();

        // Encode some RDF terms
        let subject_id = dict.encode("http://example.org/person/Alice");
        let predicate_id = dict.encode("http://example.org/knows");
        let object_id = dict.encode("http://example.org/person/Bob");
        let graph_id = dict.encode("http://example.org/graph1");

        println!("Encoded IDs:");
        println!("Subject: {} -> {}", "http://example.org/person/Alice", subject_id);
        println!("Predicate: {} -> {}", "http://example.org/knows", predicate_id);
        println!("Object: {} -> {}", "http://example.org/person/Bob", object_id);
        println!("Graph: {} -> {}", "http://example.org/graph1", graph_id);

        // Create an event
        let event = Event {
            timestamp: 1234567890,
            subject: subject_id,
            predicate: predicate_id,
            object: object_id,
            graph: graph_id,
        };

        // Decode the event
        let decoded = dict.decode_graph(&event);
        println!("\nDecoded event: {}", decoded);

        // Verify individual decodings
        assert_eq!(dict.decode(subject_id), Some("http://example.org/person/Alice"));
        assert_eq!(dict.decode(predicate_id), Some("http://example.org/knows"));
        assert_eq!(dict.decode(object_id), Some("http://example.org/person/Bob"));
        assert_eq!(dict.decode(graph_id), Some("http://example.org/graph1"));

        // Test that the decoded string contains the expected format
        assert!(decoded.contains("http://example.org/person/Alice"));
        assert!(decoded.contains("http://example.org/knows"));
        assert!(decoded.contains("http://example.org/person/Bob"));
        assert!(decoded.contains("http://example.org/graph1"));
        assert!(decoded.contains("1234567890"));
    }

    #[test]
    fn test_clean_rdf_api() {
        use crate::core::RDFEvent;

        let mut dict = Dictionary::new();

        // Test the clean API - user provides URIs directly
        let rdf_event = RDFEvent::new(
            1234567890,
            "http://example.org/person/Alice",
            "http://example.org/knows",
            "http://example.org/person/Bob",
            "http://example.org/graph1",
        );

        // Encoding happens internally
        let encoded_event = rdf_event.encode(&mut dict);

        // Decoding happens internally
        let decoded_event = encoded_event.decode(&dict);

        // Verify the round-trip works
        assert_eq!(decoded_event.subject, "http://example.org/person/Alice");
        assert_eq!(decoded_event.predicate, "http://example.org/knows");
        assert_eq!(decoded_event.object, "http://example.org/person/Bob");
        assert_eq!(decoded_event.graph, "http://example.org/graph1");
        assert_eq!(decoded_event.timestamp, 1234567890);

        println!("✅ Clean API test passed!");
        println!(
            "Original: {} {} {} in {} at timestamp {}",
            rdf_event.subject,
            rdf_event.predicate,
            rdf_event.object,
            rdf_event.graph,
            rdf_event.timestamp
        );
        println!(
            "Encoded IDs: {} {} {} {} at timestamp {}",
            encoded_event.subject,
            encoded_event.predicate,
            encoded_event.object,
            encoded_event.graph,
            encoded_event.timestamp
        );
        println!(
            "Decoded: {} {} {} in {} at timestamp {}",
            decoded_event.subject,
            decoded_event.predicate,
            decoded_event.object,
            decoded_event.graph,
            decoded_event.timestamp
        );
    }
}
