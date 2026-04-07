use janus::querying::oxigraph_adapter::{OxigraphAdapter, OxigraphError};
use janus::querying::query_processing::SparqlEngine;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use rsp_rs::QuadContainer;
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
    quads.insert(Quad::new(bob.clone(), knows.clone(), charlie.clone(), GraphName::DefaultGraph));

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
    let _adapter = OxigraphAdapter::new();
    // Adapter created successfully
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
    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?s WHERE {
            ?s ex:knows ?o
        }
    ";

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
    let query = r"
        PREFIX ex: <http://example.org/>
        ASK {
            ex:alice ex:knows ex:bob
        }
    ";

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
    let query = r"
        PREFIX ex: <http://example.org/>
        ASK {
            ex:alice ex:knows ex:charlie
        }
    ";

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
    let query = r"
        PREFIX ex: <http://example.org/>
        CONSTRUCT {
            ?s ex:knows ?o
        }
        WHERE {
            ?s ex:knows ?o
        }
    ";

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
    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT (COUNT(?s) AS ?count) WHERE {
            ?s ex:knows ?o
        }
    ";

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
    let query2 = r"
        PREFIX ex: <http://example.org/>
        SELECT ?s WHERE { ?s ex:knows ?o }
    ";
    let results2 = adapter.execute_query(query2, &container);
    assert!(results2.is_ok(), "Second query should succeed");

    // Verify both queries returned results
    assert!(!results1.unwrap().is_empty());
    assert!(!results2.unwrap().is_empty());
}

#[test]
fn test_oxigraph_error_display() {
    let error =
        OxigraphError::from(oxigraph::store::StorageError::Other("Test error message".into()));
    let error_string = format!("{}", error);
    assert!(error_string.contains("Oxigraph error"));
    assert!(error_string.contains("Test error message"));
}

#[test]
fn test_oxigraph_error_from_storage_error() {
    // This tests the From implementation for StorageError
    // We can't easily create a real StorageError, but we verify the trait is implemented
    let error = OxigraphError::from(oxigraph::store::StorageError::Other("test".into()));
    assert!(error.to_string().contains("Oxigraph error"));
}

// Tests for execute_query_bindings

#[test]
fn test_execute_query_bindings_simple_select() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?s ?o WHERE {
            ?s ex:knows ?o
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Query bindings execution should succeed");

    let bindings = bindings.unwrap();
    assert_eq!(bindings.len(), 2, "Should return 2 bindings (Alice->Bob, Bob->Charlie)");

    // Verify structure of bindings
    for binding in &bindings {
        assert!(binding.contains_key("s"), "Binding should contain 's' variable");
        assert!(binding.contains_key("o"), "Binding should contain 'o' variable");
    }
}

#[test]
fn test_execute_query_bindings_with_literals() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?person ?age WHERE {
            ?person ex:age ?age
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Query with literals should succeed");

    let bindings = bindings.unwrap();
    assert_eq!(bindings.len(), 2, "Should return 2 bindings (Alice and Bob ages)");

    // Verify each binding has both variables
    for binding in &bindings {
        assert!(binding.contains_key("person"), "Should have 'person' variable");
        assert!(binding.contains_key("age"), "Should have 'age' variable");

        let age = binding.get("age").unwrap();
        assert!(age == "\"30\"" || age == "\"25\"", "Age should be either 30 or 25");
    }
}

#[test]
fn test_execute_query_bindings_single_variable() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = "SELECT ?s WHERE { ?s ?p ?o }";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Single variable query should succeed");

    let bindings = bindings.unwrap();
    assert_eq!(bindings.len(), 4, "Should return 4 bindings");

    // Verify each binding has only the 's' variable
    for binding in &bindings {
        assert_eq!(binding.len(), 1, "Each binding should have exactly 1 variable");
        assert!(binding.contains_key("s"), "Binding should contain 's' variable");
    }
}

#[test]
fn test_execute_query_bindings_with_filter() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?person ?age WHERE {
            ?person ex:age ?age .
            FILTER(?age > "25")
        }
    "#;

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Query with filter should succeed");

    let bindings = bindings.unwrap();
    assert_eq!(bindings.len(), 1, "Should return 1 binding (only Alice is > 25)");

    let binding = &bindings[0];
    assert!(binding.get("person").unwrap().contains("alice"), "Person should be Alice");
    assert_eq!(binding.get("age").unwrap(), "\"30\"", "Age should be 30");
}

#[test]
fn test_execute_query_bindings_empty_result() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    // Query that matches nothing
    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?s WHERE {
            ?s ex:nonexistent ?o
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Query with no results should succeed");

    let bindings = bindings.unwrap();
    assert!(bindings.is_empty(), "Should return empty bindings list");
}

#[test]
fn test_execute_query_bindings_empty_container() {
    let adapter = OxigraphAdapter::new();
    let empty_container = QuadContainer::new(HashSet::new(), 1000);

    let query = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }";

    let bindings = adapter.execute_query_bindings(query, &empty_container);
    assert!(bindings.is_ok(), "Query on empty container should succeed");

    let bindings = bindings.unwrap();
    assert!(bindings.is_empty(), "Should return empty bindings for empty container");
}

#[test]
fn test_execute_query_bindings_ask_query_returns_empty() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    // ASK queries don't return bindings
    let query = r"
        PREFIX ex: <http://example.org/>
        ASK {
            ex:alice ex:knows ex:bob
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "ASK query should succeed");

    let bindings = bindings.unwrap();
    assert!(
        bindings.is_empty(),
        "ASK queries should return empty bindings (use execute_query instead)"
    );
}

#[test]
fn test_execute_query_bindings_construct_query_returns_empty() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    // CONSTRUCT queries don't return bindings
    let query = r"
        PREFIX ex: <http://example.org/>
        CONSTRUCT {
            ?s ex:knows ?o
        }
        WHERE {
            ?s ex:knows ?o
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "CONSTRUCT query should succeed");

    let bindings = bindings.unwrap();
    assert!(
        bindings.is_empty(),
        "CONSTRUCT queries should return empty bindings (use execute_query instead)"
    );
}

#[test]
fn test_execute_query_bindings_invalid_query() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = "INVALID SPARQL QUERY";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_err(), "Invalid query should return an error");

    let error = bindings.unwrap_err();
    assert!(error.to_string().contains("Oxigraph error"), "Error should be an OxigraphError");
}

#[test]
fn test_execute_query_bindings_multiple_variables() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?s ?p ?o WHERE {
            ?s ?p ?o
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Query with multiple variables should succeed");

    let bindings = bindings.unwrap();
    assert_eq!(bindings.len(), 4, "Should return 4 bindings (one per quad)");

    // Verify each binding has all three variables
    for binding in &bindings {
        assert_eq!(binding.len(), 3, "Each binding should have exactly 3 variables");
        assert!(binding.contains_key("s"), "Should have 's' variable");
        assert!(binding.contains_key("p"), "Should have 'p' variable");
        assert!(binding.contains_key("o"), "Should have 'o' variable");
    }
}

#[test]
fn test_execute_query_bindings_with_aggregation() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT (COUNT(?s) AS ?count) WHERE {
            ?s ex:knows ?o
        }
    ";

    let bindings = adapter.execute_query_bindings(query, &container);
    assert!(bindings.is_ok(), "Query with aggregation should succeed");

    let bindings = bindings.unwrap();
    assert_eq!(bindings.len(), 1, "Aggregation should return 1 binding");

    let binding = &bindings[0];
    assert!(binding.contains_key("count"), "Should have 'count' variable");
    assert_eq!(
        binding.get("count").unwrap(),
        "\"2\"^^<http://www.w3.org/2001/XMLSchema#integer>",
        "Count should be 2"
    );
}

#[test]
fn test_execute_query_bindings_comparison_with_execute_query() {
    let adapter = OxigraphAdapter::new();
    let container = create_test_container();

    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?s WHERE {
            ?s ex:knows ?o
        }
    ";

    // Execute with both methods
    let debug_results = adapter.execute_query(query, &container).unwrap();
    let bindings = adapter.execute_query_bindings(query, &container).unwrap();

    // Both should return the same number of results
    assert_eq!(
        debug_results.len(),
        bindings.len(),
        "Both methods should return same number of results"
    );
    assert_eq!(bindings.len(), 2, "Should have 2 results");
}
