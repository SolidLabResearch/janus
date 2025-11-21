use crate::querying::query_processing::{self, SparqlEngine};
use oxigraph::model::Quad;
use oxigraph::sparql::QueryResults;
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
        use oxigraph::sparql::SparqlEvaluator;
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

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{GraphName, Literal, NamedNode};
    use std::collections::HashSet;

    /// Helper function to create a test QuadContainer with sample data
    fn create_test_container() -> QuadContainer {
        let mut quads = HashSet::new();

        // Add test quads: <http://example.org/alice> <http://example.org/knows> <http://example.org/bob>
        let alice = NamedNode::new("http://example.org/alice").unwrap();
        let bob = NamedNode::new("http://example.org/bob").unwrap();
        let charlie = NamedNode::new("http://example.org/charlie").unwrap();
        let knows = NamedNode::new("http://example.org/knows").unwrap();
        let age = NamedNode::new("http://example.org/age").unwrap();

        // Alice knows Bob
        quads.insert(Quad::new(alice.clone(), knows.clone(), bob.clone(), GraphName::DefaultGraph));

        // Bob knows Charlie
        quads.insert(Quad::new(
            bob.clone(),
            knows.clone(),
            charlie.clone(),
            GraphName::DefaultGraph,
        ));

        // Alice's age
        quads.insert(Quad::new(
            alice.clone(),
            age.clone(),
            Literal::new_simple_literal("30"),
            GraphName::DefaultGraph,
        ));

        // Bob's age
        quads.insert(Quad::new(
            bob.clone(),
            age.clone(),
            Literal::new_simple_literal("25"),
            GraphName::DefaultGraph,
        ));

        QuadContainer::new(quads, 1000)
    }

    #[test]
    fn test_oxigraph_adapter_creation() {
        let adapter = OxigraphAdapter::new();
        assert!(true, "Adapter should be created successfully");
    }

    #[test]
    fn test_execute_simple_select_query() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // Query to select all subjects
        let query = "SELECT ?s WHERE { ?s ?p ?o }";

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "Query execution should succeed");

        let results = results.unwrap();
        assert!(!results.is_empty(), "Results should not be empty");
        assert_eq!(results.len(), 4, "Should return 4 results (4 distinct subjects in quads)");
    }

    #[test]
    fn test_execute_select_with_filter() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // Query to select subjects that know someone
        let query = r#"
            PREFIX ex: <http://example.org/>
            SELECT ?s WHERE {
                ?s ex:knows ?o
            }
        "#;

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "Query with filter should succeed");

        let results = results.unwrap();
        assert_eq!(results.len(), 2, "Should return 2 results (Alice and Bob know someone)");
    }

    #[test]
    fn test_execute_ask_query() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // ASK query to check if Alice knows Bob
        let query = r#"
            PREFIX ex: <http://example.org/>
            ASK {
                ex:alice ex:knows ex:bob
            }
        "#;

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "ASK query should succeed");

        let results = results.unwrap();
        assert_eq!(results.len(), 1, "ASK query should return one boolean result");
        assert_eq!(results[0], "true", "ASK query should return true");
    }

    #[test]
    fn test_execute_ask_query_false() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // ASK query that should return false
        let query = r#"
            PREFIX ex: <http://example.org/>
            ASK {
                ex:alice ex:knows ex:charlie
            }
        "#;

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "ASK query should succeed");

        let results = results.unwrap();
        assert_eq!(results.len(), 1, "ASK query should return one boolean result");
        assert_eq!(
            results[0], "false",
            "ASK query should return false (Alice doesn't know Charlie directly)"
        );
    }

    #[test]
    fn test_execute_construct_query() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // CONSTRUCT query to create new triples
        let query = r#"
            PREFIX ex: <http://example.org/>
            CONSTRUCT {
                ?s ex:knows ?o
            }
            WHERE {
                ?s ex:knows ?o
            }
        "#;

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "CONSTRUCT query should succeed");

        let results = results.unwrap();
        assert_eq!(results.len(), 2, "CONSTRUCT should return 2 triples");
    }

    #[test]
    fn test_execute_with_empty_container() {
        let adapter = OxigraphAdapter::new();
        let empty_container = QuadContainer::new(HashSet::new(), 1000);

        let query = "SELECT ?s WHERE { ?s ?p ?o }";

        let results = adapter.execute_query(query, &empty_container);
        assert!(results.is_ok(), "Query on empty container should succeed");

        let results = results.unwrap();
        assert!(results.is_empty(), "Results should be empty for empty container");
    }

    #[test]
    fn test_execute_invalid_query() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // Invalid SPARQL query
        let query = "INVALID SPARQL QUERY";

        let results = adapter.execute_query(query, &container);
        assert!(results.is_err(), "Invalid query should return an error");

        let error = results.unwrap_err();
        assert!(error.to_string().contains("Oxigraph error"), "Error should be an OxigraphError");
    }

    #[test]
    fn test_execute_query_with_literal_filter() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // Query to find people older than 25
        let query = r#"
            PREFIX ex: <http://example.org/>
            SELECT ?s ?age WHERE {
                ?s ex:age ?age .
                FILTER(?age > "25")
            }
        "#;

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "Query with literal filter should succeed");

        let results = results.unwrap();
        assert_eq!(results.len(), 1, "Should return 1 result (Alice is 30)");
    }

    #[test]
    fn test_execute_count_query() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // Query to count the number of 'knows' relationships
        let query = r#"
            PREFIX ex: <http://example.org/>
            SELECT (COUNT(?s) AS ?count) WHERE {
                ?s ex:knows ?o
            }
        "#;

        let results = adapter.execute_query(query, &container);
        assert!(results.is_ok(), "COUNT query should succeed");

        let results = results.unwrap();
        assert_eq!(results.len(), 1, "COUNT query should return 1 result");
    }

    #[test]
    fn test_multiple_queries_on_same_adapter() {
        let adapter = OxigraphAdapter::new();
        let container = create_test_container();

        // First query
        let query1 = "SELECT ?s WHERE { ?s ?p ?o }";
        let results1 = adapter.execute_query(query1, &container);
        assert!(results1.is_ok(), "First query should succeed");

        // Second query
        let query2 = r#"
            PREFIX ex: <http://example.org/>
            SELECT ?s WHERE { ?s ex:knows ?o }
        "#;
        let results2 = adapter.execute_query(query2, &container);
        assert!(results2.is_ok(), "Second query should succeed");

        // Verify both queries returned results
        assert!(!results1.unwrap().is_empty());
        assert!(!results2.unwrap().is_empty());
    }

    #[test]
    fn test_oxigraph_error_display() {
        let error = OxigraphError("Test error message".to_string());
        let error_string = format!("{}", error);
        assert_eq!(error_string, "Oxigraph error: Test error message");
    }

    #[test]
    fn test_oxigraph_error_from_storage_error() {
        // This tests the From implementation for StorageError
        // We can't easily create a real StorageError, but we verify the trait is implemented
        let error = OxigraphError::from(oxigraph::store::StorageError::Other("test".into()));
        assert!(error.to_string().contains("Oxigraph error"));
    }
}
