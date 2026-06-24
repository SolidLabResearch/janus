use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use crate::{
    core::Event,
    storage::{
        indexing::dictionary::Dictionary,
        util::{BatchBuffer, EnhancedSegmentMetadata, StreamingConfig},
    },
};

use super::StreamingSegmentedStorage;

impl StreamingSegmentedStorage {
    pub(super) fn background_flush_loop(
        batch_buffer: Arc<RwLock<BatchBuffer>>,
        segments: Arc<RwLock<Vec<EnhancedSegmentMetadata>>>,
        shutdown_signal: Arc<Mutex<bool>>,
        background_flush_error: Arc<Mutex<Option<String>>>,
        config: StreamingConfig,
        dictionary: Arc<RwLock<Dictionary>>,
    ) {
        while !*shutdown_signal.lock().unwrap() {
            std::thread::sleep(Duration::from_millis(100));

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
                if let Err(e) = Self::flush_background(
                    batch_buffer.clone(),
                    segments.clone(),
                    config.clone(),
                    dictionary.clone(),
                ) {
                    let message = format!("Background flush failed: {}", e);
                    eprintln!("{}", message);
                    *background_flush_error.lock().unwrap() = Some(message);
                    break;
                }
            }
        }
    }

    fn flush_background(
        batch_buffer: Arc<RwLock<BatchBuffer>>,
        segments: Arc<RwLock<Vec<EnhancedSegmentMetadata>>>,
        config: StreamingConfig,
        dictionary: Arc<RwLock<Dictionary>>,
    ) -> std::io::Result<()> {
        let mut events_to_flush = {
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

        let events_ref = &mut events_to_flush;
        let flush_result = (|| -> std::io::Result<()> {
            let new_segment = Self::write_segment_files(&config, events_ref)?;

            {
                let mut segments = segments.write().unwrap();
                segments.push(new_segment);
                segments.sort_by_key(|s| s.start_timstamp);
            }

            let dict_path = std::path::Path::new(&config.segment_base_path).join("dictionary.bin");
            let dict = dictionary.read().unwrap();
            dict.save_to_file(&dict_path)?;

            Ok(())
        })();

        if let Err(err) = flush_result {
            Self::restore_failed_background_flush(&batch_buffer, &events_to_flush);
            return Err(err);
        }

        Ok(())
    }

    fn restore_failed_background_flush(batch_buffer: &Arc<RwLock<BatchBuffer>>, events: &[Event]) {
        if events.is_empty() {
            return;
        }

        let mut buffer = batch_buffer.write().unwrap();
        for event in events.iter().rev().cloned() {
            buffer.events.push_front(event);
            buffer.total_bytes += std::mem::size_of::<Event>();
        }

        let restored_oldest = events.first().map(|event| event.timestamp);
        let restored_newest = events.last().map(|event| event.timestamp);

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
}
