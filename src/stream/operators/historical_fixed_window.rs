use crate::core::Event;
use crate::parsing::janusql_parser::WindowDefinition;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use std::rc::Rc;

/// Operator for processing historical data with a fixed window.
/// Unlike sliding windows, this queries a single fixed time range [start, end].
pub struct HistoricalFixedWindowOperator {
    storage: Rc<StreamingSegmentedStorage>,
    window_def: WindowDefinition,
    has_yielded: bool,
}

impl HistoricalFixedWindowOperator {
    /// Creates a new HistoricalFixedWindowOperator.
    ///
    /// # Arguments
    ///
    /// * `storage` - The storage backend to query.
    /// * `window_def` - The window definition with start and end timestamps.
    pub fn new(storage: Rc<StreamingSegmentedStorage>, window_def: WindowDefinition) -> Self {
        HistoricalFixedWindowOperator { storage, window_def, has_yielded: false }
    }
}

impl Iterator for HistoricalFixedWindowOperator {
    type Item = Vec<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        // Fixed window only yields once
        if self.has_yielded {
            return None;
        }

        // Start and end are mandatory for HistoricalFixed windows
        let start = self.window_def.start.expect("Start must be defined for HistoricalFixedWindow");
        let end = self.window_def.end.expect("End must be defined for HistoricalFixedWindow");

        // Query the storage for events in the fixed window
        let events_result = self.storage.query(start, end);

        self.has_yielded = true;

        match events_result {
            Ok(events) => Some(events),
            Err(e) => {
                eprintln!("Error querying storage for fixed window: {}", e);
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
    fn test_historical_fixed_window() {
        let test_dir = "/tmp/janus_test_fixed_window";
        let _ = fs::remove_dir_all(test_dir);

        let config = create_test_config(test_dir);
        let storage = Rc::new(StreamingSegmentedStorage::new(config).unwrap());

        // Write events at timestamps 100, 200, 300, 400, 500, 600
        for i in 1..=6 {
            storage.write_rdf(i * 100, "s", "p", "o", "g").unwrap();
        }

        // Define Fixed Window: [200, 500]
        let window_def = WindowDefinition {
            window_name: "w1".to_string(),
            stream_name: "s1".to_string(),
            width: 0,
            slide: 0,
            offset: None,
            start: Some(200),
            end: Some(500),
            window_type: WindowType::HistoricalFixed,
        };

        let mut operator = HistoricalFixedWindowOperator::new(storage.clone(), window_def);

        // Should yield once with events in [200, 500]
        let w1 = operator.next().unwrap();
        assert_eq!(w1.len(), 4); // Events at 200, 300, 400, 500
        assert_eq!(w1[0].timestamp, 200);
        assert_eq!(w1[3].timestamp, 500);

        // Should not yield again
        assert!(operator.next().is_none());

        let _ = fs::remove_dir_all(test_dir);
    }
}
