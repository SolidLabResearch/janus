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
use crate::parsing::janusql_parser::WindowDefinition;
use crate::querying::oxigraph_adapter::OxigraphAdapter;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use crate::stream::operators::historical_fixed_window::HistoricalFixedWindowOperator;
use crate::stream::operators::historical_sliding_window::HistoricalSlidingWindowOperator;
use oxigraph::model::{GraphName, NamedNode, Quad, Term};
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
        // Query storage directly instead of using the operator
        let start = window.start.ok_or_else(|| {
            JanusApiError::ExecutionError("Fixed window requires start timestamp".to_string())
        })?;
        let end = window.end.ok_or_else(|| {
            JanusApiError::ExecutionError("Fixed window requires end timestamp".to_string())
        })?;

        // Query the storage for events in the fixed window
        let events = self
            .storage
            .query(start, end)
            .map_err(|e| JanusApiError::StorageError(format!("Failed to query storage: {}", e)))?;

        // Execute SPARQL on the events
        self.execute_sparql_on_events(&events, sparql_query)
    }

    /// Execute a sliding window query that returns an iterator of results (bypassing operator).
    ///
    /// Note: This is a simplified implementation that queries storage directly.
    /// For production use, consider implementing proper window sliding logic.
    #[allow(dead_code)]
    fn execute_fixed_window_with_operator(
        &self,
        window: &WindowDefinition,
        sparql_query: &str,
    ) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
        // Original operator-based implementation kept for reference
        // Note: Requires Arc->Rc conversion which is currently problematic
        unimplemented!("Operator-based execution requires refactoring window operators to use Arc")
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
        // Calculate sliding windows and query storage directly
        let offset = window.offset.unwrap_or(0);
        let width = window.width;
        let slide = window.slide;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let start_time = now.saturating_sub(offset);
        let end_bound = now;

        // Create an iterator that generates windows
        SlidingWindowIterator {
            executor: self,
            current_start: start_time,
            end_bound,
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
            // Decode each field individually
            let subject = dictionary
                .decode(event.subject)
                .ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Failed to decode subject ID: {}",
                        event.subject
                    ))
                })?
                .to_string();

            let predicate = dictionary
                .decode(event.predicate)
                .ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Failed to decode predicate ID: {}",
                        event.predicate
                    ))
                })?
                .to_string();

            let object = dictionary
                .decode(event.object)
                .ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Failed to decode object ID: {}",
                        event.object
                    ))
                })?
                .to_string();

            let graph = dictionary
                .decode(event.graph)
                .ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Failed to decode graph ID: {}",
                        event.graph
                    ))
                })?
                .to_string();

            let rdf_event = RDFEvent::new(event.timestamp, &subject, &predicate, &object, &graph);
            rdf_events.push(rdf_event);
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
        // Parse subject as NamedNode
        let subject = NamedNode::new(&event.subject).map_err(|e| {
            JanusApiError::ExecutionError(format!("Invalid subject URI '{}': {}", event.subject, e))
        })?;

        // Parse predicate as NamedNode
        let predicate = NamedNode::new(&event.predicate).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Invalid predicate URI '{}': {}",
                event.predicate, e
            ))
        })?;

        // Parse object - can be URI or literal
        let object = if event.object.starts_with("http://") || event.object.starts_with("https://")
        {
            // Object is a URI
            let object_node = NamedNode::new(&event.object).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Invalid object URI '{}': {}",
                    event.object, e
                ))
            })?;
            Term::NamedNode(object_node)
        } else {
            // Object is a literal value - check if it's numeric for SPARQL aggregations
            let literal = if let Ok(_) = event.object.parse::<f64>() {
                // It's a decimal number - create typed literal for SPARQL aggregations
                oxigraph::model::Literal::new_typed_literal(
                    &event.object,
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
                )
            } else if let Ok(_) = event.object.parse::<i64>() {
                // It's an integer
                oxigraph::model::Literal::new_typed_literal(
                    &event.object,
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
                )
            } else {
                // Plain string literal
                oxigraph::model::Literal::new_simple_literal(&event.object)
            };
            Term::Literal(literal)
        };

        // Parse graph - default or named
        let graph = if event.graph.is_empty() || event.graph == "default" {
            GraphName::DefaultGraph
        } else {
            let graph_node = NamedNode::new(&event.graph).map_err(|e| {
                JanusApiError::ExecutionError(format!("Invalid graph URI '{}': {}", event.graph, e))
            })?;
            GraphName::NamedNode(graph_node)
        };

        Ok(Quad::new(subject, predicate, object, graph))
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
    end_bound: u64,
    width: u64,
    slide: u64,
    sparql_query: String,
}

impl<'a> Iterator for SlidingWindowIterator<'a> {
    type Item = Result<Vec<HashMap<String, String>>, JanusApiError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_start > self.end_bound {
            return None;
        }

        let window_start = self.current_start;
        let window_end = (window_start + self.width).min(self.end_bound);

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
