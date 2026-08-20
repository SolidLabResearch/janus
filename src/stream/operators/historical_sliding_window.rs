use crate::core::Event;
use crate::parsing::janusql_parser::WindowDefinition;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use std::rc::Rc;

/// Operator for processing historical data with a sliding window.
/// It iterates over the storage and yields events for each window.
pub struct HistoricalSlidingWindowOperator {
    storage: Rc<StreamingSegmentedStorage>,
    window_def: WindowDefinition,
    current_evaluation_time: u64,
    latest_evaluation_time: u64,
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

        HistoricalSlidingWindowOperator {
            storage,
            window_def,
            current_evaluation_time: now,
            latest_evaluation_time: now,
        }
    }
}

impl Iterator for HistoricalSlidingWindowOperator {
    type Item = Vec<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        let (window_start, window_end) =
            self.window_def.resolve_historical_bounds(self.current_evaluation_time)?;

        if window_end > self.latest_evaluation_time {
            return None;
        }

        let events_result = self.storage.query(window_start, window_end);

        match events_result {
            Ok(events) => {
                // Advance the window
                self.current_evaluation_time =
                    self.current_evaluation_time.checked_add(self.window_def.slide)?;
                Some(events)
            }
            Err(e) => {
                eprintln!("Error querying storage for window: {}", e);
                None
            }
        }
    }
}
