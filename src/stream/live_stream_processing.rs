//! Live Stream Processing Module
//!
//! This module provides real-time RDF stream processing using the rsp-rs engine.
//! It integrates RSP-QL query execution with Janus's RDFEvent data model.

use crate::core::RDFEvent;
use oxigraph::model::{GraphName, NamedNode, Quad, Term};
use rsp_rs::{BindingWithTimestamp, RDFStream, RSPEngine};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvError};

/// Live stream processing engine for RSP-QL queries
pub struct LiveStreamProcessing {
    /// RSP-RS engine instance
    engine: RSPEngine,
    /// Map of stream URIs to stream instances (cloneable in 0.3.1)
    streams: HashMap<String, RDFStream>,
    /// Result receiver for query results
    result_receiver: Option<Receiver<BindingWithTimestamp>>,
    /// Flag indicating if processing has started
    processing_started: bool,
}

/// Error type for live stream processing operations
#[derive(Debug)]
pub struct LiveStreamProcessingError(String);

impl std::fmt::Display for LiveStreamProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LiveStreamProcessingError: {}", self.0)
    }
}

impl std::error::Error for LiveStreamProcessingError {}

impl From<String> for LiveStreamProcessingError {
    fn from(err: String) -> Self {
        LiveStreamProcessingError(err)
    }
}

impl From<Box<dyn std::error::Error>> for LiveStreamProcessingError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        LiveStreamProcessingError(err.to_string())
    }
}

impl LiveStreamProcessing {
    /// Creates a new LiveStreamProcessing instance with the given RSP-QL query
    ///
    /// # Arguments
    ///
    /// * `rspql_query` - RSP-QL query string defining the continuous query
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use janus::stream::live_stream_processing::LiveStreamProcessing;
    ///
    /// let query = r#"
    ///     PREFIX ex: <http://example.org/>
    ///     REGISTER RStream <output> AS
    ///     SELECT *
    ///     FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 10000 STEP 2000]
    ///     WHERE {
    ///         WINDOW ex:w1 { ?s ?p ?o }
    ///     }
    /// "#;
    ///
    /// let processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    /// ```
    pub fn new(rspql_query: String) -> Result<Self, LiveStreamProcessingError> {
        println!("=== LiveStreamProcessing: Creating RSPEngine with RSP-QL ===");
        println!("{}", rspql_query);
        println!("=== END RSP-QL ===");

        let mut engine = RSPEngine::new(rspql_query);

        // Initialize the engine to create windows and streams
        engine.initialize().map_err(|e| {
            LiveStreamProcessingError(format!("Failed to initialize RSP engine: {}", e))
        })?;

        Ok(Self {
            engine,
            streams: HashMap::new(),
            result_receiver: None,
            processing_started: false,
        })
    }

    /// Registers a stream by its URI and stores a clone of it
    ///
    /// # Arguments
    ///
    /// * `stream_uri` - URI of the stream to register (e.g., "http://example.org/stream1")
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the stream is successfully registered, or an error if the stream
    /// doesn't exist in the query.
    pub fn register_stream(&mut self, stream_uri: &str) -> Result<(), LiveStreamProcessingError> {
        if self.streams.contains_key(stream_uri) {
            return Ok(()); // Already registered
        }

        // In rsp-rs 0.3.1, get_stream returns Option<RDFStream> (cloneable)
        let stream = self.engine.get_stream(stream_uri).ok_or_else(|| {
            LiveStreamProcessingError(format!("Stream '{}' not found in query", stream_uri))
        })?;

        self.streams.insert(stream_uri.to_string(), stream);
        Ok(())
    }

    /// Starts the processing engine and begins receiving results
    ///
    /// This must be called before adding events to streams to receive query results.
    pub fn start_processing(&mut self) -> Result<(), LiveStreamProcessingError> {
        if self.processing_started {
            return Err(LiveStreamProcessingError("Processing already started".to_string()));
        }

        let receiver = self.engine.start_processing();
        self.result_receiver = Some(receiver);
        self.processing_started = true;

        Ok(())
    }

    /// Adds an RDF event to a specific stream
    ///
    /// # Arguments
    ///
    /// * `stream_uri` - URI of the stream to add the event to
    /// * `event` - RDFEvent to add to the stream
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use janus::core::RDFEvent;
    /// use janus::stream::live_stream_processing::LiveStreamProcessing;
    ///
    /// # let mut processor = LiveStreamProcessing::new("".to_string()).unwrap();
    /// let event = RDFEvent::new(
    ///     1000,
    ///     "http://example.org/alice",
    ///     "http://example.org/knows",
    ///     "http://example.org/bob",
    ///     "http://example.org/graph1"
    /// );
    ///
    /// processor.add_event("http://example.org/stream1", event).unwrap();
    /// ```
    pub fn add_event(
        &self,
        stream_uri: &str,
        event: RDFEvent,
    ) -> Result<(), LiveStreamProcessingError> {
        let stream = self.streams.get(stream_uri).ok_or_else(|| {
            LiveStreamProcessingError(format!(
                "Stream '{}' not registered. Call register_stream() first.",
                stream_uri
            ))
        })?;

        let quad = self.rdf_event_to_quad(&event)?;

        stream
            .add_quads(
                vec![quad],
                event.timestamp.try_into().map_err(|_| {
                    LiveStreamProcessingError("Timestamp too large for i64".to_string())
                })?,
            )
            .map_err(|e| LiveStreamProcessingError(format!("Failed to add quad: {}", e)))?;

        // Results are consumed by external workers via receive_result()/try_receive_result().
        // Avoid draining the channel during event ingestion to ensure downstream consumers
        // observe every live binding.

        Ok(())
    }

    /// Adds multiple RDF events to a specific stream in batch
    ///
    /// # Arguments
    ///
    /// * `stream_uri` - URI of the stream to add events to
    /// * `events` - Vector of RDFEvents to add to the stream
    ///
    /// # Note
    ///
    /// All events in the batch use the timestamp from the first event.
    /// For different timestamps, call `add_event()` individually.
    pub fn add_events(
        &self,
        stream_uri: &str,
        events: Vec<RDFEvent>,
    ) -> Result<(), LiveStreamProcessingError> {
        if events.is_empty() {
            return Ok(());
        }

        let stream = self.streams.get(stream_uri).ok_or_else(|| {
            LiveStreamProcessingError(format!(
                "Stream '{}' not registered. Call register_stream() first.",
                stream_uri
            ))
        })?;

        let timestamp: i64 = events[0]
            .timestamp
            .try_into()
            .map_err(|_| LiveStreamProcessingError("Timestamp too large for i64".to_string()))?;
        let quads: Result<Vec<Quad>, LiveStreamProcessingError> =
            events.iter().map(|e| self.rdf_event_to_quad(e)).collect();

        stream
            .add_quads(quads?, timestamp)
            .map_err(|e| LiveStreamProcessingError(format!("Failed to add quads: {}", e)))?;

        Ok(())
    }

    /// Closes a stream and triggers final window closures
    ///
    /// This is a convenience method that adds a sentinel event with a high timestamp
    /// to force all remaining windows to close and emit their results.
    ///
    /// # Arguments
    ///
    /// * `stream_uri` - URI of the stream to close
    /// * `final_timestamp` - Timestamp for the sentinel event (should be after all data)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use janus::stream::live_stream_processing::LiveStreamProcessing;
    ///
    /// # let mut processor = LiveStreamProcessing::new("".to_string()).unwrap();
    /// // After adding all events...
    /// processor.close_stream("http://example.org/stream1", 100000).unwrap();
    /// ```
    pub fn close_stream(
        &self,
        stream_uri: &str,
        final_timestamp: i64,
    ) -> Result<(), LiveStreamProcessingError> {
        let sentinel_event = RDFEvent::new(
            final_timestamp.try_into().map_err(|_| {
                LiveStreamProcessingError("Timestamp cannot be negative".to_string())
            })?,
            "urn:rsp:sentinel:subject",
            "urn:rsp:sentinel:predicate",
            "urn:rsp:sentinel:object",
            "",
        );

        self.add_event(stream_uri, sentinel_event)
    }

    /// Adds static background knowledge to the RSP engine
    ///
    /// Static data is available for joins with streaming data in RSP-QL queries.
    ///
    /// # Arguments
    ///
    /// * `event` - RDFEvent representing static knowledge
    pub fn add_static_data(&mut self, event: RDFEvent) -> Result<(), LiveStreamProcessingError> {
        let quad = self.rdf_event_to_quad(&event)?;
        self.engine.add_static_data(quad);
        Ok(())
    }

    /// Receives the next query result from the processing engine
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(result))` if a result is available,
    /// `Ok(None)` if the channel is disconnected,
    /// or an error if processing hasn't started.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use janus::stream::live_stream_processing::LiveStreamProcessing;
    ///
    /// # let mut processor = LiveStreamProcessing::new("".to_string()).unwrap();
    /// processor.start_processing().unwrap();
    ///
    /// // Process results
    /// while let Ok(Some(result)) = processor.receive_result() {
    ///     println!("Result bindings: {}", result.bindings);
    ///     println!("Timestamp: {} to {}", result.timestamp_from, result.timestamp_to);
    /// }
    /// ```
    pub fn receive_result(
        &self,
    ) -> Result<Option<BindingWithTimestamp>, LiveStreamProcessingError> {
        let receiver = self.result_receiver.as_ref().ok_or_else(|| {
            LiveStreamProcessingError(
                "Processing not started. Call start_processing() first.".to_string(),
            )
        })?;

        match receiver.recv() {
            Ok(result) => Ok(Some(result)),
            Err(RecvError) => Ok(None), // Channel disconnected
        }
    }

    /// Attempts to receive a result without blocking
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(result))` if a result is immediately available,
    /// `Ok(None)` if no result is available or channel is disconnected.
    pub fn try_receive_result(
        &self,
    ) -> Result<Option<BindingWithTimestamp>, LiveStreamProcessingError> {
        let receiver = self.result_receiver.as_ref().ok_or_else(|| {
            LiveStreamProcessingError(
                "Processing not started. Call start_processing() first.".to_string(),
            )
        })?;

        match receiver.try_recv() {
            Ok(result) => {
                println!(
                    "LiveStreamProcessing.try_receive_result(): Returning result, bindings: {}",
                    result.bindings
                );
                Ok(Some(result))
            }
            Err(_) => Ok(None), // Either empty or disconnected
        }
    }

    /// Collects all available results into a vector
    ///
    /// This is a blocking operation that will collect results until the channel is empty.
    ///
    /// # Arguments
    ///
    /// * `max_results` - Optional maximum number of results to collect
    ///
    /// # Returns
    ///
    /// Vector of all collected results
    pub fn collect_results(
        &self,
        max_results: Option<usize>,
    ) -> Result<Vec<BindingWithTimestamp>, LiveStreamProcessingError> {
        let mut results = Vec::new();
        let limit = max_results.unwrap_or(usize::MAX);

        while results.len() < limit {
            match self.try_receive_result()? {
                Some(result) => results.push(result),
                None => break,
            }
        }

        Ok(results)
    }

    /// Converts an RDFEvent to an oxigraph Quad
    ///
    /// # Arguments
    ///
    /// * `event` - RDFEvent to convert
    ///
    /// # Returns
    ///
    /// Returns the corresponding oxigraph Quad
    fn rdf_event_to_quad(&self, event: &RDFEvent) -> Result<Quad, LiveStreamProcessingError> {
        // Parse subject as NamedNode
        let subject = NamedNode::new(&event.subject)
            .map_err(|e| LiveStreamProcessingError(format!("Invalid subject URI: {}", e)))?;

        // Parse predicate as NamedNode
        let predicate = NamedNode::new(&event.predicate)
            .map_err(|e| LiveStreamProcessingError(format!("Invalid predicate URI: {}", e)))?;

        // Parse object - can be NamedNode or Literal
        // For simplicity, treat as NamedNode first, fall back to literal if needed
        let object = if event.object.starts_with("http://") || event.object.starts_with("https://")
        {
            // Try as NamedNode
            match NamedNode::new(&event.object) {
                Ok(node) => Term::NamedNode(node),
                Err(_) => {
                    Term::Literal(oxigraph::model::Literal::new_simple_literal(&event.object))
                }
            }
        } else {
            // Treat as literal - check if it's a numeric value
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

        // Parse graph - use default graph if empty
        // NOTE: In rsp-rs 0.3.1+, the window automatically assigns quads to the window's graph
        // so we can use DefaultGraph here and it will be rewritten by the window
        let graph = if event.graph.is_empty() || event.graph == "default" {
            GraphName::DefaultGraph
        } else {
            GraphName::NamedNode(
                NamedNode::new(&event.graph)
                    .map_err(|e| LiveStreamProcessingError(format!("Invalid graph URI: {}", e)))?,
            )
        };

        Ok(Quad::new(subject, predicate, object, graph))
    }

    /// Returns the list of registered stream URIs
    pub fn get_registered_streams(&self) -> Vec<String> {
        self.streams.keys().cloned().collect()
    }

    /// Checks if processing has been started
    pub fn is_processing(&self) -> bool {
        self.processing_started
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_processor() {
        let query = r"
            PREFIX ex: <http://example.org/>
            REGISTER RStream <output> AS
            SELECT *
            FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 10000 STEP 2000]
            WHERE {
                WINDOW ex:w1 { ?s ?p ?o }
            }
        ";

        let result = LiveStreamProcessing::new(query.to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_stream() {
        let query = r"
            PREFIX ex: <http://example.org/>
            REGISTER RStream <output> AS
            SELECT *
            FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 10000 STEP 2000]
            WHERE {
                WINDOW ex:w1 { ?s ?p ?o }
            }
        ";

        let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
        let result = processor.register_stream("http://example.org/stream1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_rdf_event_to_quad() {
        let query = r"
            PREFIX ex: <http://example.org/>
            REGISTER RStream <output> AS
            SELECT *
            FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 10000 STEP 2000]
            WHERE {
                WINDOW ex:w1 { ?s ?p ?o }
            }
        ";

        let processor = LiveStreamProcessing::new(query.to_string()).unwrap();

        let event = RDFEvent::new(
            1000,
            "http://example.org/alice",
            "http://example.org/knows",
            "http://example.org/bob",
            "http://example.org/graph1",
        );

        let result = processor.rdf_event_to_quad(&event);
        assert!(result.is_ok());
    }

    #[test]
    fn test_processing_state() {
        let query = r"
            PREFIX ex: <http://example.org/>
            REGISTER RStream <output> AS
            SELECT *
            FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 10000 STEP 2000]
            WHERE {
                WINDOW ex:w1 { ?s ?p ?o }
            }
        ";

        let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
        assert!(!processor.is_processing());

        processor.start_processing().unwrap();
        assert!(processor.is_processing());
    }
}
