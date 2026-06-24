use std::{
    io::{BufWriter, Seek, Write},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    core::{
        encoding::{encode_record, RECORD_SIZE},
        Event,
    },
    storage::util::{EnhancedSegmentMetadata, IndexBlock, StreamingConfig},
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

        let segment = Self::write_segment_files(&self.config, &mut events_to_flush)?;

        {
            let mut segments = self.segments.write().unwrap();
            segments.push(segment);
            segments.sort_by_key(|s| s.start_timstamp);
        }

        self.save_dictionary()?;
        Ok(())
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
                                let (index_directory, start_ts, end_ts, record_count) =
                                    if fs::metadata(&index_path).is_ok() {
                                        Self::load_index_directory_from_file(&index_path)
                                            .unwrap_or_else(|_| (Vec::new(), 0, u64::MAX, 0))
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

        segments.sort_by_key(|s| s.start_timstamp);

        {
            let mut self_segments = self.segments.write().unwrap();
            *self_segments = segments;
        }

        Ok(())
    }

    pub(super) fn load_index_directory_from_file(
        index_path: &str,
    ) -> std::io::Result<(Vec<IndexBlock>, u64, u64, u64)> {
        use std::io::Read;

        let mut file = std::fs::File::open(index_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        if buffer.is_empty() {
            return Ok((Vec::new(), 0, u64::MAX, 0));
        }

        let mut index_directory = Vec::new();
        let mut file_offset = 0u64;
        let mut global_min_ts = u64::MAX;
        let mut global_max_ts = 0u64;
        let mut total_records = 0u64;

        let entries_per_block = 1000;
        let mut current_block_start = 0;

        while current_block_start < buffer.len() {
            let block_size =
                std::cmp::min(entries_per_block * 16, buffer.len() - current_block_start);
            let block_end = current_block_start + block_size;
            let block_entries = block_end - current_block_start;
            let entry_count = (block_entries / 16) as u32;

            if entry_count == 0 {
                break;
            }

            let first_ts = u64::from_le_bytes(
                buffer[current_block_start..current_block_start + 8].try_into().unwrap(),
            );
            let last_entry_start = current_block_start + ((entry_count - 1) as usize * 16);
            let last_ts = u64::from_le_bytes(
                buffer[last_entry_start..last_entry_start + 8].try_into().unwrap(),
            );

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
}
