//! Legacy storage utilities - to be moved to storage module

use std::fs::File;
use std::io::Write;
use crate::core::encoding::{encode_record, RECORD_SIZE};

/// Log writer for appending encoded records to a file
pub struct LogWriter {
    log_file: File,
    record_count: u64,
}

impl LogWriter {
    /// Create a new log writer for the given file path
    pub fn create(path: &str) -> std::io::Result<Self> {
        let log_file = File::create(path)?;
        Ok(Self {
            log_file,
            record_count: 0
        })
    }

    /// Append an encoded record to the log file
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

    /// Get the current record count
    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Flush the log file
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.log_file.flush()
    }
}









