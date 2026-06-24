use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use bincode;
use serde::{Deserialize, Serialize};

use crate::core::Event;

use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
const OBJECT_TERM_PREFIX: &str = "__JANUS_TERM_V1__";

#[derive(Debug, Serialize, Deserialize)]
struct EncodedObjectTerm {
    value: String,
    datatype: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dictionary {
    pub string_to_id: HashMap<String, u32>,
    pub id_to_uri: HashMap<u32, String>,
    pub next_id: u32,
}

impl Dictionary {
    pub fn looks_like_iri(s: &str) -> bool {
        if s.is_empty() || s.contains(' ') || s.contains('"') {
            return false;
        }
        let mut chars = s.chars();
        if let Some(first) = chars.next() {
            if !first.is_ascii_alphabetic() {
                return false;
            }
        } else {
            return false;
        }

        let mut has_colon = false;
        for c in chars {
            if c == ':' {
                has_colon = true;
                break;
            }
            if !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '.' {
                return false;
            }
        }
        has_colon
    }

    pub fn encode_object_term(value: &str, is_literal: bool, datatype: Option<&str>) -> String {
        if is_literal {
            let payload = EncodedObjectTerm {
                value: value.to_string(),
                datatype: datatype.map(std::string::ToString::to_string),
            };
            let encoded = serde_json::to_string(&payload)
                .expect("object-term JSON serialization should not fail");
            return format!("{OBJECT_TERM_PREFIX}{encoded}");
        }

        value.to_string()
    }

    pub fn decode_object_term(encoded: &str) -> (String, bool, Option<String>) {
        if let Some(payload) = encoded.strip_prefix(OBJECT_TERM_PREFIX) {
            if let Ok(term) = serde_json::from_str::<EncodedObjectTerm>(payload) {
                return (term.value, true, term.datatype);
            }
        }

        // Legacy dictionary values never stored explicit literal metadata.
        // For those older entries we can only infer IRI-vs-literal heuristically.
        let is_literal = !Self::looks_like_iri(encoded);
        (encoded.to_string(), is_literal, None)
    }

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

    pub fn size(&self) -> usize {
        self.string_to_id.len()
    }

    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let encoded = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Dictionary target path must have a parent directory",
            )
        })?;

        let pid = std::process::id();
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_filename = format!("dictionary.bin.{}.{}.tmp", pid, counter);
        let temp_path = parent.join(temp_filename);

        // Scope to ensure the temp file is closed before renaming
        let write_result = (|| -> std::io::Result<()> {
            let mut file = File::create(&temp_path)?;
            file.write_all(&encoded)?;
            file.flush()?;
            file.sync_all()?;
            Ok(())
        })();

        if let Err(e) = write_result {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }

        if let Err(e) = std::fs::rename(&temp_path, path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }

        // Best-effort Unix parent directory sync
        #[cfg(unix)]
        {
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }

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
