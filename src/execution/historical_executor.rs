//! Historical Query Executor
//!
//! This module provides the `HistoricalExecutor` which executes SPARQL queries
//! over historical RDF data using window operators and storage backend.
//!
//! # Architecture
//!
//! The executor orchestrates:
//! 1. Window operators (Fixed/Sliding) to fetch Event data from storage
//! 2. Dictionary decoding to convert Event → RDFEvent
//! 3. RDF conversion to transform RDFEvent → Quad
//! 4. SPARQL execution via OxigraphAdapter
//! 5. Result formatting as structured bindings

use crate::api::janus_api::JanusApiError;
use crate::core::{Event, RDFEvent};
use crate::execution::rdf_conversion::rdf_event_to_quad;
use crate::parsing::janusql_parser::WindowDefinition;
use crate::querying::oxigraph_adapter::OxigraphAdapter;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use crate::stream::operators::historical_fixed_window::HistoricalFixedWindowOperator;
use crate::stream::operators::historical_sliding_window::HistoricalSlidingWindowOperator;
use oxigraph::model::{GraphName, NamedNode, Quad};
use rsp_rs::QuadContainer;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

/// Executor for historical SPARQL queries over stored RDF data.
///
/// # Example
///
/// ```ignore
/// let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
///
/// let bindings = executor.execute_fixed_window(&window_def, sparql_query)?;
/// for binding in bindings {
///     println!("Result: {:?}", binding);
/// }
/// ```
pub struct HistoricalExecutor {
    storage: Arc<StreamingSegmentedStorage>,
    sparql_engine: OxigraphAdapter,
}

impl HistoricalExecutor {
    /// Creates a new HistoricalExecutor.
    ///
    /// # Arguments
    ///
    /// * `storage` - Shared reference to the segmented storage backend
    /// * `sparql_engine` - SPARQL query engine (OxigraphAdapter)
    pub fn new(storage: Arc<StreamingSegmentedStorage>, sparql_engine: OxigraphAdapter) -> Self {
        Self { storage, sparql_engine }
    }

    /// Execute a fixed window query that returns results once.
    ///
    /// # Arguments
    ///
    /// * `window` - Window definition with start and end timestamps
    /// * `sparql_query` - SPARQL SELECT query string
    ///
    /// # Returns
    ///
    /// A vector of HashMaps where each HashMap represents one solution with
    /// variable bindings (variable name → value).
    ///
    /// # Errors
    ///
    /// Returns `JanusApiError` if:
    /// - Window definition is invalid
    /// - Storage query fails
    /// - Event decoding fails
    /// - SPARQL execution fails
    pub fn execute_fixed_window(
        &self,
        window: &WindowDefinition,
        sparql_query: &str,
    ) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
        let (start, end) = window
            .resolve_historical_bounds(window.end.unwrap_or_default())
            .ok_or_else(|| {
                JanusApiError::ExecutionError(
                    "Fixed window requires start/end timestamps".to_string(),
                )
            })?;
        self.execute_window_bounds(start, end, sparql_query)
    }

    /// Execute a historical query over explicit bounds.
    pub fn execute_window_bounds(
        &self,
        start: u64,
        end: u64,
        sparql_query: &str,
    ) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
        let events = self
            .storage
            .query(start, end)
            .map_err(|e| JanusApiError::StorageError(format!("Failed to query storage: {}", e)))?;

        self.execute_sparql_on_events(&events, sparql_query)
    }

    /// Execute one historical materialized result over one or more historical windows by
    /// loading each window into a synthetic named graph keyed by the JanusQL window name.
    pub fn execute_materialized_historical_subquery(
        &self,
        windows: &[&WindowDefinition],
        sparql_query: &str,
        evaluation_time: u64,
    ) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
        let mut quads = Vec::new();
        let mut timestamps = Vec::new();

        for window in windows {
            let (start, end) =
                window.resolve_historical_bounds(evaluation_time).ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Failed to resolve historical bounds for window '{}'",
                        window.window_name
                    ))
                })?;
            let events = self.storage.query(start, end).map_err(|e| {
                JanusApiError::StorageError(format!("Failed to query storage: {}", e))
            })?;
            timestamps.extend(events.iter().map(|event| event.timestamp));
            let rdf_events = self.decode_events(&events)?;
            quads.extend(
                self.rdf_events_to_quads_for_window_graph(&rdf_events, &window.window_name)?,
            );
        }

        let max_timestamp = timestamps.into_iter().max().unwrap_or(evaluation_time);
        let container = self.build_quad_container_from_quads(quads, max_timestamp)?;
        self.sparql_engine
            .execute_query_bindings(sparql_query, &container)
            .map_err(|e| JanusApiError::ExecutionError(format!("SPARQL execution failed: {}", e)))
    }

    /// Execute a sliding window query that returns an iterator of results.
    ///
    /// # Arguments
    ///
    /// * `window` - Window definition with width, slide, and offset
    /// * `sparql_query` - SPARQL SELECT query string
    ///
    /// # Returns
    ///
    /// An iterator where each item is a Result containing a vector of bindings
    /// for one window's SPARQL results.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for window_result in executor.execute_sliding_windows(&window_def, query)? {
    ///     match window_result {
    ///         Ok(bindings) => println!("Window results: {:?}", bindings),
    ///         Err(e) => eprintln!("Window error: {}", e),
    ///     }
    /// }
    /// ```
    pub fn execute_sliding_windows<'a>(
        &'a self,
        window: &WindowDefinition,
        sparql_query: &'a str,
    ) -> impl Iterator<Item = Result<Vec<HashMap<String, String>>, JanusApiError>> + 'a {
        let offset = window.offset.unwrap_or(0);
        let width = window.width;
        let slide = window.slide;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let start_time = now.saturating_sub(offset);

        SlidingWindowIterator {
            executor: self,
            current_start: start_time,
            evaluation_time: now,
            width,
            slide,
            sparql_query: sparql_query.to_string(),
        }
    }

    /// Core conversion and execution logic for a set of events.
    ///
    /// # Process
    ///
    /// 1. Decode Event → RDFEvent using Dictionary
    /// 2. Convert RDFEvent → Quad with proper URI parsing
    /// 3. Build QuadContainer for SPARQL engine
    /// 4. Execute SPARQL query with structured bindings
    ///
    /// # Arguments
    ///
    /// * `events` - Slice of internal Event structs (24-byte format)
    /// * `sparql_query` - SPARQL SELECT query string
    ///
    /// # Returns
    ///
    /// Vector of solution bindings (variable name → value)
    fn execute_sparql_on_events(
        &self,
        events: &[Event],
        sparql_query: &str,
    ) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
        // 1. Decode Event → RDFEvent
        let rdf_events = self.decode_events(events)?;

        // 2. Convert RDFEvent → Quad
        let quads = self.rdf_events_to_quads(&rdf_events)?;

        // 3. Build QuadContainer
        let container = self.build_quad_container(quads, events)?;

        // 4. Execute SPARQL with structured bindings
        let result = self
            .sparql_engine
            .execute_query_bindings(sparql_query, &container)
            .map_err(|e| JanusApiError::ExecutionError(format!("SPARQL execution failed: {}", e)));

        result
    }

    /// Decodes internal Event structs to RDFEvent using the Dictionary.
    ///
    /// # Arguments
    ///
    /// * `events` - Slice of Event structs with dictionary-encoded IDs
    ///
    /// # Returns
    ///
    /// Vector of RDFEvent with full URI strings
    ///
    /// # Errors
    ///
    /// Returns error if dictionary decoding fails for any event
    fn decode_events(&self, events: &[Event]) -> Result<Vec<RDFEvent>, JanusApiError> {
        let dictionary = self.storage.get_dictionary().read().map_err(|e| {
            JanusApiError::StorageError(format!("Failed to acquire dictionary lock: {}", e))
        })?;

        let mut rdf_events = Vec::with_capacity(events.len());

        for event in events {
            if dictionary.decode(event.subject).is_none() {
                return Err(JanusApiError::ExecutionError(format!(
                    "Failed to decode subject ID: {}",
                    event.subject
                )));
            }
            if dictionary.decode(event.predicate).is_none() {
                return Err(JanusApiError::ExecutionError(format!(
                    "Failed to decode predicate ID: {}",
                    event.predicate
                )));
            }
            if dictionary.decode(event.object).is_none() {
                return Err(JanusApiError::ExecutionError(format!(
                    "Failed to decode object ID: {}",
                    event.object
                )));
            }
            if dictionary.decode(event.graph).is_none() {
                return Err(JanusApiError::ExecutionError(format!(
                    "Failed to decode graph ID: {}",
                    event.graph
                )));
            }

            rdf_events.push(event.decode(&dictionary));
        }

        Ok(rdf_events)
    }

    /// Converts RDFEvent structs to Oxigraph Quad format.
    ///
    /// # Arguments
    ///
    /// * `rdf_events` - Slice of RDFEvent with URI strings
    ///
    /// # Returns
    ///
    /// Vector of Quad structs ready for SPARQL execution
    ///
    /// # Errors
    ///
    /// Returns error if any URI is invalid or conversion fails
    fn rdf_events_to_quads(&self, rdf_events: &[RDFEvent]) -> Result<Vec<Quad>, JanusApiError> {
        let mut quads = Vec::with_capacity(rdf_events.len());

        for rdf_event in rdf_events {
            let quad = self.rdf_event_to_quad(rdf_event)?;
            quads.push(quad);
        }

        Ok(quads)
    }

    fn rdf_events_to_quads_for_window_graph(
        &self,
        rdf_events: &[RDFEvent],
        window_graph: &str,
    ) -> Result<Vec<Quad>, JanusApiError> {
        let mut quads = Vec::with_capacity(rdf_events.len());
        let graph_node = NamedNode::new(window_graph).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Invalid synthetic historical window graph '{}': {}",
                window_graph, e
            ))
        })?;

        for rdf_event in rdf_events {
            let mut quad = self.rdf_event_to_quad(rdf_event)?;
            quad.graph_name = GraphName::NamedNode(graph_node.clone());
            quads.push(quad);
        }

        Ok(quads)
    }

    /// Converts a single RDFEvent to an Oxigraph Quad.
    ///
    /// # URI Handling
    ///
    /// - Subject: Must be a valid URI (NamedNode)
    /// - Predicate: Must be a valid URI (NamedNode)
    /// - Object: Can be URI (NamedNode) or literal value (Literal)
    /// - Graph: Can be URI (NamedNode) or "default" (DefaultGraph)
    ///
    /// # Arguments
    ///
    /// * `event` - RDFEvent with string URIs
    ///
    /// # Returns
    ///
    /// Oxigraph Quad ready for SPARQL processing
    fn rdf_event_to_quad(&self, event: &RDFEvent) -> Result<Quad, JanusApiError> {
        rdf_event_to_quad(event).map_err(JanusApiError::ExecutionError)
    }

    /// Builds a QuadContainer for SPARQL execution.
    ///
    /// # Arguments
    ///
    /// * `quads` - Vector of Quad structs
    /// * `events` - Original events (used for timestamp metadata)
    ///
    /// # Returns
    ///
    /// QuadContainer with timestamp set to the latest event timestamp
    fn build_quad_container(
        &self,
        quads: Vec<Quad>,
        events: &[Event],
    ) -> Result<QuadContainer, JanusApiError> {
        // Find the maximum timestamp from events
        let max_timestamp = events.iter().map(|e| e.timestamp).max().unwrap_or(0);

        // Convert Vec<Quad> to HashSet<Quad>
        let quad_set: HashSet<Quad> = quads.into_iter().collect();

        // Create QuadContainer with the timestamp
        Ok(QuadContainer::new(quad_set, max_timestamp.try_into().unwrap_or(0)))
    }

    fn build_quad_container_from_quads(
        &self,
        quads: Vec<Quad>,
        max_timestamp: u64,
    ) -> Result<QuadContainer, JanusApiError> {
        let quad_set: HashSet<Quad> = quads.into_iter().collect();
        Ok(QuadContainer::new(quad_set, max_timestamp.try_into().unwrap_or(0)))
    }

    /// Extracts time range from window definition.
    ///
    /// # Arguments
    ///
    /// * `window` - Window definition with timing parameters
    ///
    /// # Returns
    ///
    /// Tuple of (start_timestamp, end_timestamp) in milliseconds
    ///
    /// # Errors
    ///
    /// Returns error if required timing fields are missing
    #[allow(dead_code)]
    pub fn extract_time_range(
        &self,
        window: &WindowDefinition,
    ) -> Result<(u64, u64), JanusApiError> {
        // For fixed windows: use explicit start/end
        if let (Some(start), Some(end)) = (window.start, window.end) {
            return Ok((start, end));
        }

        // For sliding windows: calculate from offset and width
        if let Some(offset) = window.offset {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| JanusApiError::ExecutionError(format!("System time error: {}", e)))?
                .as_millis() as u64;

            let start = now.saturating_sub(offset);
            let end = start + window.width;
            return Ok((start, end));
        }

        Err(JanusApiError::ExecutionError(
            "Window definition must have either (start, end) or (offset, width)".to_string(),
        ))
    }
}

/// Iterator for sliding windows that queries storage directly
struct SlidingWindowIterator<'a> {
    executor: &'a HistoricalExecutor,
    current_start: u64,
    evaluation_time: u64,
    width: u64,
    slide: u64,
    sparql_query: String,
}

impl<'a> Iterator for SlidingWindowIterator<'a> {
    type Item = Result<Vec<HashMap<String, String>>, JanusApiError>;

    fn next(&mut self) -> Option<Self::Item> {
        let window_start = self.current_start;
        let window_end = match window_start.checked_add(self.width) {
            Some(window_end) => window_end,
            None => return None,
        };

        if window_end > self.evaluation_time {
            return None;
        }

        // Query storage
        let events = match self.executor.storage.query(window_start, window_end) {
            Ok(events) => events,
            Err(e) => {
                return Some(Err(JanusApiError::StorageError(format!("Query failed: {}", e))))
            }
        };

        // Execute SPARQL
        let result = self.executor.execute_sparql_on_events(&events, &self.sparql_query);

        // Advance window
        self.current_start += self.slide;

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::Term;

    #[test]
    fn test_historical_executor_creation() {
        // This test verifies the executor can be created
        // Actual execution tests require full integration setup
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let _executor = HistoricalExecutor::new(storage, engine);
    }

    #[test]
    fn test_extract_time_range_fixed_window() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let window = WindowDefinition {
            window_name: "test_window".to_string(),
            source_kind: crate::parsing::janusql_parser::SourceKind::Stream,
            stream_name: "test_stream".to_string(),
            width: 1000,
            slide: 100,
            offset: None,
            start: Some(1000),
            end: Some(2000),
            window_type: crate::parsing::janusql_parser::WindowType::HistoricalFixed,
        };

        let result = executor.extract_time_range(&window);
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert_eq!(start, 1000);
        assert_eq!(end, 2000);
    }

    #[test]
    fn test_extract_time_range_sliding_window() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let window = WindowDefinition {
            window_name: "test_window".to_string(),
            source_kind: crate::parsing::janusql_parser::SourceKind::Stream,
            stream_name: "test_stream".to_string(),
            width: 1000,
            slide: 100,
            offset: Some(5000),
            start: None,
            end: None,
            window_type: crate::parsing::janusql_parser::WindowType::HistoricalSliding,
        };

        let result = executor.extract_time_range(&window);
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(start > 0);
        assert_eq!(end - start, 1000);
    }

    #[test]
    fn test_execute_sliding_windows_skips_future_crossing_windows() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let window = WindowDefinition {
            window_name: "test_window".to_string(),
            source_kind: crate::parsing::janusql_parser::SourceKind::Log,
            stream_name: "test_stream".to_string(),
            width: 100,
            slide: 50,
            offset: Some(250),
            start: None,
            end: None,
            window_type: crate::parsing::janusql_parser::WindowType::HistoricalSliding,
        };

        let results = executor
            .execute_sliding_windows(&window, "SELECT ?s WHERE { ?s ?p ?o }")
            .collect::<Vec<_>>();

        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|result| result.is_ok()));
    }

    #[test]
    fn test_execute_tumbling_historical_windows_stops_before_evaluation_time() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let window = WindowDefinition {
            window_name: "test_tumbling_window".to_string(),
            source_kind: crate::parsing::janusql_parser::SourceKind::Log,
            stream_name: "test_stream".to_string(),
            width: 100,
            slide: 100,
            offset: Some(250),
            start: None,
            end: None,
            window_type: crate::parsing::janusql_parser::WindowType::HistoricalSliding,
        };

        let results = executor
            .execute_sliding_windows(&window, "SELECT ?s WHERE { ?s ?p ?o }")
            .collect::<Vec<_>>();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.is_ok()));
    }

    #[test]
    fn test_rdf_event_to_quad_with_uri_object() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let event = RDFEvent::new(
            1000,
            "http://example.org/alice",
            "http://example.org/knows",
            "http://example.org/bob",
            "default",
        );

        let result = executor.rdf_event_to_quad(&event);
        assert!(result.is_ok());

        let quad = result.unwrap();
        assert_eq!(quad.subject.to_string(), "<http://example.org/alice>");
        assert_eq!(quad.predicate.to_string(), "<http://example.org/knows>");
    }

    #[test]
    fn test_rdf_event_to_quad_with_literal_object() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let event = RDFEvent::new(
            1000,
            "http://example.org/alice",
            "http://example.org/age",
            "30",
            "default",
        );

        let result = executor.rdf_event_to_quad(&event);
        assert!(result.is_ok());

        let quad = result.unwrap();
        assert_eq!(quad.subject.to_string(), "<http://example.org/alice>");
        assert_eq!(quad.predicate.to_string(), "<http://example.org/age>");
        // Object should be a literal
        if let Term::Literal(lit) = quad.object {
            assert_eq!(lit.value(), "30");
        } else {
            panic!("Expected literal object");
        }
    }

    #[test]
    fn test_rdf_event_to_quad_invalid_subject() {
        let storage = Arc::new(
            StreamingSegmentedStorage::new(crate::storage::util::StreamingConfig::default())
                .expect("Failed to create storage"),
        );
        let engine = OxigraphAdapter::new();
        let executor = HistoricalExecutor::new(storage, engine);

        let event =
            RDFEvent::new(1000, "not a valid uri", "http://example.org/pred", "value", "default");

        let result = executor.rdf_event_to_quad(&event);
        assert!(result.is_err());
    }
}
