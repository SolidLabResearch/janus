//! Oxigraph-based SPARQL query engine adapter.
//!
//! This module provides an adapter for executing SPARQL queries using the Oxigraph engine.
//! It supports both legacy string-based results (`execute_query`) and structured bindings
//! (`execute_query_bindings`).
//!
//! # Example
//!
//! ```ignore
//! use janus::querying::oxigraph_adapter::OxigraphAdapter;
//! use oxigraph::model::{GraphName, NamedNode, Quad};
//! use rsp_rs::QuadContainer;
//! use std::collections::HashSet;
//!
//! // Create adapter
//! let adapter = OxigraphAdapter::new();
//!
//! // Create test data
//! let mut quads = HashSet::new();
//! let alice = NamedNode::new("http://example.org/alice").unwrap();
//! let bob = NamedNode::new("http://example.org/bob").unwrap();
//! let knows = NamedNode::new("http://example.org/knows").unwrap();
//! quads.insert(Quad::new(alice, knows, bob, GraphName::DefaultGraph));
//!
//! let container = QuadContainer::new(quads, 1000);
//!
//! // Execute query with structured bindings
//! let query = r"
//!     PREFIX ex: <http://example.org/>
//!     SELECT ?s ?o WHERE { ?s ex:knows ?o }
//! ";
//!
//! let bindings = adapter.execute_query_bindings(query, &container).unwrap();
//! for binding in bindings {
//!     println!("Subject: {}, Object: {}",
//!              binding.get("s").unwrap(),
//!              binding.get("o").unwrap());
//! }
//! ```

use crate::querying::query_processing::{self, SparqlEngine};
use oxigraph::model::Quad;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use rsp_rs::QuadContainer;
use std::collections::HashMap;
use std::fmt;

pub struct OxigraphStore {}

#[derive(Debug)]
pub struct OxigraphError(String);

impl fmt::Display for OxigraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Oxigraph error: {}", self.0)
    }
}

impl std::error::Error for OxigraphError {}

impl From<oxigraph::store::StorageError> for OxigraphError {
    fn from(err: oxigraph::store::StorageError) -> Self {
        OxigraphError(err.to_string())
    }
}

impl From<oxigraph::sparql::QueryEvaluationError> for OxigraphError {
    fn from(err: oxigraph::sparql::QueryEvaluationError) -> Self {
        OxigraphError(err.to_string())
    }
}

pub struct OxigraphAdapter {
    #[allow(dead_code)]
    store: OxigraphStore,
}

impl OxigraphAdapter {
    pub fn new() -> Self {
        Self { store: OxigraphStore {} }
    }

    /// Execute a SPARQL query and return structured bindings as a Vec of HashMaps.
    /// Each HashMap represents one solution/row with variable names as keys and bound values as strings.
    ///
    /// # Arguments
    /// * `query` - The SPARQL query string
    /// * `container` - The QuadContainer with RDF data to query against
    ///
    /// # Returns
    /// A vector of HashMaps where each HashMap contains variable bindings for one solution.
    /// Returns an empty vector for ASK queries or CONSTRUCT queries.
    ///
    /// # Example
    /// ```ignore
    /// let adapter = OxigraphAdapter::new();
    /// let bindings = adapter.execute_query_bindings("SELECT ?s ?p WHERE { ?s ?p ?o }", &container)?;
    /// for binding in bindings {
    ///     println!("s: {:?}, p: {:?}", binding.get("s"), binding.get("p"));
    /// }
    /// ```
    pub fn execute_query_bindings(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<HashMap<String, String>>, OxigraphError> {
        let store = Store::new()?;

        // Insert all quads into the store
        for quad in &container.elements {
            store.insert(quad)?;
        }

        #[cfg(debug_assertions)]
        {
            println!("Executing query on Oxigraph store with {} quads", container.len());
            println!("Query: {}", query);
        }

        // Execute the query using the SparqlEvaluator API
        let evaluator = SparqlEvaluator::new();
        let parsed_query =
            evaluator.parse_query(query).map_err(|e| OxigraphError(e.to_string()))?;
        let results = parsed_query.on_store(&store).execute()?;

        let mut bindings_list = Vec::new();

        // Only process SELECT queries that return solutions
        if let QueryResults::Solutions(solutions) = results {
            for solution in solutions {
                let solution = solution?;
                let mut binding = HashMap::new();

                // Extract each variable binding from the solution
                for (var, term) in solution.iter() {
                    binding.insert(var.as_str().to_string(), term.to_string());
                }

                bindings_list.push(binding);
            }
        }
        // For ASK and CONSTRUCT queries, return empty vector
        // Users should use execute_query() for those query types

        Ok(bindings_list)
    }
}

impl SparqlEngine for OxigraphAdapter {
    type EngineError = OxigraphError;

    fn execute_query(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<String>, Self::EngineError> {
        let store = Store::new()?;

        for quad in &container.elements {
            store.insert(quad)?;
        }

        #[cfg(debug_assertions)]
        {
            println!("Executing query on Oxigraph store with {} quads", container.len());
            println!("Query: {}", query);
            for (i, quad) in container.elements.iter().enumerate() {
                println!("Quad {}: {:?}", i + 1, quad);
            }
        }

        // Execute the query using the new SparqlEvaluator API
        let evaluator = SparqlEvaluator::new();
        let parsed_query =
            evaluator.parse_query(query).map_err(|e| OxigraphError(e.to_string()))?;
        let results = parsed_query.on_store(&store).execute()?;

        // Convert QueryResults to Vec<String>
        let mut result_strings = Vec::new();

        if let QueryResults::Solutions(solutions) = results {
            for solution in solutions {
                let solution = solution?;
                result_strings.push(format!("{:?}", solution));
            }
        } else if let QueryResults::Boolean(b) = results {
            result_strings.push(format!("{}", b));
        } else if let QueryResults::Graph(graph) = results {
            for triple in graph {
                let triple = triple?;
                result_strings.push(format!("{:?}", triple));
            }
        }

        Ok(result_strings)
    }
}
