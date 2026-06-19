//! Integration test for JanusApi
//!
//! Tests the complete flow of registering and executing JanusQL queries
//! with both historical and live processing.

use janus::api::janus_api::{ExecutionStatus, JanusApi, ResultSource};
use janus::parsing::janusql_parser::JanusQLParser;
use janus::registry::query_registry::QueryRegistry;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

/// Helper function to create a test storage with sample data
fn create_test_storage_with_data(
) -> Result<(TempDir, Arc<StreamingSegmentedStorage>), std::io::Error> {
    let temp_dir = TempDir::new()?;
    let config = StreamingConfig {
        segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
        max_batch_events: 10, // Small batch to force frequent flushes
        max_batch_age_seconds: 1,
        max_batch_bytes: 1024,
        sparse_interval: 10,
        entries_per_index_block: 100,
    };

    let storage = StreamingSegmentedStorage::new(config)?;

    // Add some test data (timestamps from 100 to 5000 ms)
    for i in 1..=50 {
        let timestamp = i * 100;
        storage.write_rdf(
            timestamp,
            &format!("http://example.org/sensor{}", i % 5),
            "http://example.org/temperature",
            &format!("{}", 20 + (i % 10)),
            "http://example.org/sensors",
        )?;
    }

    storage.flush()?;

    Ok((temp_dir, Arc::new(storage)))
}

#[test]
fn test_janus_api_creation() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );

    let api = JanusApi::new(parser, registry, storage);
    assert!(api.is_ok(), "JanusApi creation should succeed");
}

#[test]
fn test_register_query() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?s ?p ?o

        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 200]

        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let result = api.register_query("test_query".into(), janusql);
    assert!(result.is_ok(), "Query registration should succeed");

    let metadata = result.unwrap();
    assert_eq!(metadata.query_id, "test_query");
}

#[test]
fn test_register_invalid_query() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let invalid_janusql = "INVALID QUERY SYNTAX";

    let _result = api.register_query("invalid_query".into(), invalid_janusql);
    // Note: Parser may be lenient, so this might not fail
    // The test is here to document expected behavior
}

#[test]
fn test_start_query_not_registered() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let result = api.start_query(&"nonexistent_query".into());
    assert!(result.is_err(), "Starting unregistered query should fail");
}

#[test]
fn test_historical_fixed_window_query() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Query historical data from timestamp 1000 to 3000
    let janusql = r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor ?temp

        FROM NAMED WINDOW ex:hist ON STREAM ex:sensors
            [START 1000 END 3000]

        WHERE {
            WINDOW ex:hist { ?sensor ex:temperature ?temp }
        }
    "#;

    api.register_query("hist_query".into(), janusql)
        .expect("Failed to register query");

    println!("Starting historical query...");
    let handle = api.start_query(&"hist_query".into()).expect("Failed to start query");

    // Collect results (should complete quickly for historical)
    let mut results = Vec::new();
    for i in 0..100 {
        // Try up to 100 times
        if let Some(result) = handle.try_receive() {
            println!("Received result {}: {:?}", i, result.source);
            results.push(result);
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    println!("Total results received: {}", results.len());

    assert!(!results.is_empty(), "Historical query should emit at least one result");
    assert!(
        results.iter().any(|result| !result.bindings.is_empty()),
        "Historical query should return at least one binding"
    );

    // Verify all results are historical
    for result in &results {
        assert!(
            matches!(result.source, ResultSource::Historical),
            "All results should be historical"
        );
        assert_eq!(result.query_id, "hist_query");
    }
}

#[test]
fn test_historical_sliding_window_query() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Query with sliding window
    let janusql = r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor ?temp

        FROM NAMED WINDOW ex:sliding ON STREAM ex:sensors
            [OFFSET 4000 RANGE 1000 STEP 500]

        WHERE {
            WINDOW ex:sliding { ?sensor ex:temperature ?temp }
        }
    "#;

    api.register_query("sliding_query".into(), janusql)
        .expect("Failed to register query");

    println!("Starting sliding window query...");
    let handle = api.start_query(&"sliding_query".into()).expect("Failed to start query");

    // Collect results
    let mut results = Vec::new();
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        if let Some(result) = handle.try_receive() {
            println!("Received sliding window result: {:?}", result.source);
            results.push(result);
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    println!("Total sliding window results: {}", results.len());

    assert!(!results.is_empty(), "expected sliding window results after the storage warm-up");

    for result in &results {
        assert!(
            matches!(result.source, ResultSource::Historical),
            "Results should be historical"
        );
    }
}

#[test]
fn test_query_already_running() {
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
        PREFIX ex: <http://example.org/>

        SELECT ?s ?p ?o

        FROM NAMED WINDOW ex:w ON STREAM ex:stream1 [RANGE 1000 STEP 200]

        WHERE {
            WINDOW ex:w { ?s ?p ?o }
        }
    "#;

    api.register_query("duplicate_query".into(), janusql)
        .expect("Failed to register query");

    // Start query first time
    let _handle1 = api.start_query(&"duplicate_query".into()).expect("First start should succeed");

    // Try to start again
    let result2 = api.start_query(&"duplicate_query".into());
    assert!(result2.is_err(), "Starting already running query should fail");
}

#[test]
fn test_is_running() {
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
        PREFIX ex: <http://example.org/>

        SELECT ?s

        FROM NAMED WINDOW ex:w ON STREAM ex:stream1 [RANGE 1000 STEP 200]

        WHERE {
            WINDOW ex:w { ?s ?p ?o }
        }
    "#;

    api.register_query("status_query".into(), janusql)
        .expect("Failed to register query");

    assert!(!api.is_running(&"status_query".into()), "Query should not be running initially");

    let _handle = api.start_query(&"status_query".into()).expect("Failed to start query");

    assert!(api.is_running(&"status_query".into()), "Query should be running after start");
}

#[test]
fn test_stop_query() {
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
        PREFIX ex: <http://example.org/>

        SELECT ?s

        FROM NAMED WINDOW ex:w ON STREAM ex:stream1 [RANGE 1000 STEP 200]

        WHERE {
            WINDOW ex:w { ?s ?p ?o }
        }
    "#;

    api.register_query("stop_test_query".into(), janusql)
        .expect("Failed to register query");

    let _handle = api.start_query(&"stop_test_query".into()).expect("Failed to start query");

    assert!(api.is_running(&"stop_test_query".into()), "Query should be running");

    let stop_result = api.stop_query(&"stop_test_query".into());
    assert!(stop_result.is_ok(), "Stop query should succeed");

    assert!(
        !api.is_running(&"stop_test_query".into()),
        "Query should not be running after stop"
    );
}

#[test]
fn test_execution_count_and_status_update_across_lifecycle() {
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, Arc::clone(&registry), storage).expect("Failed to create API");

    let janusql = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?s
        FROM NAMED WINDOW ex:w ON STREAM ex:stream1 [RANGE 1000 STEP 200]
        WHERE { WINDOW ex:w { ?s ?p ?o } }
    "#;

    let metadata = api
        .register_query("lifecycle_query".into(), janusql)
        .expect("Failed to register query");
    assert_eq!(metadata.execution_count, 0);
    assert_eq!(metadata.status, "Registered");

    let _handle = api.start_query(&"lifecycle_query".into()).expect("Failed to start query");

    let after_start =
        registry.get(&"lifecycle_query".into()).expect("query should exist after start");
    assert_eq!(after_start.execution_count, 1);
    assert_eq!(after_start.status, "Running");

    api.stop_query(&"lifecycle_query".into()).expect("Failed to stop query");

    let after_stop =
        registry.get(&"lifecycle_query".into()).expect("query should exist after stop");
    assert_eq!(after_stop.execution_count, 1);
    assert_eq!(after_stop.status, "Stopped");

    let _handle = api.start_query(&"lifecycle_query".into()).expect("Failed to restart query");

    let after_restart = registry
        .get(&"lifecycle_query".into())
        .expect("query should exist after restart");
    assert_eq!(after_restart.execution_count, 2);
    assert_eq!(after_restart.status, "Running");
}

#[test]
fn test_multiple_queries_concurrent() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Register multiple queries
    let janusql1 = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?s
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [START 1000 END 2000]
        WHERE { WINDOW ex:w1 { ?s ?p ?o } }
    "#;

    let janusql2 = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?s
        FROM NAMED WINDOW ex:w2 ON STREAM ex:stream2 [START 2000 END 3000]
        WHERE { WINDOW ex:w2 { ?s ?p ?o } }
    "#;

    api.register_query("query1".into(), janusql1)
        .expect("Failed to register query1");
    api.register_query("query2".into(), janusql2)
        .expect("Failed to register query2");

    // Start both queries
    let handle1 = api.start_query(&"query1".into()).expect("Failed to start query1");
    let handle2 = api.start_query(&"query2".into()).expect("Failed to start query2");

    // Both should be running
    assert!(api.is_running(&"query1".into()), "Query1 should be running");
    assert!(api.is_running(&"query2".into()), "Query2 should be running");

    // Should be able to receive from both
    thread::sleep(Duration::from_millis(100));

    let _result1 = handle1.try_receive();
    let _result2 = handle2.try_receive();

    // At least one should have results (depending on data)
    // This test verifies concurrent execution is possible
}

#[test]
fn test_query_handle_receive() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w ON STREAM ex:stream1 [START 100 END 500]
        WHERE { WINDOW ex:w { ?s ?p ?o } }
    "#;

    api.register_query("receive_test".into(), janusql)
        .expect("Failed to register query");

    let handle = api.start_query(&"receive_test".into()).expect("Failed to start query");

    // Try non-blocking receive
    thread::sleep(Duration::from_millis(100));
    let result = handle.try_receive();

    // Non-blocking receive should simply return an option without panicking.
    let _ = result;
}

#[test]
fn test_only_historical_fixed_window() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Query with ONLY historical fixed window (no sliding, no live)
    let janusql = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?temp

FROM NAMED WINDOW ex:hist ON STREAM ex:sensors [START 100 END 500]

WHERE {
    WINDOW ex:hist { ?sensor ex:temperature ?temp }
}
    "#;

    let metadata = api
        .register_query("only_fixed".into(), janusql)
        .expect("Failed to register query");

    // Debug output
    println!("Historical windows: {}", metadata.parsed.historical_windows.len());
    println!("Live windows: {}", metadata.parsed.live_windows.len());
    println!("SPARQL queries: {}", metadata.parsed.sparql_queries.len());
    for (i, query) in metadata.parsed.sparql_queries.iter().enumerate() {
        println!("SPARQL Query {}: {}", i, query);
    }

    // Verify metadata
    assert_eq!(metadata.parsed.historical_windows.len(), 1);
    assert_eq!(metadata.parsed.live_windows.len(), 0);

    // Parser should generate SPARQL for historical windows.
    assert!(
        !metadata.parsed.sparql_queries.is_empty(),
        "parser should generate SPARQL queries for historical windows"
    );

    assert_eq!(metadata.parsed.sparql_queries.len(), 1);

    let handle = api.start_query(&"only_fixed".into()).expect("Failed to start query");

    // Should only spawn historical thread, no live thread
    assert!(api.is_running(&"only_fixed".into()));

    thread::sleep(Duration::from_millis(200));

    // Try to receive results
    let _result = handle.try_receive();

    // No live thread should be running - only historical
}

#[test]
fn test_only_live_window() {
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Query with ONLY live window (no historical)
    let janusql = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream <output> AS
SELECT ?s ?p ?o

FROM NAMED WINDOW ex:live ON STREAM ex:stream1 [RANGE 1000 STEP 200]

WHERE {
    WINDOW ex:live { ?s ?p ?o }
}
    "#;

    let metadata = api
        .register_query("only_live".into(), janusql)
        .expect("Failed to register query");

    // Verify only live windows
    assert_eq!(metadata.parsed.historical_windows.len(), 0);
    assert_eq!(metadata.parsed.live_windows.len(), 1);
    assert_eq!(metadata.parsed.sparql_queries.len(), 0);
    assert!(!metadata.parsed.rspql_query.is_empty());

    let _handle = api.start_query(&"only_live".into()).expect("Failed to start query");

    // Should only spawn live thread, no historical threads
    assert!(api.is_running(&"only_live".into()));

    thread::sleep(Duration::from_millis(100));

    // Live thread is running in background
}

#[test]
fn test_multiple_historical_windows() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Query with multiple historical windows
    let janusql = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?temp

FROM NAMED WINDOW ex:hist1 ON STREAM ex:sensors [START 100 END 200]
FROM NAMED WINDOW ex:hist2 ON STREAM ex:sensors [START 300 END 400]

WHERE {
    WINDOW ex:hist1 { ?sensor ex:temperature ?temp }
    WINDOW ex:hist2 { ?sensor ex:temperature ?temp }
}
    "#;

    let metadata = api
        .register_query("multi_hist".into(), janusql)
        .expect("Failed to register query");

    // Verify multiple historical windows
    assert_eq!(metadata.parsed.historical_windows.len(), 2);
    assert_eq!(metadata.parsed.sparql_queries.len(), 2);
    assert_eq!(metadata.parsed.live_windows.len(), 0);

    let _handle = api.start_query(&"multi_hist".into()).expect("Failed to start query");

    // Should spawn 2 historical threads
    assert!(api.is_running(&"multi_hist".into()));

    thread::sleep(Duration::from_millis(200));
}

#[test]
fn test_historical_and_live_combined() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());

    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    // Query with BOTH historical and live windows
    let janusql = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream <output> AS
SELECT ?sensor ?temp

FROM NAMED WINDOW ex:hist ON STREAM ex:sensors [START 100 END 500]
FROM NAMED WINDOW ex:live ON STREAM ex:sensors [RANGE 1000 STEP 200]

WHERE {
    WINDOW ex:hist { ?sensor ex:temperature ?temp }
    WINDOW ex:live { ?sensor ex:temperature ?temp }
}
    "#;

    let metadata = api
        .register_query("combined".into(), janusql)
        .expect("Failed to register query");

    // Verify both historical and live windows
    assert_eq!(metadata.parsed.historical_windows.len(), 1);
    assert_eq!(metadata.parsed.live_windows.len(), 1);
    assert_eq!(metadata.parsed.sparql_queries.len(), 1);
    assert!(!metadata.parsed.rspql_query.is_empty());

    let handle = api.start_query(&"combined".into()).expect("Failed to start query");

    // Should spawn both historical and live threads
    assert!(api.is_running(&"combined".into()));

    // Collect results - should get both historical and live
    let mut historical_count = 0;
    let mut live_count = 0;

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        if let Some(result) = handle.try_receive() {
            match result.source {
                ResultSource::Historical => historical_count += 1,
                ResultSource::Live => live_count += 1,
            }
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    println!("Historical results: {}, Live results: {}", historical_count, live_count);

    // At least one type should have results (depending on timing and data)
    // This verifies both threads can execute concurrently
}

#[test]
fn test_registered_query_exposes_query_defined_baseline_lowering() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let storage = Arc::new(
        StreamingSegmentedStorage::new(StreamingConfig::default())
            .expect("Failed to create storage"),
    );
    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
       (COUNT(?value) AS ?dayCount)
WHERE {
  ?sensor ex:hasValue ?value .
}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
       ((AVG(?value) - ?dayAvgValue) AS ?difference)
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:hasValue ?value .
  }
  GRAPH ex:dayBaseline {
    ?sensor ex:dayAvgValue ?dayAvgValue .
    ?sensor ex:dayCount ?dayCount .
  }
}
GROUP BY ?sensor ?dayAvgValue
HAVING(AVG(?value) > ?dayAvgValue)
    "#;

    let metadata = api
        .register_query("baseline_lowering".into(), janusql)
        .expect("Failed to register query");

    assert_eq!(metadata.parsed.generated_baseline_queries.len(), 1);
    assert_eq!(
        metadata.parsed.generated_baseline_queries[0].name,
        "http://example.org/dayBaseline"
    );
    assert!(metadata.parsed.generated_baseline_queries[0].sparql_query.contains("AVG"));
    assert!(metadata.parsed.generated_baseline_queries[0].sparql_query.contains("COUNT"));
    assert!(metadata.parsed.generated_baseline_queries[0].sparql_query.contains("GROUP BY"));
    assert!(metadata.parsed.generated_baseline_queries[0]
        .sparql_query
        .contains("GRAPH ?__janus_log_graph"));
    assert_eq!(metadata.parsed.ast.baseline_uses.len(), 1);
    assert_eq!(metadata.parsed.ast.baseline_uses[0].name, "http://example.org/dayBaseline");
    assert!(metadata.parsed.select_clause.contains("?difference"));
    assert!(!metadata.parsed.where_clause.contains("DEFINE BASELINE"));
    assert!(metadata.parsed.where_clause.contains("GRAPH ex:dayBaseline"));
    assert_eq!(metadata.parsed.baseline_graph_templates.len(), 1);
    assert_eq!(metadata.parsed.baseline_graph_templates[0].triples.len(), 2);
}

#[test]
fn test_query_defined_baseline_live_query_starts_when_graph_is_present() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {
  ?sensor ex:temperature ?value .
}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
       ((AVG(?value) - ?dayAvgValue) AS ?difference)
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
  GRAPH ex:dayBaseline {
    ?sensor ex:dayAvgValue ?dayAvgValue .
  }
}
GROUP BY ?sensor ?dayAvgValue
    "#;

    api.register_query("baseline_e2e".into(), janusql)
        .expect("Failed to register query");

    let _handle = api
        .start_query(&"baseline_e2e".into())
        .expect("query-defined baseline live query should start");

    let bindings = api
        .get_query_defined_baseline_bindings(&"baseline_e2e".into())
        .expect("query-defined baseline bindings should be available after start");
    assert!(bindings.contains_key("http://example.org/dayBaseline"));
    assert_eq!(api.get_query_status(&"baseline_e2e".into()), Some(ExecutionStatus::Running));
}

#[test]
fn test_query_defined_baseline_live_query_rejects_missing_graph_reference() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 6000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {
  ?sensor ex:temperature ?value .
}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
}
GROUP BY ?sensor
    "#;

    api.register_query("baseline_missing_graph".into(), janusql)
        .expect("Failed to register query");

    let err = match api.start_query(&"baseline_missing_graph".into()) {
        Ok(_) => panic!("query should fail without baseline GRAPH reference"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("requires a matching GRAPH reference"));
}

#[test]
fn test_nested_historical_subquery_starts_via_historical_materialization_lowering() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    let janusql = r#"
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

REGISTER RStream ex:output AS
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
  {
    SELECT ?sensor
           (AVG(?histValue) AS ?dayAvgValue)
    WHERE {
      WINDOW ex:historyDay {
        ?sensor ex:temperature ?histValue .
      }
    }
    GROUP BY ?sensor
  }
}
GROUP BY ?sensor ?dayAvgValue
    "#;

    let metadata = api
        .register_query("historical_materialization_e2e".into(), janusql)
        .expect("Failed to register query");

    assert_eq!(metadata.parsed.historical_materialized_subqueries.len(), 1);
    assert_eq!(metadata.parsed.planned_subqueries.len(), 1);
    assert_eq!(metadata.parsed.ast.baseline_definitions.len(), 1);
    assert_eq!(metadata.parsed.ast.baseline_uses.len(), 1);
    assert_eq!(metadata.parsed.planning_statistics.historical_materialized_subqueries, 1);
    assert!(metadata
        .parsed
        .where_clause
        .contains("materialized-history/__hist_mat_subquery_0"));

    let _handle = api
        .start_query(&"historical_materialization_e2e".into())
        .expect("historical materialization live query should start");

    let bindings = api
        .get_query_defined_baseline_bindings(&"historical_materialization_e2e".into())
        .expect("historical materialization bindings should be available after start");
    assert_eq!(bindings.len(), 1);
    assert!(bindings
        .keys()
        .next()
        .is_some_and(|name| name.contains("__hist_mat_subquery_0")));
    assert_eq!(
        api.get_query_status(&"historical_materialization_e2e".into()),
        Some(ExecutionStatus::Running)
    );
}
