use std::fs::File;
use std::io::Write;
use crate::indexing::dictionary::Dictionary;

#[doc = ""]
pub const RECORD_SIZE: usize = 40;

#[doc = ""]
pub fn encode_record(
    buffer: &mut [u8; RECORD_SIZE],
    timestamp: u64,
    subject: u64,
    predicate: u64,
    object: u64,
    graph: u64,
) {
    buffer[0..8].copy_from_slice(&timestamp.to_le_bytes());
    buffer[8..16].copy_from_slice(&subject.to_le_bytes());
    buffer[16..24].copy_from_slice(&predicate.to_le_bytes());
    buffer[24..32].copy_from_slice(&object.to_le_bytes());
    buffer[32..40].copy_from_slice(&graph.to_le_bytes());
}

#[doc = ""]
pub fn decode_record(buffer: &[u8; RECORD_SIZE]) -> (u64, u64, u64, u64, u64) {
    let timestamp = u64::from_le_bytes(buffer[0..8].try_into().unwrap());
    let subject = u64::from_le_bytes(buffer[8..16].try_into().unwrap());
    let predicate = u64::from_le_bytes(buffer[16..24].try_into().unwrap());
    let object = u64::from_le_bytes(buffer[24..32].try_into().unwrap());
    let graph = u64::from_le_bytes(buffer[32..40].try_into().unwrap());
    (timestamp, subject, predicate, object, graph)
}

#[doc = ""]
pub struct LogWriter {
    log_file: File,
    record_count: u64,
}

#[doc = ""]
impl LogWriter {
    #[doc = ""]
    pub fn create(path: &str) -> std::io::Result<Self> {
        let log_file = match File::create(path) {
            Ok(file) => file,
            Err(error) => {
                return Err(error);
            }
        };
        Ok(Self { log_file, record_count: 0 })
    }

    #[doc = ""]
    pub fn append_record(
        &mut self,
        timestamp: u64,
        subject: u64,
        predicate: u64,
        object: u64,
        graph: u64,
    ) -> std::io::Result<()> {
        let mut buffer = [0u8; RECORD_SIZE];
        encode_record(&mut buffer, timestamp, subject, predicate, object, graph);
        self.log_file.write_all(&buffer)?;
        self.record_count += 1;
        Ok(())
    }

    #[doc = ""]
    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    #[doc = ""]
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.log_file.flush()
    }
}

#[derive(Clone, Debug)]
#[doc = ""]
pub struct Event {
    #[doc = ""]
    pub timestamp: u64,
    #[doc = ""]
    pub subject: u64,
    #[doc = ""]
    pub predicate: u64,
    #[doc = ""]
    pub object: u64,
    #[doc = ""]
    pub graph: u64,
}

#[derive(Debug, Clone)]
pub struct ResolvedEvent{
    pub timestamp: u64,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub graph: String,
}

impl Event {
    pub fn resolve(&self, dict: &Dictionary) -> ResolvedEvent {
        ResolvedEvent {
            timestamp: self.timestamp,
            subject: dict.fetch_uri(self.subject).unwrap_or("UNKNOWN").to_string(),
            predicate: dict.fetch_uri(self.predicate).unwrap_or("UNKNOWN").to_string(),
            object: dict.fetch_uri(self.object).unwrap_or("UNKNOWN").to_string(),
            graph: dict.fetch_uri(self.graph).unwrap_or("UNKNOWN").to_string()
         }
    }
}
