use std::{
    collections::HashSet,
    io::{Read, Seek, SeekFrom}, time::Instant,
};

use crate::{
    core::{
        encoding::{decode_record, RECORD_SIZE},
        Event, RDFEvent,
    },
    storage::util::{EnhancedSegmentMetadata, IndexBlock},
};

use super::{AccessMetrics, StreamingSegmentedStorage};

impl StreamingSegmentedStorage {
    /// Query events in the half-open timestamp interval `[start, end)`.
    ///
    /// The underlying storage query API is inclusive at both ends for
    /// compatibility with fixed historical ranges and point lookups. Historical
    /// sliding windows use this adapter so their resolved end remains exclusive.
    pub fn query_half_open(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<Event>> {
        self.ensure_background_flush_healthy()?;
        if start_timestamp >= end_timestamp {
            return Ok(Vec::new());
        }
        self.query_with_metrics(start_timestamp, end_timestamp - 1)
            .map(|(rows, _)| rows)
    }

    /// Measured timestamp-only query for the inclusive interval `[start, end]`.
    /// Its counters have the same definitions as subject-aware access.
    pub fn query_with_metrics(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<(Vec<Event>, AccessMetrics)> {
        self.ensure_background_flush_healthy()?;
        let mut metrics = AccessMetrics::default();
        let mut results = Vec::new();
        {
            let batch_buffer = self.batch_buffer.read().unwrap();
            for event in &batch_buffer.events {
                metrics.records_examined += 1;
                if event.timestamp >= start_timestamp && event.timestamp <= end_timestamp {
                    metrics.records_matched += 1;
                    results.push(event.clone());
                }
            }
        }
        let segments = self.segments.read().unwrap();
        for segment in segments
            .iter()
            .filter(|segment| self.segment_overlaps(segment, start_timestamp, end_timestamp))
        {
            let before = metrics.records_examined;
            results.extend(self.query_segment_two_level_with_metrics(
                segment,
                start_timestamp,
                end_timestamp,
                &mut metrics,
            )?);
            if metrics.records_examined != before {
                metrics.segments_touched += 1;
            }
        }
        results.sort_by_key(|e| e.timestamp);
        metrics.records_returned = results.len() as u64;
        Ok((results, metrics))
    }

    /// Measured timestamp-only query for the half-open interval `[start, end)`.
    pub fn query_half_open_with_metrics(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<(Vec<Event>, AccessMetrics)> {
        if start_timestamp >= end_timestamp {
            return Ok((Vec::new(), AccessMetrics::default()));
        }
        self.query_with_metrics(start_timestamp, end_timestamp - 1)
    }

    /// Query events within a timestamp range from the storage system but result in encoded Events and not RDFEvents.
    pub fn query(&self, start_timestamp: u64, end_timestamp: u64) -> std::io::Result<Vec<Event>> {
        self.query_with_metrics(start_timestamp, end_timestamp).map(|(rows, _)| rows)
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

    /// Query RDF events in the half-open timestamp interval `[start, end)`.
    pub fn query_rdf_half_open(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
    ) -> std::io::Result<Vec<RDFEvent>> {
        let encoded_events = self.query_half_open(start_timestamp, end_timestamp)?;
        let dict = self.dictionary.read().unwrap();
        Ok(encoded_events.into_iter().map(|event| event.decode(&dict)).collect())
    }

    /// Query a half-open interval restricted by RDF subject, using the
    /// persistent per-segment subject-offset sidecar where available.
    pub fn query_rdf_half_open_for_subjects(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
        subjects: &HashSet<String>,
    ) -> std::io::Result<Vec<RDFEvent>> {
        self.query_rdf_half_open_for_subjects_with_metrics(start_timestamp, end_timestamp, subjects)
            .map(|(rows, _)| rows)
    }

    pub fn query_rdf_half_open_for_subjects_with_metrics(
        &self,
        start_timestamp: u64,
        end_timestamp: u64,
        subjects: &HashSet<String>,
    ) -> std::io::Result<(Vec<RDFEvent>, AccessMetrics)> {
        self.query_rdf_half_open_for_subjects_with_metrics_mode(start_timestamp, end_timestamp, subjects, super::SubjectAccessMode::Linear)
    }

    /// Measured subject-aware access using an explicit lookup algorithm over
    /// the same immutable `.sidx` sidecar.
    pub fn query_rdf_half_open_for_subjects_with_metrics_mode(
        &self, start_timestamp: u64, end_timestamp: u64, subjects: &HashSet<String>, mode: super::SubjectAccessMode,
    ) -> std::io::Result<(Vec<RDFEvent>, AccessMetrics)> {
        self.ensure_background_flush_healthy()?;
        let mut metrics = AccessMetrics::default();
        if start_timestamp >= end_timestamp || subjects.is_empty() {
            return Ok((Vec::new(), metrics));
        }
        let ids = {
            let dictionary = self.dictionary.read().unwrap();
            subjects
                .iter()
                .filter_map(|subject| dictionary.string_to_id.get(subject).copied())
                .collect::<HashSet<_>>()
        };
        if ids.is_empty() {
            return Ok((Vec::new(), metrics));
        }
        let mut events = Vec::new();
        let mut used_index = false;
        let mut fallback_used = false;
        {
            let batch = self.batch_buffer.read().unwrap();
            for event in &batch.events {
                metrics.records_examined += 1;
                if event.timestamp >= start_timestamp
                    && event.timestamp < end_timestamp
                    && ids.contains(&event.subject)
                {
                    metrics.records_matched += 1;
                    events.push(event.clone());
                }
            }
        }
        let segments = self.segments.read().unwrap();
        for segment in segments
            .iter()
            .filter(|segment| self.segment_overlaps(segment, start_timestamp, end_timestamp - 1))
        {
            let subject_index =
                segment.data_path.strip_suffix(".log").unwrap_or(&segment.data_path).to_string()
                    + ".sidx";
            if !std::path::Path::new(&subject_index).exists() {
                fallback_used = true;
                // Compatibility fallback for pre-index archives; it preserves semantics.
                let mut timestamp_metrics = AccessMetrics::default();
                let rows = self.query_segment_two_level_with_metrics(
                    segment,
                    start_timestamp,
                    end_timestamp - 1,
                    &mut timestamp_metrics,
                )?;
                metrics.records_examined += timestamp_metrics.records_examined;
                if timestamp_metrics.records_examined != 0 {
                    metrics.segments_touched += 1;
                }
                for event in rows {
                    if ids.contains(&event.subject) {
                        metrics.records_matched += 1;
                        events.push(event);
                    }
                }
                continue;
            }
            let log_len = std::fs::metadata(&segment.data_path)?.len();
            used_index = true;
            if mode == super::SubjectAccessMode::Binary {
                let index_started=Instant::now();
                let log_before=metrics.log_read_decode;
                let mut index_file = std::fs::File::open(&subject_index)?;
                let entries = std::fs::metadata(&subject_index)?.len() / 12;
                let mut log_file: Option<std::fs::File> = None;
                let mut touched = false;
                for wanted in &ids {
                    let lower = self.subject_bound(&mut index_file, entries, *wanted, false, &mut metrics)?;
                    let upper = self.subject_bound(&mut index_file, entries, *wanted, true, &mut metrics)?;
                    for position in lower..upper {
                        index_file.seek(SeekFrom::Start(position * 12))?; metrics.index_seek_count += 1;
                        let mut entry=[0u8;12]; index_file.read_exact(&mut entry)?; metrics.index_entries_examined += 1;
                        let offset=u64::from_le_bytes(entry[4..12].try_into().unwrap());
                        if offset % RECORD_SIZE as u64 != 0 || offset > log_len || log_len-offset < RECORD_SIZE as u64 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"invalid selected subject-index offset")); }
                        let log_started=Instant::now(); let log=log_file.get_or_insert(std::fs::File::open(&segment.data_path)?); log.seek(SeekFrom::Start(offset))?; metrics.log_seek_count += 1;
                        let mut record=[0u8;RECORD_SIZE]; log.read_exact(&mut record)?; metrics.records_examined += 1; touched=true;
                        let (timestamp,subject,predicate,object,graph)=decode_record(&record);
                        if timestamp>=start_timestamp && timestamp<end_timestamp { metrics.records_matched += 1; events.push(Event{timestamp,subject,predicate,object,graph}); }
                        metrics.log_read_decode += log_started.elapsed();
                    }
                }
                if touched { metrics.segments_touched += 1; }
                metrics.index_lookup += index_started.elapsed().saturating_sub(metrics.log_read_decode-log_before);
                continue;
            }
            let mut index_file = std::fs::File::open(&subject_index)?;
            let mut log_file: Option<std::fs::File> = None;
            let mut entry = [0u8; 12];
            let mut touched = false;
            let mut previous: Option<(u32, u64)> = None;
            loop {
                match index_file.read_exact(&mut entry) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                        let position = index_file.stream_position()?;
                        if position % 12 != 0 {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("truncated subject index {subject_index}"),
                            ));
                        }
                        break;
                    }
                    Err(error) => return Err(error),
                }
                metrics.index_entries_examined += 1;
                let subject = u32::from_le_bytes(entry[0..4].try_into().unwrap());
                let offset = u64::from_le_bytes(entry[4..12].try_into().unwrap());
                if previous.is_some_and(|last| last > (subject, offset)) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "subject index {subject_index} is not sorted by subject and offset"
                        ),
                    ));
                }
                previous = Some((subject, offset));
                if offset % RECORD_SIZE as u64 != 0
                    || offset > log_len
                    || log_len - offset < RECORD_SIZE as u64
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid subject-index offset {offset} in {subject_index}"),
                    ));
                }
                if !ids.contains(&subject) {
                    continue;
                }
                let log_file = log_file.get_or_insert(std::fs::File::open(&segment.data_path)?);
                log_file.seek(SeekFrom::Start(offset))?;
                metrics.log_seek_count += 1;
                let mut record = [0u8; RECORD_SIZE];
                log_file.read_exact(&mut record)?;
                metrics.records_examined += 1;
                touched = true;
                let (timestamp, subject, predicate, object, graph) = decode_record(&record);
                if timestamp >= start_timestamp && timestamp < end_timestamp {
                    metrics.records_matched += 1;
                    events.push(Event { timestamp, subject, predicate, object, graph });
                }
            }
            if touched {
                metrics.segments_touched += 1;
            }
        }
        events.sort_by_key(|event| event.timestamp);
        metrics.subject_index_used = used_index && !fallback_used;
        metrics.records_returned = events.len() as u64;
        let dictionary = self.dictionary.read().unwrap();
        Ok((events.into_iter().map(|event| event.decode(&dictionary)).collect(), metrics))
    }

    fn subject_bound(&self, file: &mut std::fs::File, entries: u64, subject: u32, upper: bool, metrics: &mut AccessMetrics) -> std::io::Result<u64> {
        let (mut low, mut high) = (0, entries);
        while low < high { let mid=(low+high)/2; file.seek(SeekFrom::Start(mid*12))?; metrics.index_seek_count += 1; let mut entry=[0u8;12]; file.read_exact(&mut entry)?; metrics.index_entries_examined += 1; let found=u32::from_le_bytes(entry[0..4].try_into().unwrap()); if found < subject || (upper && found == subject) {low=mid+1;} else {high=mid;} }
        Ok(low)
    }

    // Query a segment using two-level indexing
    fn query_segment_two_level_with_metrics(
        &self,
        segment: &EnhancedSegmentMetadata,
        start_timestamp: u64,
        end_timestamp: u64,
        metrics: &mut AccessMetrics,
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
                return self.scan_data_from_offset_with_metrics(
                    &segment.data_path,
                    0,
                    start_timestamp,
                    end_timestamp,
                    metrics,
                );
            }

            // Step 3 : Binary search the loaded entries
            let lb = sparse_entries.partition_point(|(ts, _)| *ts < start_timestamp);
            let start_position = lb.saturating_sub(1);
            let start_offset = sparse_entries[start_position].1;

            // Step 4 : Sequential Scan from the checkpoint
            self.scan_data_from_offset_with_metrics(
                &segment.data_path,
                start_offset,
                start_timestamp,
                end_timestamp,
                metrics,
            )
        } else {
            // Fallback: Full scan of the data file (for segments without loaded index)
            self.scan_data_from_offset_with_metrics(
                &segment.data_path,
                0,
                start_timestamp,
                end_timestamp,
                metrics,
            )
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

            for chunk in buffer.chunks(16) {
                let timestamp = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
                let offset = u64::from_be_bytes(chunk[8..16].try_into().unwrap());
                sparse_entries.push((timestamp, offset));
            }
        }

        sparse_entries.sort_by_key(|&(ts, _)| ts);
        Ok(sparse_entries)
    }

    // Scan data file from a given offset to retrieve events within the timestamp range
    fn scan_data_from_offset_with_metrics(
        &self,
        data_path: &str,
        start_offset: u64,
        start_timestamp: u64,
        end_timestamp: u64,
        metrics: &mut AccessMetrics,
    ) -> std::io::Result<Vec<Event>> {
        let mut file = std::fs::File::open(data_path)?;
        file.seek(SeekFrom::Start(start_offset))?;

        let mut results = Vec::new();
        let mut record = [0u8; RECORD_SIZE];

        while file.read_exact(&mut record).is_ok() {
            metrics.records_examined += 1;
            let (timestamp, subject, predicate, object, graph) = decode_record(&record);

            if timestamp > end_timestamp {
                break;
            }

            if timestamp >= start_timestamp {
                metrics.records_matched += 1;
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
