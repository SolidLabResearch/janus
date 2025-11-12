use std::{
    collections::VecDeque,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    rc::Rc,
    sync::{Arc, Mutex, RwLock},
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    core::{
        encoding::{decode_record, encode_record, RECORD_SIZE},
        Event, RDFEvent,
    },
    storage::{
        indexing::dictionary::Dictionary,
        util::{BatchBuffer, EnhancedSegmentMetadata, IndexBlock, StreamingConfig},
    },
};

#[doc = "Struct for the Implementation of the Segmented Storage of RDF Streams."]
pub struct StreamingSegmentedStorage {
    batch_buffer: Arc<RwLock<BatchBuffer>>,
    segments: Arc<RwLock<Vec<EnhancedSegmentMetadata>>>,
    dictionary: Rc<RwLock<Dictionary>>,
    flush_handle: Option<JoinHandle<()>>,
    shutdown_signal: Arc<Mutex<bool>>,
    config: StreamingConfig,
}

impl StreamingSegmentedStorage {
    #[doc = ""]
    pub fn new(config: StreamingConfig) -> std::io::Result<Self> {
        std::fs::create_dir_all(&config.segment_base_path)?;

        let storage = Self {
            batch_buffer: Arc::new(RwLock::new(BatchBuffer {
                events: VecDeque::new(),
                total_bytes: 0,
                oldest_timestamp_bound: None,
                newest_timestamp_bound: None,
            })),

            segments: Arc::new(RwLock::new(Vec::new())),
            dictionary: Rc::new(RwLock::new(Dictionary::new())),
            flush_handle: None,
            shutdown_signal: Arc::new(Mutex::new(false)),
            config,
        };
        storage.load_existing_segments()?;
        Ok(storage)
    }

    #[doc = ""]
    pub fn start_background_flushing(&mut self) {
        let batch_buffer_clone = Arc::clone(&self.batch_buffer);
        let segments_clone = Arc::clone(&self.segments);
        let shutdown_clone = Arc::clone(&self.shutdown_signal);
        let config_clone = self.config.clone();

        let handle = std::thread::spawn(move || {
            Self::background_flush_loop(
                batch_buffer_clone,
                segments_clone,
                shutdown_clone,
                config_clone,
            );
        });

        self.flush_handle = Some(handle);
    }

    pub fn write(&self, event: Event) -> std::io::Result<()> {
        let event_size = std::mem::size_of::<Event>();

        {
            let mut batch_buffer = self.batch_buffer.write().unwrap();

            if batch_buffer.oldest_timestamp_bound.is_none() {
                batch_buffer.oldest_timestamp_bound = Some(event.timestamp);
            }

            batch_buffer.newest_timestamp_bound = Some(event.timestamp);

            batch_buffer.total_bytes += event_size;

            batch_buffer.events.push_back(event);
        }
        // Note: Synchronous flushing removed for high throughput.
        // Background thread handles all flushing based on time limits.
        Ok(())
    }

    /// User-friendly API: Write RDF data directly with URI strings
    pub fn write_rdf(
        &self,
        timestamp: u64,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: &str,
    ) -> std::io::Result<()> {
        let rdf_event = RDFEvent::new(timestamp, subject, predicate, object, graph);
        let encoded_event = {
            let mut dict = self.dictionary.write().unwrap();
            rdf_event.encode(&mut dict)
        };
        self.write(encoded_event)
    }

    fn should_flush(&self) -> bool {
        let batch_buffer = self.batch_buffer.read().unwrap();

        batch_buffer.events.len() >= self.config.max_batch_events.try_into().unwrap()
            || batch_buffer.total_bytes > self.config.max_batch_bytes
            || batch_buffer.oldest_timestamp_bound.map_or(false, |oldest| {
                let current_timestamp = Self::current_timestamp();

                // Use saturating subtraction to avoid underflow if oldest > current_timestamp
                current_timestamp.saturating_sub(oldest)
                    >= self.config.max_batch_age_seconds * 1_000
            })
    }

    fn current_timestamp() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    }

    fn flush_batch_buffer_to_segment(&self) -> std::io::Result<()> {
        // Automatically extract events from the batch buffer.

        let events_to_flush = {
            let mut batch_buffer = self.batch_buffer.write().unwrap();
            if batch_buffer.events.is_empty() {
                return Ok(());
            }

            let events: Vec<Event> = batch_buffer.events.drain(..).collect();

            batch_buffer.total_bytes = 0;
            batch_buffer.oldest_timestamp_bound = None;
            batch_buffer.newest_timestamp_bound = None;
            events
        };

        let segment = self.create_segment_with_two_level_index(events_to_flush)?;

        {
            let mut segments = self.segments.write().unwrap();
            segments.push(segment);
        }
        Ok(())
    }

    fn create_segment_with_two_level_index(
        &self,
        mut events: Vec<Event>,
    ) -> std::io::Result<EnhancedSegmentMetadata> {
        events.sort_by_key(|e| e.timestamp);

        let segment_id = Self::generate_segment_id();

        let data_path = format!("{}/segment-{}.log", self.config.segment_base_path, segment_id);
        let index_path = format!("{}/segment-{}.idx", self.config.segment_base_path, segment_id);

        let mut data_file = BufWriter::new(std::fs::File::create(&data_path)?);
        let mut index_file = BufWriter::new(std::fs::File::create(&index_path)?);

        let mut index_directory = Vec::new();
        let mut current_block_entries = Vec::new();

        let mut current_block_min_ts = None;
        let mut current_block_max_ts = 0u64;

        let mut data_offset = 0u64;

        for (record_count, event) in events.iter().enumerate() {
            let record_bytes = self.serialize_event_to_fixed_size(event);
            data_file.write_all(&record_bytes)?;

            if record_count % self.config.sparse_interval == 0 {
                let sparse_entry = (event.timestamp, data_offset);

                if current_block_min_ts.is_none() {
                    current_block_min_ts = Some(event.timestamp);
                }

                current_block_max_ts = event.timestamp;
                current_block_entries.push(sparse_entry);

                if current_block_entries.len() >= self.config.entries_per_index_block {
                    let block_metadata = self.flush_index_block(
                        &mut index_file,
                        &current_block_entries,
                        current_block_min_ts.unwrap(),
                        current_block_max_ts,
                    )?;

                    index_directory.push(block_metadata);

                    current_block_entries.clear();
                    current_block_min_ts = None;
                }
            }
            data_offset += record_bytes.len() as u64;
        }

        if !current_block_entries.is_empty() {
            let block_metadata = self.flush_index_block(
                &mut index_file,
                &current_block_entries,
                current_block_min_ts.unwrap(),
                current_block_max_ts,
            )?;

            index_directory.push(block_metadata);
        }

        data_file.flush()?;
        index_file.flush()?;

        Ok(EnhancedSegmentMetadata {
            start_timstamp: events.first().unwrap().timestamp,
            end_timestamp: events.last().unwrap().timestamp,
            data_path,
            index_path,
            record_count: events.len() as u64,
            index_directory,
        })
    }

    fn flush_index_block(
        &self,
        index_file: &mut BufWriter<std::fs::File>,
        entries: &[(u64, u64)],
        min_ts: u64,
        max_ts: u64,
    ) -> std::io::Result<IndexBlock> {
        Self::flush_index_block_static(index_file, entries, min_ts, max_ts)
    }

    pub fn query(&self, start_timestamp: u64, end_timestamp: u64) -> std::io::Result<Vec<Event>> {
        let mut results = Vec::new();

        // First try to query the immediate batch buffer which has the fastest visibility.

        {
            let batch_buffer = self.batch_buffer.read().unwrap();

            for event in &batch_buffer.events {
                if event.timestamp >= start_timestamp && event.timestamp <= end_timestamp {
                    results.push(event.clone());
                }
            }
        }

        // Then querying the relevant segment with a two level indexing

        {
            let segments = self.segments.read().unwrap();
            
            for segment in segments.iter() {
                if self.segment_overlaps(segment, start_timestamp, end_timestamp) {
                    let segment_results =
                        self.query_segment_two_level(segment, start_timestamp, end_timestamp)?;
                    results.extend(segment_results);
                }
            }
        }

        results.sort_by_key(|e| e.timestamp);

        Ok(results)
    }

    /// User-friendly API: Query and return RDF events with URI strings
    pub fn query_rdf(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<RDFEvent>> {
        let encoded_events = self.query(start_timestamp, end_timestamp)?;
        let dict = self.dictionary.read().unwrap();
        Ok(encoded_events.into_iter().map(|event| event.decode(&dict)).collect())
    }

    fn query_segment_two_level(
        &self,
        segment: &EnhancedSegmentMetadata,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<Event>> {
        // If we have index directory, use two-level indexing
        if !segment.index_directory.is_empty() {
            // Step 1 : Find relevant index blocks using in-memory directory
            let relevant_blocks: Vec<&IndexBlock> = segment
                .index_directory
                .iter()
                .filter(|block| {
                    block.min_timestamp <= end_timestamp && block.max_timestamp >= start_timestamp
                })
                .collect();

            if relevant_blocks.is_empty() {
                return Ok(Vec::new());
            }

            // Step 2 : Load only the relevant blocks from the disk
            let sparse_entries =
                self.load_relevant_index_blocks(&segment.index_path, &relevant_blocks)?;

            if sparse_entries.is_empty() {
                return Ok(Vec::new());
            }

            // Step 3 : Binary search the loaded entries
            let lb = sparse_entries.partition_point(|(ts, _)| *ts < start_timestamp);
            let start_position = lb.saturating_sub(1);
            let start_offset = sparse_entries[start_position].1;

            // Step 4 : Sequential Scan from the checkpoint
            self.scan_data_from_offset(
                &segment.data_path,
                start_offset,
                start_timestamp,
                end_timestamp,
            )
        } else {
            // Fallback: Full scan of the data file (for segments without loaded index)
            self.scan_data_from_offset(&segment.data_path, 0, start_timestamp, end_timestamp)
        }
    }

    fn load_relevant_index_blocks(
        &self,
        index_path: &str,
        blocks: &[&IndexBlock],
    ) -> std::io::Result<Vec<(u64, u64)>> {
        let mut index_file = std::fs::File::open(index_path)?;
        let mut sparse_entries = Vec::new();

        for block in blocks {
            index_file.seek(SeekFrom::Start(block.file_offset))?;

            let block_size = block.entry_count as usize * 16; // 16 bytes per entry.
            let mut buffer = vec![0u8; block_size];
            index_file.read_exact(&mut buffer)?;

            // Parse the entries.

            for chunk in buffer.chunks_exact(16) {
                let timestamp = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
                let offset = u64::from_be_bytes(chunk[8..16].try_into().unwrap());
                sparse_entries.push((timestamp, offset));
            }
        }

        sparse_entries.sort_by_key(|&(ts, _)| ts);
        Ok(sparse_entries)
    }

    fn scan_data_from_offset(
        &self,
        data_path: &str,
        start_offset: u64,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<Event>> {
        let mut file = std::fs::File::open(data_path)?;
        file.seek(SeekFrom::Start(start_offset))?;

        let mut results = Vec::new();
        let mut record = [0u8; RECORD_SIZE];

        while file.read_exact(&mut record).is_ok() {
            let (timestamp, subject, predicate, object, graph) = decode_record(&record);

            if timestamp > end_timestamp {
                break;
            }

            if timestamp >= start_timestamp {
                results.push(Event { timestamp, subject, predicate, object, graph });
            }
        }
        Ok(results)
    }

    fn segment_overlaps(
        &self,
        segment: &EnhancedSegmentMetadata,
        start_ts: u64,
        end_ts: u64,
    ) -> bool {
        segment.start_timstamp <= end_ts && segment.end_timestamp >= start_ts
    }

    fn background_flush_loop(
        batch_buffer: Arc<RwLock<BatchBuffer>>,
        segments: Arc<RwLock<Vec<EnhancedSegmentMetadata>>>,
        shutdown_signal: Arc<Mutex<bool>>,
        config: StreamingConfig,
    ) {
        while !*shutdown_signal.lock().unwrap() {
            std::thread::sleep(Duration::from_millis(100));

            // Check if flush is needed or not.

            let should_flush = {
                let batch_buffer = batch_buffer.read().unwrap();

                batch_buffer.events.len() >= config.max_batch_events.try_into().unwrap()
                    || batch_buffer.total_bytes >= config.max_batch_bytes
                    || batch_buffer.oldest_timestamp_bound.map_or(false, |oldest| {
                        let current_timestamp = Self::current_timestamp();
                        current_timestamp.saturating_sub(oldest)
                            >= config.max_batch_age_seconds * 1_000
                    })
            };

            if should_flush {
                // TODO : Add better error handling here in this case
                if let Err(e) =
                    Self::flush_background(batch_buffer.clone(), segments.clone(), &config)
                {
                    eprintln!("Background flush failed: {}", e);
                }
            }
        }
    }

    fn flush_background(
        batch_buffer: Arc<RwLock<BatchBuffer>>,
        segments: Arc<RwLock<Vec<EnhancedSegmentMetadata>>>,
        config: &StreamingConfig,
    ) -> std::io::Result<()> {
        // Automatically extract events from the batch buffer.

        let events_to_flush = {
            let mut batch_buffer = batch_buffer.write().unwrap();
            if batch_buffer.events.is_empty() {
                return Ok(());
            }

            let events: Vec<Event> = batch_buffer.events.drain(..).collect();

            batch_buffer.total_bytes = 0;
            batch_buffer.oldest_timestamp_bound = None;
            batch_buffer.newest_timestamp_bound = None;
            events
        };

        // Create a new segment for these events
        let segment_id = Self::current_timestamp();
        let data_path = format!("{}/segment-{}.log", config.segment_base_path, segment_id);
        let index_path = format!("{}/segment-{}.idx", config.segment_base_path, segment_id);

        // Use buffered writers for performance (same as original implementation)
        let mut data_file = BufWriter::new(std::fs::File::create(&data_path)?);
        let mut index_file = BufWriter::new(std::fs::File::create(&index_path)?);

        let mut index_directory = Vec::new();
        let mut current_block_entries = Vec::new();
        let mut current_block_min_ts = None;
        let mut current_block_max_ts = 0u64;
        let mut data_offset = 0u64;

        for (record_count, event) in events_to_flush.iter().enumerate() {
            // Use the same serialization as the original
            let record_bytes = Self::serialize_event_to_fixed_size_static(event);
            data_file.write_all(&record_bytes)?;

            if record_count % config.sparse_interval == 0 {
                let sparse_entry = (event.timestamp, data_offset);

                if current_block_min_ts.is_none() {
                    current_block_min_ts = Some(event.timestamp);
                }

                current_block_max_ts = event.timestamp;
                current_block_entries.push(sparse_entry);

                if current_block_entries.len() >= config.entries_per_index_block {
                    let block_metadata = Self::flush_index_block_static(
                        &mut index_file,
                        &current_block_entries,
                        current_block_min_ts.unwrap(),
                        current_block_max_ts,
                    )?;

                    index_directory.push(block_metadata);

                    current_block_entries.clear();
                    current_block_min_ts = None;
                }
            }
            data_offset += record_bytes.len() as u64;
        }

        if !current_block_entries.is_empty() {
            let block_metadata = Self::flush_index_block_static(
                &mut index_file,
                &current_block_entries,
                current_block_min_ts.unwrap(),
                current_block_max_ts,
            )?;

            index_directory.push(block_metadata);
        }

        data_file.flush()?;
        index_file.flush()?;

        // Add the new segment to the segments list
        let new_segment = EnhancedSegmentMetadata {
            start_timstamp: events_to_flush.first().unwrap().timestamp,
            end_timestamp: events_to_flush.last().unwrap().timestamp,
            data_path,
            index_path,
            record_count: events_to_flush.len() as u64,
            index_directory,
        };

        {
            let mut segments = segments.write().unwrap();
            segments.push(new_segment);
            // Keep segments sorted by start timestamp
            segments.sort_by_key(|s| s.start_timstamp);
        }

        Ok(())
    }

    fn load_existing_segments(&self) -> std::io::Result<()> {
        use std::fs;

        let segment_dir = &self.config.segment_base_path;
        if !fs::metadata(segment_dir).is_ok() {
            return Ok(());
        }

        let entries = fs::read_dir(segment_dir)?;
        let mut segments = Vec::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with("segment-") && filename.ends_with(".log") {
                    // Extract segment ID from filename
                    if let Some(id_str) =
                        filename.strip_prefix("segment-").and_then(|s| s.strip_suffix(".log"))
                    {
                        if let Ok(segment_id) = id_str.parse::<u64>() {
                            // Try to load the segment metadata by reading the data file
                            let data_path = format!("{}/segment-{}.log", segment_dir, segment_id);
                            let index_path = format!("{}/segment-{}.idx", segment_dir, segment_id);

                            if let Ok(_metadata) = fs::metadata(&data_path) {
                                // Load index directory if index file exists
                                let (index_directory, start_ts, end_ts, record_count) = 
                                    if fs::metadata(&index_path).is_ok() {
                                        Self::load_index_directory_from_file(&index_path).unwrap_or_else(|_| {
                                            (Vec::new(), 0, u64::MAX, 0)
                                        })
                                    } else {
                                        (Vec::new(), 0, u64::MAX, 0)
                                    };

                                let segment = EnhancedSegmentMetadata {
                                    start_timstamp: start_ts,
                                    end_timestamp: end_ts,
                                    data_path,
                                    index_path,
                                    record_count,
                                    index_directory,
                                };
                                segments.push(segment);
                            }
                        }
                    }
                }
            }
        }

        // Sort segments by start timestamp
        segments.sort_by_key(|s| s.start_timstamp);

        {
            let mut self_segments = self.segments.write().unwrap();
            *self_segments = segments;
        }

        Ok(())
    }

    fn load_index_directory_from_file(index_path: &str) -> std::io::Result<(Vec<IndexBlock>, u64, u64, u64)> {
        use std::io::Read;
        
        let mut file = std::fs::File::open(index_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // Index file format: each block is stored as consecutive (timestamp, offset) pairs (16 bytes each)
        // We need to reconstruct the IndexBlock directory structure
        
        if buffer.is_empty() {
            return Ok((Vec::new(), 0, u64::MAX, 0));
        }

        let mut index_directory = Vec::new();
        let mut file_offset = 0u64;
        let mut global_min_ts = u64::MAX;
        let mut global_max_ts = 0u64;
        let mut total_records = 0u64;

        // Read all entries to reconstruct blocks
        // Note: This is a simplified reconstruction - in practice you'd want to store block boundaries
        let entries_per_block = 1000; // From config.entries_per_index_block
        let mut current_block_start = 0;
        
        while current_block_start < buffer.len() {
            let block_size = std::cmp::min(entries_per_block * 16, buffer.len() - current_block_start);
            let block_end = current_block_start + block_size;
            let block_entries = block_end - current_block_start;
            let entry_count = (block_entries / 16) as u32;
            
            if entry_count == 0 {
                break;
            }

            //Read first and last timestamp of this block
            let first_ts = u64::from_le_bytes(buffer[current_block_start..current_block_start+8].try_into().unwrap());
            let last_entry_start = current_block_start + ((entry_count - 1) as usize * 16);
            let last_ts = u64::from_le_bytes(buffer[last_entry_start..last_entry_start+8].try_into().unwrap());

            global_min_ts = global_min_ts.min(first_ts);
            global_max_ts = global_max_ts.max(last_ts);
            total_records += entry_count as u64;

            index_directory.push(IndexBlock {
                min_timestamp: first_ts,
                max_timestamp: last_ts,
                file_offset,
                entry_count,
            });

            file_offset += block_size as u64;
            current_block_start = block_end;
        }

        Ok((index_directory, global_min_ts, global_max_ts, total_records))
    }

    pub fn shutdown(&mut self) -> std::io::Result<()> {
        *self.shutdown_signal.lock().unwrap() = true;

        // Final Flush

        self.flush_batch_buffer_to_segment()?;

        if let Some(handle) = self.flush_handle.take() {
            handle.join().unwrap();
        }
        Ok(())
    }

    fn serialize_event_to_fixed_size(&self, event: &Event) -> Vec<u8> {
        Self::serialize_event_to_fixed_size_static(event)
    }

    fn serialize_event_to_fixed_size_static(event: &Event) -> Vec<u8> {
        let mut record = [0u8; RECORD_SIZE];
        encode_record(
            &mut record,
            event.timestamp,
            event.subject,
            event.predicate,
            event.object,
            event.graph,
        );
        record.to_vec()
    }

    fn flush_index_block_static(
        index_file: &mut BufWriter<std::fs::File>,
        entries: &[(u64, u64)],
        min_ts: u64,
        max_ts: u64,
    ) -> std::io::Result<IndexBlock> {
        let file_offset = index_file.stream_position()?;

        for (timestamp, offset) in entries {
            index_file.write_all(&timestamp.to_le_bytes())?;
            index_file.write_all(&offset.to_be_bytes())?;
        }

        Ok(IndexBlock {
            min_timestamp: min_ts,
            max_timestamp: max_ts,
            file_offset,
            entry_count: entries.len() as u32,
        })
    }

    fn generate_segment_id() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    }
}
