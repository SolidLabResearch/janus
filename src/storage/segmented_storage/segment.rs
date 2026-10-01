use std::{
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    core::{
        encoding::{encode_record, RECORD_SIZE},
        Event,
    },
    storage::util::{BatchBuffer, EnhancedSegmentMetadata, IndexBlock, StreamingConfig},
};

use super::StreamingSegmentedStorage;

static NEXT_SEGMENT_ID: AtomicU64 = AtomicU64::new(0);

impl StreamingSegmentedStorage {
    pub(super) fn save_dictionary(&self) -> std::io::Result<()> {
        let dict_path = std::path::Path::new(&self.config.segment_base_path).join("dictionary.bin");
        let dict = self.dictionary.read().unwrap();
        dict.save_to_file(&dict_path)?;
        Ok(())
    }

    pub(super) fn current_timestamp() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    }

    pub(super) fn flush_batch_buffer_to_segment(&self) -> std::io::Result<()> {
        let _guard = self.flush_lock.lock().unwrap();
        let mut events_to_flush = {
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

        // The dictionary must be durable before a segment referencing its IDs is committed.
        let flush_result = (|| -> std::io::Result<()> {
            self.save_dictionary()?;
            let segment = Self::write_segment_files(&self.config, &mut events_to_flush)?;

            let mut segments = self.segments.write().unwrap();
            segments.push(segment);
            segments.sort_by_key(|s| s.start_timstamp);
            Ok(())
        })();

        if let Err(err) = flush_result {
            Self::restore_failed_flush(&self.batch_buffer, &events_to_flush);
            return Err(err);
        }

        Ok(())
    }

    pub(super) fn restore_failed_flush(batch_buffer: &Arc<RwLock<BatchBuffer>>, events: &[Event]) {
        if events.is_empty() {
            return;
        }

        let mut buffer = batch_buffer.write().unwrap();
        for event in events.iter().rev().cloned() {
            buffer.events.push_front(event);
            buffer.total_bytes += std::mem::size_of::<Event>();
        }

        let restored_oldest = events.iter().map(|event| event.timestamp).min();
        let restored_newest = events.iter().map(|event| event.timestamp).max();
        buffer.oldest_timestamp_bound = match (buffer.oldest_timestamp_bound, restored_oldest) {
            (Some(existing), Some(restored)) => Some(existing.min(restored)),
            (None, restored) => restored,
            (existing, None) => existing,
        };
        buffer.newest_timestamp_bound = match (buffer.newest_timestamp_bound, restored_newest) {
            (Some(existing), Some(restored)) => Some(existing.max(restored)),
            (None, restored) => restored,
            (existing, None) => existing,
        };
    }

    pub(crate) fn write_segment_files(
        config: &StreamingConfig,
        events: &mut [Event],
    ) -> std::io::Result<EnhancedSegmentMetadata> {
        if events.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot write an empty segment",
            ));
        }

        events.sort_by_key(|e| e.timestamp);

        let segment_id = Self::generate_segment_id();
        let data_path = format!("{}/segment-{}.log", config.segment_base_path, segment_id);
        let index_path = format!("{}/segment-{}.idx", config.segment_base_path, segment_id);

        let mut data_file = BufWriter::new(std::fs::File::create(&data_path)?);
        let mut index_file = BufWriter::new(std::fs::File::create(&index_path)?);

        let mut index_directory = Vec::new();
        let mut current_block_entries = Vec::new();
        let mut current_block_min_ts = None;
        let mut current_block_max_ts = 0u64;
        let mut data_offset = 0u64;

        for (record_count, event) in events.iter().enumerate() {
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
        Self::write_subject_index_file(&data_path, events)?;
        Self::validate_subject_index_file(&data_path)?;

        if !index_directory.is_empty() {
            let last_event_ts = events.last().unwrap().timestamp;
            for i in 0..index_directory.len() {
                if i + 1 < index_directory.len() {
                    index_directory[i].max_timestamp = index_directory[i + 1].min_timestamp;
                } else {
                    index_directory[i].max_timestamp = last_event_ts;
                }
            }
        }

        Ok(EnhancedSegmentMetadata {
            start_timstamp: events.first().unwrap().timestamp,
            end_timestamp: events.last().unwrap().timestamp,
            data_path,
            index_path,
            record_count: events.len() as u64,
            index_directory,
        })
    }

    /// Persist compact subject-to-log-offset entries. The `.sidx` format has
    /// no header and consists of 12-byte entries: a little-endian `u32`
    /// dictionary subject ID followed by a little-endian `u64` byte offset in
    /// the corresponding fixed-size `.log`. Entries are sorted by `(subject
    /// ID, log offset)`, so equal subjects are contiguous. Current lookup
    /// validates and linearly scans the entire sidecar (it does not yet seek a
    /// subject range); it only opens a segment log after finding a bound ID.
    /// Segment logs remain authoritative and the sidecar stores no RDF terms.
    fn write_subject_index_file(data_path: &str, events: &[Event]) -> std::io::Result<()> {
        let index_path = data_path.strip_suffix(".log").unwrap_or(data_path).to_string() + ".sidx";
        let mut entries = events
            .iter()
            .enumerate()
            .map(|(position, event)| (event.subject, position as u64 * RECORD_SIZE as u64))
            .collect::<Vec<_>>();
        entries.sort_unstable();
        let mut file = BufWriter::new(std::fs::File::create(index_path)?);
        for (subject, offset) in entries {
            file.write_all(&subject.to_le_bytes())?;
            file.write_all(&offset.to_le_bytes())?;
        }
        file.flush()
    }

    /// Immutable persisted sidecars are fully validated at flush finalization
    /// and again on reopen, so binary lookup need not re-scan them per query.
    pub(super) fn validate_subject_index_file(data_path: &str) -> std::io::Result<()> {
        let path = data_path.strip_suffix(".log").unwrap_or(data_path).to_string() + ".sidx";
        let log_len = std::fs::metadata(data_path)?.len();
        let mut file = std::fs::File::open(&path)?;
        if file.metadata()?.len() % 12 != 0 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("truncated subject index {path}"))); }
        let mut previous=None; let mut entry=[0u8;12];
        while file.read_exact(&mut entry).is_ok() {
            let subject=u32::from_le_bytes(entry[0..4].try_into().unwrap()); let offset=u64::from_le_bytes(entry[4..12].try_into().unwrap());
            if previous.is_some_and(|last| last>(subject,offset)) || offset % RECORD_SIZE as u64 != 0 || offset > log_len || log_len-offset < RECORD_SIZE as u64 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid subject index {path}"))); }
            previous=Some((subject,offset));
        }
        Ok(())
    }

    pub(super) fn serialize_event_to_fixed_size_static(event: &Event) -> Vec<u8> {
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

    pub(super) fn flush_index_block_static(
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

    pub(super) fn generate_segment_id() -> u64 {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let mut candidate = NEXT_SEGMENT_ID.load(Ordering::Relaxed);

        loop {
            let next = now_ms.max(candidate.saturating_add(1));
            match NEXT_SEGMENT_ID.compare_exchange(
                candidate,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return next,
                Err(observed) => candidate = observed,
            }
        }
    }

    pub(super) fn load_existing_segments(&self) -> std::io::Result<()> {
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
                    if let Some(id_str) =
                        filename.strip_prefix("segment-").and_then(|s| s.strip_suffix(".log"))
                    {
                        if let Ok(segment_id) = id_str.parse::<u64>() {
                            let data_path = format!("{}/segment-{}.log", segment_dir, segment_id);
                            let index_path = format!("{}/segment-{}.idx", segment_dir, segment_id);

                            if let Ok(_metadata) = fs::metadata(&data_path) {
                                let subject_path = data_path.strip_suffix(".log").unwrap_or(&data_path).to_string() + ".sidx";
                                if fs::metadata(&subject_path).is_ok() { Self::validate_subject_index_file(&data_path)?; }
                                let (start_ts, end_ts, record_count) =
                                    Self::load_segment_log_metadata(&data_path)?;
                                let mut index_directory = if fs::metadata(&index_path).is_ok() {
                                    Self::load_index_directory_from_file(
                                        &index_path,
                                        self.config.entries_per_index_block,
                                    )?
                                } else {
                                    Vec::new()
                                };

                                for block_index in 0..index_directory.len() {
                                    index_directory[block_index].max_timestamp =
                                        if block_index + 1 < index_directory.len() {
                                            index_directory[block_index + 1].min_timestamp
                                        } else {
                                            end_ts
                                        };
                                }

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

        segments.sort_by_key(|s| s.start_timstamp);

        {
            let mut self_segments = self.segments.write().unwrap();
            *self_segments = segments;
        }

        Ok(())
    }

    fn load_segment_log_metadata(data_path: &str) -> std::io::Result<(u64, u64, u64)> {
        let mut file = std::fs::File::open(data_path)?;
        let byte_len = file.metadata()?.len();
        if byte_len == 0 || byte_len % RECORD_SIZE as u64 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Segment log '{data_path}' is empty or truncated"),
            ));
        }

        let record_count = byte_len / RECORD_SIZE as u64;
        let mut record = [0u8; RECORD_SIZE];
        file.read_exact(&mut record)?;
        let (start_timestamp, ..) = crate::core::encoding::decode_record(&record);
        file.seek(SeekFrom::Start((record_count - 1) * RECORD_SIZE as u64))?;
        file.read_exact(&mut record)?;
        let (end_timestamp, ..) = crate::core::encoding::decode_record(&record);
        Ok((start_timestamp, end_timestamp, record_count))
    }

    pub(super) fn load_index_directory_from_file(
        index_path: &str,
        entries_per_index_block: usize,
    ) -> std::io::Result<Vec<IndexBlock>> {
        const INDEX_ENTRY_SIZE: usize = 16;
        if entries_per_index_block == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "entries_per_index_block must be greater than zero",
            ));
        }
        let mut file = std::fs::File::open(index_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        if buffer.is_empty() {
            return Ok(Vec::new());
        }
        if buffer.len() % INDEX_ENTRY_SIZE != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Sparse index '{index_path}' is truncated"),
            ));
        }

        let mut index_directory = Vec::new();
        let mut file_offset = 0u64;
        let mut current_block_start = 0;

        while current_block_start < buffer.len() {
            let block_size = std::cmp::min(
                entries_per_index_block * INDEX_ENTRY_SIZE,
                buffer.len() - current_block_start,
            );
            let block_end = current_block_start + block_size;
            let block_entries = block_end - current_block_start;
            let entry_count = (block_entries / INDEX_ENTRY_SIZE) as u32;

            let first_ts = u64::from_le_bytes(
                buffer[current_block_start..current_block_start + 8].try_into().unwrap(),
            );
            let last_entry_start =
                current_block_start + ((entry_count - 1) as usize * INDEX_ENTRY_SIZE);
            let last_ts = u64::from_le_bytes(
                buffer[last_entry_start..last_entry_start + 8].try_into().unwrap(),
            );

            index_directory.push(IndexBlock {
                min_timestamp: first_ts,
                max_timestamp: last_ts,
                file_offset,
                entry_count,
            });

            file_offset += block_size as u64;
            current_block_start = block_end;
        }

        Ok(index_directory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;

    #[test]
    fn recovery_uses_log_metadata_and_configured_index_block_size() {
        let temp_dir = TempDir::new().unwrap();
        let config = StreamingConfig {
            segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
            max_batch_events: 100,
            max_batch_age_seconds: 60,
            max_batch_bytes: 1_000_000,
            sparse_interval: 2,
            entries_per_index_block: 2,
        };

        {
            let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
            for timestamp in (10..=70).step_by(10) {
                storage
                    .write(Event { timestamp, subject: 0, predicate: 0, object: 0, graph: 0 })
                    .unwrap();
            }
            storage.flush().unwrap();
        }

        let storage = StreamingSegmentedStorage::new(config).unwrap();
        let segments = storage.segments.read().unwrap();
        assert_eq!(segments.len(), 1);
        let segment = &segments[0];
        assert_eq!(segment.record_count, 7);
        assert_eq!(segment.start_timstamp, 10);
        assert_eq!(segment.end_timestamp, 70);
        assert_eq!(segment.index_directory.len(), 2);
        assert_eq!(segment.index_directory.last().unwrap().max_timestamp, 70);
        drop(segments);

        let tail = storage.query(70, 70).unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].timestamp, 70);
    }

    #[test]
    fn persisted_subject_index_matches_timestamp_filter_and_prunes_records() {
        let temp_dir = TempDir::new().unwrap();
        let config = StreamingConfig {
            segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
            max_batch_events: 100,
            max_batch_age_seconds: 60,
            max_batch_bytes: 1_000_000,
            sparse_interval: 2,
            entries_per_index_block: 2,
        };
        let subjects = (0..10).map(|n| format!("https://example.org/s{n}")).collect::<Vec<_>>();
        {
            let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
            for timestamp in 0..100 {
                storage
                    .write_rdf(timestamp, &subjects[(timestamp % 10) as usize], "p", "1", "g")
                    .unwrap();
            }
            storage.flush().unwrap();
        }
        let storage = StreamingSegmentedStorage::new(config).unwrap();
        let bindings =
            [subjects[1].clone(), subjects[3].clone()].into_iter().collect::<HashSet<_>>();
        let full = storage
            .query_rdf_half_open(20, 80)
            .unwrap()
            .into_iter()
            .filter(|row| bindings.contains(&row.subject))
            .collect::<Vec<_>>();
        let (indexed, metrics) = storage
            .query_rdf_half_open_for_subjects_with_metrics(20, 80, &bindings)
            .unwrap();
        let key = |row: &crate::core::RDFEvent| {
            format!(
                "{}|{}|{}|{}|{}",
                row.timestamp, row.subject, row.predicate, row.object, row.graph
            )
        };
        assert_eq!(
            indexed.iter().map(key).collect::<Vec<_>>(),
            full.iter().map(key).collect::<Vec<_>>()
        );
        assert_eq!(metrics.records_returned as usize, indexed.len());
        assert!(metrics.records_examined < 100);
        // Lookup is currently a linear sidecar scan, so this is the actual
        // total inspected entry count rather than a match/binding count.
        assert_eq!(metrics.index_entries_examined, 100);
        assert!(metrics.segments_touched >= 1);
    }

    fn subject_fixture() -> (TempDir, StreamingConfig, Vec<String>) {
        let temp = TempDir::new().unwrap();
        let config = StreamingConfig {
            segment_base_path: temp.path().to_string_lossy().into_owned(),
            max_batch_events: 3,
            max_batch_age_seconds: 60,
            max_batch_bytes: 1_000_000,
            sparse_interval: 2,
            entries_per_index_block: 2,
        };
        let subjects = (0..10).map(|id| format!("https://example.org/s{id}")).collect::<Vec<_>>();
        let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
        for timestamp in 0..12 {
            storage
                .write_rdf(timestamp, &subjects[(timestamp % 10) as usize], "p", "1", "g")
                .unwrap();
            if (timestamp + 1) % 3 == 0 {
                storage.flush().unwrap();
            }
        }
        (temp, config, subjects)
    }

    #[test]
    fn subject_access_is_equivalent_for_binding_cases_and_half_open_bounds() {
        let (_temp, config, subjects) = subject_fixture();
        let storage = StreamingSegmentedStorage::new(config).unwrap();
        for bindings in [
            vec![subjects[0].clone()],
            vec![subjects[0].clone(), subjects[1].clone()],
            subjects.clone(),
            vec!["https://example.org/absent".into()],
            vec![subjects[0].clone(), "https://example.org/absent".into()],
            vec![],
        ] {
            let bindings = bindings.into_iter().collect::<HashSet<_>>();
            let expected = storage
                .query_rdf_half_open(1, 11)
                .unwrap()
                .into_iter()
                .filter(|row| bindings.contains(&row.subject))
                .collect::<Vec<_>>();
            let (actual, metrics) =
                storage.query_rdf_half_open_for_subjects_with_metrics(1, 11, &bindings).unwrap();
            let canonical = |rows: Vec<crate::core::RDFEvent>| {
                rows.into_iter()
                    .map(|row| (row.timestamp, row.subject, row.predicate, row.object, row.graph))
                    .collect::<Vec<_>>()
            };
            let actual_len = actual.len();
            assert_eq!(canonical(actual), canonical(expected));
            assert_eq!(metrics.records_returned as usize, actual_len);
        }
        let rows = storage.query_rdf_half_open(3, 4).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].timestamp, 3);
        assert!(storage.query_rdf_half_open(20, 21).unwrap().is_empty());
        let (_, metrics) = storage.query_half_open_with_metrics(1, 11).unwrap();
        assert_eq!(metrics.records_returned, 10);
        assert_eq!(metrics.records_matched, 10);
        assert!(metrics.records_examined >= metrics.records_matched);
    }

    #[test]
    fn missing_sidecar_falls_back_but_corrupt_sidecar_is_an_error() {
        let (temp, config, subjects) = subject_fixture();
        let sidecars = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "sidx"))
            .collect::<Vec<_>>();
        std::fs::remove_file(&sidecars[0]).unwrap();
        let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
        let bindings = [subjects[0].clone()].into_iter().collect::<HashSet<_>>();
        let (rows, metrics) =
            storage.query_rdf_half_open_for_subjects_with_metrics(0, 12, &bindings).unwrap();
        assert!(!rows.is_empty());
        assert!(!metrics.subject_index_used); // mixed index/fallback access is not fully indexed.
        assert!(metrics.segments_touched >= 1);
        drop(storage);
        let sidecar = sidecars[1].clone();
        std::fs::write(&sidecar, [1u8; 11]).unwrap();
        let storage = StreamingSegmentedStorage::new(config).unwrap();
        let error = storage
            .query_rdf_half_open_for_subjects_with_metrics(0, 12, &bindings)
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn subject_index_rejects_unaligned_out_of_range_and_incomplete_record_offsets() {
        for offset in [1u64, u64::MAX, crate::core::encoding::RECORD_SIZE as u64] {
            let (temp, config, subjects) = subject_fixture();
            let sidecar = std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|extension| extension == "sidx"))
                .unwrap();
            let mut bytes = std::fs::read(&sidecar).unwrap();
            bytes[4..12].copy_from_slice(&offset.to_le_bytes());
            std::fs::write(sidecar, bytes).unwrap();
            // For the aligned offset case make the log incomplete after that offset.
            if offset == crate::core::encoding::RECORD_SIZE as u64 {
                let log = std::fs::read_dir(temp.path())
                    .unwrap()
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| path.extension().is_some_and(|extension| extension == "log"))
                    .unwrap();
                std::fs::write(&log, vec![0u8; crate::core::encoding::RECORD_SIZE + 1]).unwrap();
            }
            let bindings = [subjects[0].clone()].into_iter().collect::<HashSet<_>>();
            match StreamingSegmentedStorage::new(config) {
                Ok(storage) => assert_eq!(
                    storage
                        .query_rdf_half_open_for_subjects_with_metrics(0, 12, &bindings)
                        .unwrap_err()
                        .kind(),
                    std::io::ErrorKind::InvalidData
                ),
                Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::InvalidData),
            }
        }
    }

    #[test]
    fn storage_size_accounting_matches_files() {
        let (temp, config, _) = subject_fixture();
        let storage = StreamingSegmentedStorage::new(config).unwrap();
        let expected = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let length = entry.metadata().ok()?.len();
                Some((name, length))
            })
            .fold((0, 0), |(base, subject), (name, length)| {
                if name == "dictionary.bin" || name.ends_with(".log") || name.ends_with(".idx") {
                    (base + length, subject)
                } else if name.ends_with(".sidx") {
                    (base, subject + length)
                } else {
                    (base, subject)
                }
            });
        let actual = storage.storage_size_accounting().unwrap();
        assert_eq!((actual.base_storage_bytes, actual.subject_index_bytes), expected);
        assert_eq!(actual.total_storage_bytes, expected.0 + expected.1);
        assert_eq!(actual.index_entry_count, expected.1 / 12);
    }

    #[test]
    fn multi_segment_subject_access_touches_only_logs_with_bound_subjects_after_reopen() {
        let temp = TempDir::new().unwrap();
        let config = StreamingConfig {
            segment_base_path: temp.path().to_string_lossy().into_owned(),
            max_batch_events: 3,
            max_batch_age_seconds: 60,
            max_batch_bytes: 1_000_000,
            sparse_interval: 2,
            entries_per_index_block: 2,
        };
        let rows = [
            (0, "first"),
            (1, "common"),
            (2, "other"),
            (3, "other"),
            (4, "common"),
            (5, "other"),
            (6, "other"),
            (7, "common"),
            (8, "last"),
        ];
        {
            let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
            for (timestamp, subject) in rows {
                storage.write_rdf(timestamp, subject, "p", "1", "g").unwrap();
                if (timestamp + 1) % 3 == 0 {
                    storage.flush().unwrap();
                }
            }
        }
        let storage = StreamingSegmentedStorage::new(config).unwrap();
        let bindings =
            ["first".to_string(), "last".to_string()].into_iter().collect::<HashSet<_>>();
        let expected = storage
            .query_rdf_half_open(0, 9)
            .unwrap()
            .into_iter()
            .filter(|row| bindings.contains(&row.subject))
            .collect::<Vec<_>>();
        let (actual, metrics) =
            storage.query_rdf_half_open_for_subjects_with_metrics(0, 9, &bindings).unwrap();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(metrics.records_examined, 2);
        assert_eq!(metrics.records_matched, 2);
        assert_eq!(metrics.records_returned, 2);
        assert_eq!(metrics.segments_touched, 2); // middle log was never opened/read.
        assert_eq!(metrics.index_entries_examined, 9); // all three sidecars were inspected.
        assert!(metrics.subject_index_used);
    }
}
