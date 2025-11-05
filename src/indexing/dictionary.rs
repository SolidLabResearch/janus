use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug)]
pub struct Dictionary {
    uri_to_id: HashMap<String, u64>,
    id_to_uri: Vec<String>,
    next_id: u64,
}

impl Dictionary {
    pub fn new() -> Self {
        Self { uri_to_id: HashMap::new(), id_to_uri: Vec::new(), next_id: 0 }
    }

    pub fn fetch_id(&mut self, uri: &str) -> u64 {
        if let Some(&id) = self.uri_to_id.get(uri) {
            id
        } else {
            let id = self.next_id;
            self.uri_to_id.insert(uri.to_string(), id);
            self.id_to_uri.push(uri.to_string());
            self.next_id += 1;
            id
        }
    }

    pub fn fetch_uri(&self, id: u64) -> Option<&str> {
        self.id_to_uri.get(id as usize).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.uri_to_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.uri_to_id.is_empty()
    }

    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(&(self.id_to_uri.len() as u64).to_be_bytes())?;

        for uri in &self.id_to_uri {
            let uri_bytes = uri.as_bytes();
            file.write_all(&(uri_bytes.len() as u32).to_be_bytes())?;
            file.write_all(uri_bytes)?;
        }
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let mut uri_to_id = HashMap::new();
        let mut id_to_uri = Vec::new();

        // Reading the number of entries
        let mut count_bytes = [0u8; 8];
        file.read_exact(&mut count_bytes)?;
        let count = u64::from_be_bytes(count_bytes);

        // Reading each IRI Entry

        for id in 0..count {
            let mut len_bytes = [0u8; 4];
            file.read_exact(&mut len_bytes)?;

            let length = u32::from_be_bytes(len_bytes) as usize;
            let mut uri_bytes = vec![0u8; length];
            file.read_exact(&mut uri_bytes)?;
            let uri = String::from_utf8(uri_bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            uri_to_id.insert(uri.clone(), id);
            id_to_uri.push(uri);
        }

        Ok(Self { uri_to_id, id_to_uri, next_id: count })
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}
