use crate::querying::query_processing::{self, SparqlEngine};
use oxigraph::model::Quad;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use rsp_rs::QuadContainer;
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
