use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::core::Event;

#[derive(Debug)]
/// Storage component memory usage breakdown
pub struct StorageComponentSizes {
    pub batch_buffer_bytes: usize,
    pub segments_count: usize,
    pub dictionary_bytes: usize,
    pub estimated_total_bytes: usize,
}

#[derive(Debug)]
/// In-memory buffer that batches events before persisting them to disk
pub struct BatchBuffer {
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
    /// Maximum number of events to buffer before flushing to disk
    pub max_batch_events: u64,
    /// Maximum age in seconds before flushing buffered events to disk
    pub max_batch_age_seconds: u64,
    /// Maximum bytes to buffer before flushing to disk
    pub max_batch_bytes: usize,
    pub sparse_interval: usize,
    pub entries_per_index_block: usize,
    pub segment_base_path: String,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            max_batch_bytes: 10 * 1024 * 1024,
            max_batch_age_seconds: 60,
            max_batch_events: 100_000,
            sparse_interval: 1000,
            entries_per_index_block: 1024,
            segment_base_path: "./data".to_string(),
        }
    }
}
