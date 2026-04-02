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

use crate::anomaly::query_options::build_evaluator;
use crate::anomaly::vocab;
use crate::api::janus_api::JanusApiError;
use crate::core::{Event, RDFEvent};
use crate::parsing::janusql_parser::WindowDefinition;
use crate::querying::oxigraph_adapter::OxigraphAdapter;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use crate::stream::operators::historical_fixed_window::HistoricalFixedWindowOperator;
use crate::stream::operators::historical_sliding_window::HistoricalSlidingWindowOperator;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use rsp_rs::QuadContainer;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Error type for the two-pass materialisation step
// ---------------------------------------------------------------------------

/// Errors that can arise during Pass 1 stat materialisation.
#[derive(Debug)]
pub(crate) enum HistoricalExecutorError {
    /// The SPARQL query used in Pass 1 failed to parse.
    SparqlParseError(String),
    /// The SPARQL query used in Pass 1 failed at evaluation time.
    SparqlExecutionError(String),
    /// A triple could not be inserted into the Oxigraph store.
    StoreError(String),
    /// No numeric observations matching `value_predicate` were found in the
    /// window.  The window may be empty, or the wrong predicate was supplied.
    NoObservations,
}

impl fmt::Display for HistoricalExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SparqlParseError(e) => write!(f, "Pass-1 SPARQL parse error: {}", e),
            Self::SparqlExecutionError(e) => write!(f, "Pass-1 SPARQL execution error: {}", e),
            Self::StoreError(e) => write!(f, "Pass-1 store error: {}", e),
            Self::NoObservations => write!(
                f,
                "materialise_stats: no numeric literals found in window — window may be empty"
            ),
        }
    }
}

impl std::error::Error for HistoricalExecutorError {}

/// Executor for historical SPARQL queries over stored RDF data.
///
/// # Two-pass execution (always on)
///
/// Every window invocation runs two SPARQL passes against the same in-memory
/// Oxigraph store:
///
/// - **Pass 1** (`materialise_stats`): scans all numeric literals in the window
///   using `FILTER(isNumeric(?value))`, groups them by sensor subject, and
///   inserts `janus:histMean` / `janus:histStdDev` triples back into the store.
///   No predicate needs to be specified — any numeric observation value is picked
///   up automatically.  If the window is empty, Pass 1 is a no-op.
/// - **Pass 2**: runs the user's Janus-QL SPARQL query against the enriched
///   store, where `?sensor janus:histMean ?mean` is now a matchable fact.
///
/// # Example
///
/// ```ignore
/// let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
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
    /// Creates a new `HistoricalExecutor`.
    ///
    /// Two-pass stat materialisation is always active — no additional
    /// configuration is required.
    ///
    /// # Arguments
    ///
    /// * `storage` - Shared reference to the segmented storage backend
    /// * `sparql_engine` - SPARQL query engine (OxigraphAdapter, kept for
    ///   API compatibility — Pass 2 uses `build_evaluator()` directly)
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
    /// 3. Load quads into a fresh Oxigraph `Store`
    /// 4. **Pass 1**: `materialise_stats` — scan all numeric literals in the
    ///    store with `FILTER(isNumeric(?value))` and insert `janus:histMean` /
    ///    `janus:histStdDev` triples.  If the window is empty, this is a no-op.
    /// 5. **Pass 2**: run the user's SPARQL query against the enriched store.
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

        // 3. Build a shared store for both passes.
        let store = Store::new().map_err(|e| {
            JanusApiError::ExecutionError(format!("Oxigraph store creation failed: {}", e))
        })?;

        for quad in &quads {
            store.insert(quad).map_err(|e| {
                JanusApiError::StorageError(format!("Quad insertion failed: {}", e))
            })?;
        }

        // 4. Pass 1 — materialise stats.  NoObservations (empty window) is not
        //    an error — Pass 2 will simply return no results for stat-pattern
        //    queries, which is the correct behaviour for an empty window.
        match materialise_stats(&store) {
            Ok(()) | Err(HistoricalExecutorError::NoObservations) => {}
            Err(e) => {
                return Err(JanusApiError::ExecutionError(format!(
                    "Two-pass materialisation failed: {}",
                    e
                )));
            }
        }

        // 5. Pass 2 — run the user's query against the enriched store.
        run_user_query_on_store(&store, sparql_query)
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

// ---------------------------------------------------------------------------
// Two-pass helper functions (module-level, not tied to HistoricalExecutor)
// ---------------------------------------------------------------------------

/// Pass 1 of the two-pass executor: compute per-sensor aggregate statistics
/// from raw observation triples and materialise them back into the store.
///
/// # What it does
///
/// Runs the following SPARQL query against `store`:
///
/// ```sparql
/// SELECT DISTINCT ?sensor ?pred ?value WHERE {
///     { ?sensor ?pred ?value . FILTER(isNumeric(?value)) }
///     UNION
///     { GRAPH ?g { ?sensor ?pred ?value . FILTER(isNumeric(?value)) } }
/// }
/// ```
///
/// No value predicate needs to be specified — any numeric literal in the window
/// is picked up automatically.  The UNION covers both the default graph and any
/// named graphs the window data may have been loaded into.
///
/// For each sensor subject, computes:
/// - **mean** = arithmetic mean of all distinct observed numeric values
/// - **std_dev** = population standard deviation (σ, not sample s)
///
/// And inserts into the **default graph**:
/// ```text
/// <sensor> janus:histMean    "…"^^xsd:decimal .
/// <sensor> janus:histStdDev  "…"^^xsd:decimal .
/// ```
///
/// # Note on multiple numeric predicates
///
/// If a sensor has more than one numeric predicate (e.g. both `ex:temperature`
/// and `ex:humidity`), both sets of values are pooled together before computing
/// the mean and σ.  For the common case of one numeric predicate per sensor this
/// is not an issue.
///
/// # Edge cases
///
/// - **Empty window**: returns `HistoricalExecutorError::NoObservations`.
///   The caller (`execute_sparql_on_events`) treats this as a no-op so that
///   Pass 2 still runs and returns empty results rather than an error.
/// - **Single observation per sensor**: std_dev is `0` by definition.
/// - **σ = 0 for repeated identical values**: RDF dataset semantics deduplicate
///   identical `(s, p, o)` triples within the same graph, so repeated readings
///   of the same value collapse to one triple.  σ = 0 is still correct.
///
/// # Slope materialisation
///
/// `janus:histSlope` / `janus:liveSlope` are defined in [`crate::anomaly::vocab`]
/// but are **not yet materialised** here.  To add slope:
/// 1. Extend this query to also bind a timestamp variable (e.g. via `dct:date`).
/// 2. Perform OLS linear regression over `(timestamp, value)` pairs in Rust.
/// 3. Insert `<sensor> janus:histSlope "slope"^^xsd:decimal .` alongside mean/σ.
fn materialise_stats(store: &Store) -> Result<(), HistoricalExecutorError> {
    // isNumeric() matches xsd:decimal, xsd:integer, xsd:float, xsd:double and
    // their subtypes — exactly the types rdf_event_to_quad produces for numeric
    // object values.  No predicate needs to be known ahead of time.
    let query = "SELECT DISTINCT ?sensor ?value WHERE { \
                     { ?sensor ?pred ?value . FILTER(isNumeric(?value)) } \
                     UNION \
                     { GRAPH ?g { ?sensor ?pred ?value . FILTER(isNumeric(?value)) } } \
                 }";

    let evaluator = build_evaluator();
    let parsed = evaluator
        .parse_query(query)
        .map_err(|e| HistoricalExecutorError::SparqlParseError(e.to_string()))?;
    let results = parsed
        .on_store(store)
        .execute()
        .map_err(|e| HistoricalExecutorError::SparqlExecutionError(e.to_string()))?;

    // Collect all (sensor_iri → [values]) from the solution set.
    let mut sensor_values: HashMap<String, Vec<f64>> = HashMap::new();

    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            let solution = solution
                .map_err(|e| HistoricalExecutorError::SparqlExecutionError(e.to_string()))?;

            let sensor_term = solution.get("sensor").cloned();
            let value_term = solution.get("value").cloned();

            if let (Some(Term::NamedNode(sensor_node)), Some(Term::Literal(value_lit))) =
                (sensor_term, value_term)
            {
                if let Ok(v) = value_lit.value().parse::<f64>() {
                    sensor_values
                        .entry(sensor_node.as_str().to_string())
                        .or_default()
                        .push(v);
                }
                // Non-parseable literals (strings, booleans) are silently skipped.
            }
        }
    }

    if sensor_values.is_empty() {
        return Err(HistoricalExecutorError::NoObservations);
    }

    // Discover all named graphs in the store so we can insert stats into each.
    // The JanusQL parser wraps WINDOW block content in GRAPH <streamUri> { … },
    // so stats inserted only into DefaultGraph would be invisible to patterns
    // generated by that wrapper.  Inserting into every named graph ensures
    // `?sensor janus:histMean ?mean` works both in and outside GRAPH clauses.
    let graphs_evaluator = build_evaluator();
    let graphs_parsed = graphs_evaluator
        .parse_query("SELECT DISTINCT ?g WHERE { GRAPH ?g { ?s ?p ?o } }")
        .map_err(|e| HistoricalExecutorError::SparqlParseError(e.to_string()))?;
    let graphs_results = graphs_parsed
        .on_store(store)
        .execute()
        .map_err(|e| HistoricalExecutorError::SparqlExecutionError(e.to_string()))?;

    let mut named_graphs: Vec<NamedNode> = Vec::new();
    if let QueryResults::Solutions(solutions) = graphs_results {
        for sol in solutions {
            let sol = sol
                .map_err(|e| HistoricalExecutorError::SparqlExecutionError(e.to_string()))?;
            if let Some(Term::NamedNode(g)) = sol.get("g").cloned() {
                named_graphs.push(g);
            }
        }
    }

    let xsd_decimal =
        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();

    for (sensor_iri, values) in &sensor_values {
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        // Population standard deviation: σ = sqrt(Σ(xi − μ)² / n)
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        let sensor_node = NamedNode::new(sensor_iri).map_err(|e| {
            HistoricalExecutorError::StoreError(format!(
                "Sensor IRI '{}' is not a valid NamedNode: {}",
                sensor_iri, e
            ))
        })?;

        let mean_literal =
            Literal::new_typed_literal(&mean.to_string(), xsd_decimal.clone());
        let sigma_literal =
            Literal::new_typed_literal(&std_dev.to_string(), xsd_decimal.clone());

        // Insert into DefaultGraph and every named graph so stats are visible
        // inside both `GRAPH <streamUri> { }` wrappers and bare triple patterns.
        let target_graphs: Vec<GraphName> = std::iter::once(GraphName::DefaultGraph)
            .chain(named_graphs.iter().cloned().map(GraphName::NamedNode))
            .collect();

        for target_graph in target_graphs {
            store
                .insert(&Quad::new(
                    sensor_node.clone(),
                    vocab::hist_mean(),
                    mean_literal.clone(),
                    target_graph.clone(),
                ))
                .map_err(|e| HistoricalExecutorError::StoreError(e.to_string()))?;

            store
                .insert(&Quad::new(
                    sensor_node.clone(),
                    vocab::hist_std_dev(),
                    sigma_literal.clone(),
                    target_graph,
                ))
                .map_err(|e| HistoricalExecutorError::StoreError(e.to_string()))?;
        }

        // NOTE: janus:histSlope and janus:liveSlope materialisation is deferred.
        // See the doc comment on this function for implementation steps.
    }

    Ok(())
}

/// Pass 2 of the two-pass executor: run the user's SPARQL query against the
/// store that has already been enriched by [`materialise_stats`].
fn run_user_query_on_store(
    store: &Store,
    sparql_query: &str,
) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
    let evaluator = build_evaluator();
    let parsed = evaluator
        .parse_query(sparql_query)
        .map_err(|e| JanusApiError::ExecutionError(format!("SPARQL parse error: {}", e)))?;
    let results = parsed
        .on_store(store)
        .execute()
        .map_err(|e| JanusApiError::ExecutionError(format!("SPARQL execution error: {}", e)))?;

    let mut bindings_list = Vec::new();

    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            let solution = solution
                .map_err(|e| JanusApiError::ExecutionError(format!("Solution error: {}", e)))?;
            let mut binding = HashMap::new();
            for (var, term) in solution.iter() {
                binding.insert(var.as_str().to_string(), term.to_string());
            }
            bindings_list.push(binding);
        }
    }

    Ok(bindings_list)
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
    use crate::anomaly::vocab;
    use oxigraph::sparql::QueryResults;

    // -----------------------------------------------------------------------
    // Helpers shared by two-pass tests
    // -----------------------------------------------------------------------

    fn xsd_decimal() -> NamedNode {
        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap()
    }

    fn insert_decimal(store: &Store, s: &str, p: &str, v: &str) {
        store
            .insert(&Quad::new(
                NamedNode::new(s).unwrap(),
                NamedNode::new(p).unwrap(),
                Literal::new_typed_literal(v, xsd_decimal()),
                GraphName::DefaultGraph,
            ))
            .unwrap();
    }

    /// Query `store` for a single literal bound to `?val` and return it as f64.
    fn query_f64(store: &Store, sparql: &str) -> f64 {
        let evaluator = build_evaluator();
        let parsed = evaluator.parse_query(sparql).expect("query must parse");
        let results = parsed.on_store(store).execute().expect("query must execute");
        if let QueryResults::Solutions(mut sols) = results {
            let sol = sols.next().expect("expected at least one row").unwrap();
            if let Term::Literal(lit) = sol.get("val").expect("?val must be bound") {
                return lit.value().parse::<f64>().expect("literal must be numeric");
            }
        }
        panic!("query_f64: could not extract a numeric ?val");
    }

    // -----------------------------------------------------------------------
    // Step 4, Test 1: materialise_stats inserts correct triples
    // -----------------------------------------------------------------------

    /// Insert three distinct observations for each of two sensors, run
    /// materialise_stats, then assert the inserted mean and std_dev triples
    /// are numerically correct.
    ///
    /// sensor1 values: [1.0, 2.0, 3.0] → mean = 2.0, σ = √(2/3) ≈ 0.8165
    /// sensor2 values: [10.0, 11.0, 12.0] → mean = 11.0, σ ≈ 0.8165
    #[test]
    fn test_materialise_stats_inserts_hist_mean_and_std_dev() {
        let store = Store::new().unwrap();
        let pred = "http://test.org/val";
        let s1 = "http://test.org/sensor1";
        let s2 = "http://test.org/sensor2";

        for v in ["1.0", "2.0", "3.0"] {
            insert_decimal(&store, s1, pred, v);
        }
        for v in ["10.0", "11.0", "12.0"] {
            insert_decimal(&store, s2, pred, v);
        }

        materialise_stats(&store)
            .expect("materialise_stats must succeed for non-empty store");

        // --- sensor1 mean ---
        let mean1 = query_f64(
            &store,
            &format!("SELECT ?val WHERE {{ <{}> <{}> ?val . }}", s1, vocab::HIST_MEAN_IRI),
        );
        assert!(
            (mean1 - 2.0).abs() < 1e-9,
            "sensor1 histMean should be 2.0, got {}",
            mean1
        );

        // --- sensor1 std_dev ---
        let expected_sigma = ((2.0_f64 / 3.0_f64) as f64).sqrt();
        let sigma1 = query_f64(
            &store,
            &format!("SELECT ?val WHERE {{ <{}> <{}> ?val . }}", s1, vocab::HIST_STD_DEV_IRI),
        );
        assert!(
            (sigma1 - expected_sigma).abs() < 1e-9,
            "sensor1 histStdDev should be {}, got {}",
            expected_sigma,
            sigma1
        );

        // --- sensor2 mean ---
        let mean2 = query_f64(
            &store,
            &format!("SELECT ?val WHERE {{ <{}> <{}> ?val . }}", s2, vocab::HIST_MEAN_IRI),
        );
        assert!(
            (mean2 - 11.0).abs() < 1e-9,
            "sensor2 histMean should be 11.0, got {}",
            mean2
        );

        // --- sensor2 std_dev (same formula, same σ) ---
        let sigma2 = query_f64(
            &store,
            &format!("SELECT ?val WHERE {{ <{}> <{}> ?val . }}", s2, vocab::HIST_STD_DEV_IRI),
        );
        assert!(
            (sigma2 - expected_sigma).abs() < 1e-9,
            "sensor2 histStdDev should be {}, got {}",
            expected_sigma,
            sigma2
        );
    }

    /// A sensor with a single observation must get std_dev = 0.
    #[test]
    fn test_materialise_stats_single_observation_sigma_zero() {
        let store = Store::new().unwrap();
        let pred = "http://test.org/val";
        let s1 = "http://test.org/sensor1";
        insert_decimal(&store, s1, pred, "42.0");

        materialise_stats(&store).expect("must succeed");

        let sigma = query_f64(
            &store,
            &format!("SELECT ?val WHERE {{ <{}> <{}> ?val . }}", s1, vocab::HIST_STD_DEV_IRI),
        );
        assert!(
            sigma.abs() < 1e-9,
            "single-observation std_dev must be 0, got {}",
            sigma
        );
    }

    /// materialise_stats must return NoObservations when the store has no numeric literals.
    #[test]
    fn test_materialise_stats_empty_store_returns_no_observations() {
        let store = Store::new().unwrap();
        let result = materialise_stats(&store);
        assert!(
            matches!(result, Err(HistoricalExecutorError::NoObservations)),
            "expected NoObservations, got {:?}",
            result
        );
    }

    // -----------------------------------------------------------------------
    // Step 4, Test 3: full two-pass — absolute_threshold_exceeded filter
    // -----------------------------------------------------------------------

    /// Full two-pass integration test using vocab constants throughout.
    ///
    /// sensor1: hist observations [1.0, 2.0, 3.0] → histMean = 2.0
    ///          live mean inserted manually as 5.0
    ///          |5.0 − 2.0| = 3.0 > 1.0 → anomaly, must appear in results
    ///
    /// sensor2: hist observations [10.0, 11.0, 12.0] → histMean = 11.0
    ///          live mean inserted manually as 11.5
    ///          |11.5 − 11.0| = 0.5 < 1.0 → normal, must NOT appear
    #[test]
    fn test_two_pass_absolute_threshold_filter() {
        let store = Store::new().unwrap();
        let obs_pred = "http://test.org/val";
        let live_pred = "http://test.org/liveMean";
        let s1 = "http://test.org/sensor1";
        let s2 = "http://test.org/sensor2";

        // Insert historical observations (raw data — Pass 1 will aggregate these)
        for v in ["1.0", "2.0", "3.0"] {
            insert_decimal(&store, s1, obs_pred, v);
        }
        for v in ["10.0", "11.0", "12.0"] {
            insert_decimal(&store, s2, obs_pred, v);
        }

        // Insert live means (simulating what the live window would provide)
        insert_decimal(&store, s1, live_pred, "5.0");
        insert_decimal(&store, s2, live_pred, "11.5");

        // --- Pass 1 ---
        materialise_stats(&store).expect("Pass 1 must succeed");

        // --- Pass 2: query using vocab constants (no hardcoded IRI strings) ---
        let user_query = format!(
            r#"
            PREFIX janus:  <https://janus.rs/fn#>
            PREFIX stat:   <https://janus.rs/stat#>
            PREFIX test:   <http://test.org/>
            SELECT ?sensor WHERE {{
                ?sensor <{stat_mean}> ?histMean ;
                        <{live_pred}> ?liveMean .
                FILTER(janus:absolute_threshold_exceeded(?liveMean, ?histMean, "1.0"^^<http://www.w3.org/2001/XMLSchema#decimal>))
            }}
            "#,
            stat_mean = vocab::HIST_MEAN_IRI,
            live_pred = live_pred,
        );

        let results = run_user_query_on_store(&store, &user_query)
            .expect("Pass 2 must succeed");

        assert_eq!(
            results.len(),
            1,
            "expected exactly one anomalous sensor, got {} rows: {:?}",
            results.len(),
            results
        );

        let sensor_val = results[0].get("sensor").expect("?sensor must be bound");
        assert!(
            sensor_val.contains("sensor1"),
            "only sensor1 should be anomalous, got: {}",
            sensor_val
        );
    }

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
