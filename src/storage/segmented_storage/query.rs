use std::io::{Read, Seek, SeekFrom};

use crate::{
    core::{
        encoding::{decode_record, RECORD_SIZE},
        Event, RDFEvent,
    },
    storage::util::{EnhancedSegmentMetadata, IndexBlock},
};

use super::StreamingSegmentedStorage;

impl StreamingSegmentedStorage {
    /// Query events within a timestamp range from the storage system but result in encoded Events and not RDFEvents.
    pub fn query(&self, start_timestamp: u64, end_timestamp: u64) -> std::io::Result<Vec<Event>> {
        self.ensure_background_flush_healthy()?;
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

    /// User-friendly API: Query and return RDF events with URI strings.
    pub fn query_rdf(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<RDFEvent>> {
        self.ensure_background_flush_healthy()?;
        let encoded_events = self.query(start_timestamp, end_timestamp)?;
        let dict = self.dictionary.read().unwrap();
        Ok(encoded_events.into_iter().map(|event| event.decode(&dict)).collect())
    }

    // Query a segment using two-level indexing
    fn query_segment_two_level(
        &self,
        segment: &EnhancedSegmentMetadata,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<Event>> {
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

            // If no entries loaded, fall back to full scan
            if sparse_entries.is_empty() {
                return self.scan_data_from_offset(
                    &segment.data_path,
                    0,
                    start_timestamp,
                    end_timestamp,
                );
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

    // Load only the relevant index blocks from disk
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

            for chunk in buffer.chunks_exact(16) {
                let timestamp = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
                let offset = u64::from_be_bytes(chunk[8..16].try_into().unwrap());
                sparse_entries.push((timestamp, offset));
            }
        }

        sparse_entries.sort_by_key(|&(ts, _)| ts);
        Ok(sparse_entries)
    }

    // Scan data file from a given offset to retrieve events within the timestamp range
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

    // Check if a segment overlaps with the given timestamp range
    fn segment_overlaps(
        &self,
        segment: &EnhancedSegmentMetadata,
        start_ts: u64,
        end_ts: u64,
    ) -> bool {
        segment.start_timstamp <= end_ts && segment.end_timestamp >= start_ts
    }
}
