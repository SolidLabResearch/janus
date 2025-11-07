use std::collections::{VecDeque, HashMap};
use std::sync::{Arc, RwLock, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::indexing::shared::Event;

#[derive(Debug)]
pub struct WAL {
    pub events: VecDeque<Event>,
    pub total_bytes: usize,
    pub oldest_timestamp_bound: Option<u64>,
    pub newest_timestamp_bound: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct IndexBlock {
    pub min_timestamp: u64,
    pub max_timestamp: u64,
    pub file_offset: u64,
    pub entry_count: u32,
}

#[derive(Debug, Clone)]
pub struct EnhancedSegmentMetadata {
    pub start_timstamp: u64,
    pub end_timestamp: u64,
    pub data_path: String,
    pub index_path: String,
    pub record_count: u64,
    pub index_directory: Vec<IndexBlock>,
}

#[derive(Clone)]
pub struct StreamingConfig {
    pub max_wal_events: u64,
    pub max_wal_age_seconds: u64,
    pub max_wal_bytes: usize,
    pub sparse_interval: usize,
    pub entries_per_index_block: usize,
    pub segment_base_path: String,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            max_wal_bytes: 10 * 1024 * 1024,
            max_wal_age_seconds: 60,
            max_wal_events: 100_000,
            sparse_interval: 1000,
            entries_per_index_block: 1024,
            segment_base_path: "./data".to_string(),
        }
    }
}


