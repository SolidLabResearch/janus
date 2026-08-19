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
    evaluation_time: u64,
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
            evaluation_time: now,
        }
    }
}

impl Iterator for HistoricalSlidingWindowOperator {
    type Item = Vec<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        let window_start = self.current_start;
        let window_end = window_start.checked_add(self.window_def.width)?;

        if window_end > self.evaluation_time {
            return None;
        }

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
