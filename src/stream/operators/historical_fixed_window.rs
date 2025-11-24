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
