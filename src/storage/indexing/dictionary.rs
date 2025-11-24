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
