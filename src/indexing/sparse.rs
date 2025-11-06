use crate::indexing::dictionary::Dictionary;
use crate::indexing::shared::{decode_record, Event, ResolvedEvent, RECORD_SIZE};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Builder for creating sparse indexes that store only periodic entries.
///
/// A sparse index reduces storage space by indexing only every Nth record,
/// trading some query precision for significant space savings.
#[doc = ""]
pub struct SparseIndexBuilder {
    index_file: File,
    interval: usize,
}
#[doc = ""]
impl SparseIndexBuilder {
    /// Creates a new sparse index builder that writes to the specified file.
    ///
    /// # Arguments
    /// * `index_path` - Path where the index file will be created
    /// * `interval` - Number of records between index entries (e.g., 1000 means index every 1000th record)
    ///
    /// # Returns
    /// A new `SparseIndexBuilder` instance or an I/O error
    #[doc = ""]
    pub fn create(index_path: &str, interval: usize) -> std::io::Result<Self> {
        let index_file = File::create(index_path)?;
        Ok(Self { index_file, interval })
    }

    /// Adds an entry to the sparse index if the record count matches the interval.
    ///
    /// Only records where `record_count % interval == 0` are indexed to save space.
    ///
    /// # Arguments
    /// * `record_count` - The current record number in the log
    /// * `timestamp` - Timestamp of the record
    /// * `offset` - Byte offset of the record in the log file
    ///
    /// # Returns
    /// `true` if the entry was added to the index, `false` if skipped
    #[doc = ""]
    pub fn add_entry(
        &mut self,
        record_count: u64,
        timestamp: u64,
        offset: u64,
    ) -> std::io::Result<bool> {
        if record_count % self.interval as u64 == 0 {
            self.index_file.write_all(&timestamp.to_be_bytes())?;
            self.index_file.write_all(&offset.to_be_bytes())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Finalizes the index by flushing any buffered writes to disk.
    ///
    /// This should be called after all entries have been added.
    #[doc = ""]
    pub fn finalize(&mut self) -> std::io::Result<()> {
        self.index_file.flush()
    }
}

/// Builds a sparse index for an existing log file.
///
/// This function reads through the entire log file and creates an index
/// with entries only for records at the specified interval.
///
/// # Arguments
/// * `log_path` - Path to the log file to index
/// * `index_path` - Path where the index file will be created
/// * `interval` - Number of records between index entries
///
/// # Returns
/// Ok(()) on success, or an I/O error
pub fn build_sparse_index(
    log_path: &str,
    index_path: &str,
    interval: &usize,
) -> std::io::Result<()> {
    let mut log = File::open(log_path)?;
    let mut builder = SparseIndexBuilder::create(index_path, *interval)?;

    let mut offset = 0u64;
    let mut record_count = 0u64;
    let mut record = [0u8; RECORD_SIZE];

    while log.read_exact(&mut record).is_ok() {
        let (timestamp, _, _, _, _) = decode_record(&record);
        builder.add_entry(record_count, timestamp, offset)?;
        offset += RECORD_SIZE as u64;
        record_count += 1;
    }

    builder.finalize()?;
    Ok(())
}

/// Builds a sparse index and initializes an empty dictionary.
///
/// This is a convenience function that creates both the index and
/// an empty dictionary file. The dictionary can be populated separately
/// when processing RDF data.
///
/// # Arguments
/// * `log_path` - Path to the log file to index
/// * `index_path` - Path where the index file will be created
/// * `dictionary_path` - Path where the dictionary file will be created
/// * `interval` - Number of records between index entries
///
/// # Returns
/// Ok(()) on success, or an I/O error
pub fn build_sparse_index_with_dictionary(
    log_path: &str,
    index_path: &str,
    dictionary_path: &str,
    interval: &usize,
) -> std::io::Result<()> {
    let mut log = File::open(log_path)?;
    let mut builder = SparseIndexBuilder::create(index_path, *interval)?;
    let dictionary = Dictionary::new();

    let mut offset = 0u64;
    let mut record_count = 0u64;
    let mut record = [0u8; RECORD_SIZE];

    while log.read_exact(&mut record).is_ok() {
        let (timestamp, _subject, _predicate, _object, _graph) = decode_record(&record);

        builder.add_entry(record_count, timestamp, offset)?;

        offset += RECORD_SIZE as u64;
        record_count += 1;
    }

    builder.finalize()?;
    dictionary.save_to_file(Path::new(dictionary_path))?;

    Ok(())
}

/// Reader for sparse indexes that enables efficient timestamp-based queries.
///
/// The sparse reader loads the entire index into memory for fast binary search,
/// then performs sequential scans of the log file starting from the appropriate position.
pub struct SparseReader {
    index: Vec<(u64, u64)>,
    #[allow(dead_code)]
    interval: usize,
}

impl SparseReader {
    /// Opens a sparse index and its associated dictionary.
    ///
    /// # Arguments
    /// * `index_path` - Path to the sparse index file
    /// * `dictionary_path` - Path to the dictionary file
    /// * `interval` - The interval used when building the index
    ///
    /// # Returns
    /// A tuple of (SparseReader, Dictionary) or an I/O error
    pub fn open_with_dictionary(
        index_path: &str,
        dictionary_path: &str,
        interval: usize,
    ) -> std::io::Result<(Self, Dictionary)> {
        let reader = Self::open(index_path, interval)?;
        let dictionary = Dictionary::load_from_file(Path::new(dictionary_path))?;
        Ok((reader, dictionary))
    }
    /// Queries the log and returns results with URIs resolved from the dictionary.
    ///
    /// This method performs the same query as `query()` but resolves all numeric IDs
    /// back to their original URI strings using the provided dictionary.
    ///
    /// # Arguments
    /// * `log_path` - Path to the log file
    /// * `dict` - Dictionary for resolving IDs to URIs
    /// * `timestamp_start_bound` - Minimum timestamp (inclusive)
    /// * `timestamp_end_bound` - Maximum timestamp (inclusive)
    ///
    /// # Returns
    /// Vector of resolved events or an I/O error
    pub fn query_resolved(
        &self,
        log_path: &str,
        dict: &Dictionary,
        timestamp_start_bound: u64,
        timestamp_end_bound: u64,
    ) -> std::io::Result<Vec<ResolvedEvent>> {
        let events = self.query(log_path, timestamp_start_bound, timestamp_end_bound)?;
        Ok(events.into_iter().map(|e| e.resolve(dict)).collect())
    }

    /// Opens a sparse index file and loads it into memory.
    ///
    /// # Arguments
    /// * `index_path` - Path to the sparse index file
    /// * `interval` - The interval used when building the index
    ///
    /// # Returns
    /// A new SparseReader instance or an I/O error
    pub fn open(index_path: &str, interval: usize) -> std::io::Result<Self> {
        let mut index_file = File::open(index_path)?;
        let mut index = Vec::new();
        let mut entry = [0u8; 16];

        while index_file.read_exact(&mut entry).is_ok() {
            let timestamp = u64::from_be_bytes(entry[0..8].try_into().unwrap());
            let offset = u64::from_be_bytes(entry[8..16].try_into().unwrap());

            index.push((timestamp, offset));
        }
        Ok(Self { index, interval })
    }

    /// Queries the log file for events within the specified timestamp range.
    ///
    /// Uses binary search on the index to find the starting position, then
    /// performs a sequential scan of the log file to collect matching events.
    ///
    /// # Arguments
    /// * `log_path` - Path to the log file
    /// * `timestamp_start_bound` - Minimum timestamp (inclusive)
    /// * `timestamp_end_bound` - Maximum timestamp (inclusive)
    ///
    /// # Returns
    /// Vector of events with numeric IDs or an I/O error
    pub fn query(
        &self,
        log_path: &str,
        timestamp_start_bound: u64,
        timestamp_end_bound: u64,
    ) -> std::io::Result<Vec<Event>> {
        if timestamp_start_bound > timestamp_end_bound {
            return Ok(Vec::new());
        }

        if self.index.is_empty() {
            return Ok(Vec::new());
        }

        let position = self
            .index
            .binary_search_by_key(&timestamp_start_bound, |x| x.0)
            .unwrap_or_else(|i| i.saturating_sub(1));

        let mut log = File::open(log_path)?;
        log.seek(SeekFrom::Start(self.index[position].1))?;

        let mut results = Vec::new();
        let mut record = [0u8; RECORD_SIZE];

        while log.read_exact(&mut record).is_ok() {
            let (timestamp, subject, predicate, object, graph) = decode_record(&record);

            if timestamp > timestamp_end_bound {
                break;
            }

            if timestamp >= timestamp_start_bound {
                results.push(Event { timestamp, subject, predicate, object, graph });
            }
        }

        Ok(results)
    }

    /// Returns the size of the index in bytes.
    ///
    /// Each index entry is 16 bytes (8 bytes timestamp + 8 bytes offset),
    /// so this returns `index.len() * 16`.
    pub fn index_size_bytes(&self) -> usize {
        self.index.len() * 16
    }
}
