//! Integration tests for Janus
//!
//! These tests verify the overall functionality of the Janus engine
//! by testing the integration of multiple components together.

use janus::api::janus_api::{JanusApi, ResultSource};
use janus::parsing::janusql_parser::JanusQLParser;
use janus::registry::query_registry::QueryRegistry;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::{Error, Result};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_basic_functionality() {
    let temp_dir = TempDir::new().expect("failed to create temporary directory");
    let mut storage = StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
        max_batch_events: 10,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1024 * 1024,
        sparse_interval: 10,
        entries_per_index_block: 100,
    })
    .expect("failed to create storage");
    storage.start_background_flushing();

    storage
        .write_rdf(
            1_000,
            "http://example.org/sensor1",
            "http://example.org/temperature",
            "21",
            "http://example.org/sensors",
        )
        .expect("failed to write first event");
    storage
        .write_rdf(
            1_500,
            "http://example.org/sensor2",
            "http://example.org/temperature",
            "22",
            "http://example.org/sensors",
        )
        .expect("failed to write second event");
    storage.flush().expect("failed to flush storage");

    let api = JanusApi::new(
        JanusQLParser::new().expect("failed to create parser"),
        Arc::new(QueryRegistry::new()),
        Arc::new(storage),
    )
    .expect("failed to create JanusApi");

    let janusql = r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor ?temp

        FROM NAMED WINDOW ex:hist ON STREAM ex:sensors [START 1000 END 2000]

        WHERE {
            WINDOW ex:hist { ?sensor ex:temperature ?temp }
        }
    "#;

    api.register_query("smoke_test".into(), janusql)
        .expect("failed to register query");
    let handle = api.start_query(&"smoke_test".into()).expect("failed to start query");

    let mut result = None;
    for _ in 0..20 {
        if let Some(next_result) = handle.try_receive() {
            result = Some(next_result);
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let result = result.expect("expected a historical query result");
    assert_eq!(result.query_id, "smoke_test");
    assert!(matches!(result.source, ResultSource::Historical));
    assert!(!result.bindings.is_empty(), "expected at least one binding");
}

#[test]
fn test_error_types() {
    let config_error = Error::Config("test".to_string());
    assert!(format!("{}", config_error).contains("Configuration error"));

    let store_error = Error::Store("test".to_string());
    assert!(format!("{}", store_error).contains("Store error"));

    let stream_error = Error::Stream("test".to_string());
    assert!(format!("{}", stream_error).contains("Stream error"));

    let query_error = Error::Query("test".to_string());
    assert!(format!("{}", query_error).contains("Query error"));
}

#[test]
fn test_result_type() {
    fn returns_ok() -> Result<i32> {
        Ok(42)
    }

    fn returns_err() -> Result<i32> {
        Err(Error::Other("test error".to_string()))
    }

    assert!(returns_ok().is_ok());
    assert!(returns_err().is_err());
}
