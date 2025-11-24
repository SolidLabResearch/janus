use crate::core::Event;
use crate::parsing::janusql_parser::WindowDefinition;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use std::rc::Rc;

/// Operator for processing historical data with a sliding window.
/// It iterates over the storage and yields events for each window.
pub struct HistoricalSlidingWindowOperator {
    storage: Rc<StreamingSegmentedStorage>,
    window_def: WindowDefinition,
    current_start: u64,
    end_bound: u64,
}

impl HistoricalSlidingWindowOperator {
    /// Creates a new HistoricalSlidingWindowOperator.
    ///
    /// # Arguments
    ///
    /// * `storage` - The storage backend to query.
    /// * `window_def` - The window definition (width, slide, offset, etc.).
    pub fn new(storage: Rc<StreamingSegmentedStorage>, window_def: WindowDefinition) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Offset is mandatory for HistoricalSliding windows as per the parser and requirements.
        // We subtract it from the query_start to "go back" in time.
        let offset = window_def.offset.expect("Offset must be defined for HistoricalSlidingWindow");
        let start_time = now.saturating_sub(offset);

        HistoricalSlidingWindowOperator {
            storage,
            window_def,
            current_start: start_time,
            end_bound: now,
        }
    }
}

impl Iterator for HistoricalSlidingWindowOperator {
    type Item = Vec<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        // Calculate the window bounds
        let window_start = self.current_start;
        let window_end = (window_start + self.window_def.width).min(self.end_bound);

        // Check if we have exceeded the query range
        // We stop if the window start goes beyond the end bound.
        // (Alternative: stop if window_end > end_bound, depending on strict containment requirements)
        if window_start > self.end_bound {
            return None;
        }

        // Query the storage for events in this window
        // Note: query() is inclusive, so we might need to adjust if we want [start, end)
        // For now, we assume the storage query semantics match what we want or we accept inclusive.
        // Usually windows are [start, end).
        let events_result = self.storage.query(window_start, window_end);

        match events_result {
            Ok(events) => {
                // Advance the window
                self.current_start += self.window_def.slide;
                Some(events)
            }
            Err(e) => {
                eprintln!("Error querying storage for window: {}", e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::janusql_parser::WindowType;
    use crate::storage::util::StreamingConfig;
    use std::fs;

    fn create_test_config(path: &str) -> StreamingConfig {
        StreamingConfig {
            segment_base_path: path.to_string(),
            max_batch_events: 10,
            max_batch_bytes: 1024,
            max_batch_age_seconds: 1,
            sparse_interval: 2,
            entries_per_index_block: 2,
        }
    }

    #[test]
    fn test_historical_sliding_window() {
        let test_dir = "/tmp/janus_test_sliding_window";
        let _ = fs::remove_dir_all(test_dir); // Clean up before test

        let config = create_test_config(test_dir);
        let storage = Rc::new(StreamingSegmentedStorage::new(config).unwrap());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Write events in the past: now-500, now-400, now-300, now-200, now-100, now
        for i in 0..6 {
            let ts = now - (500 - (i * 100));
            storage.write_rdf(ts, "s", "p", "o", "g").unwrap();
        }

        // Define Window: Width 200, Slide 100, Offset 500 (Start at now - 500)
        let window_def = WindowDefinition {
            window_name: "w1".to_string(),
            stream_name: "s1".to_string(),
            width: 200,
            slide: 100,
            offset: Some(500),
            start: None,
            end: None,
            window_type: WindowType::HistoricalSliding,
        };

        let mut operator = HistoricalSlidingWindowOperator::new(storage.clone(), window_def);

        // Window 1: [now-500, now-300] -> Events at now-500, now-400, now-300
        // Note: query is inclusive.
        let w1 = operator.next().unwrap();
        assert_eq!(w1.len(), 3);
        assert_eq!(w1[0].timestamp, now - 500);
        assert_eq!(w1[2].timestamp, now - 300);

        // Window 2: [now-400, now-200] -> Events at now-400, now-300, now-200
        let w2 = operator.next().unwrap();
        assert_eq!(w2.len(), 3);
        assert_eq!(w2[0].timestamp, now - 400);
        assert_eq!(w2[2].timestamp, now - 200);

        // Window 3: [now-300, now-100] -> Events at now-300, now-200, now-100
        let w3 = operator.next().unwrap();
        assert_eq!(w3.len(), 3);
        assert_eq!(w3[0].timestamp, now - 300);
        assert_eq!(w3[2].timestamp, now - 100);

        let _ = fs::remove_dir_all(test_dir);
    }
}
