use janus::parsing::janusql_parser::{WindowDefinition, WindowType};
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::stream::operators::historical_sliding_window::HistoricalSlidingWindowOperator;
use std::fs;
use std::sync::Arc;

fn create_test_config(path: &str) -> StreamingConfig {
    StreamingConfig {
        segment_base_path: path.to_string(),
        max_batch_events: 10,
        max_batch_bytes: 1024,
        max_batch_age_seconds: 1,
        sparse_interval: 2,
        entries_per_index_block: 2,
    }
}

#[test]
fn test_historical_sliding_window_with_real_iris() {
    let test_dir = "/tmp/janus_test_sliding_window_iris";
    let _ = fs::remove_dir_all(test_dir);

    let config = create_test_config(test_dir);
    let storage = Arc::new(StreamingSegmentedStorage::new(config).unwrap());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Write events with real RDF IRIs
    // Simulating sensor data: temperature readings from different sensors
    let sensors = [
        "http://example.org/sensor/temp1",
        "http://example.org/sensor/temp2",
        "http://example.org/sensor/temp3",
    ];

    for i in 0..6 {
        let ts = now - (500 - (i * 100));
        let sensor = sensors[(i % 3) as usize];
        let temp_value = format!("http://example.org/value/{}", 20 + i);

        storage
            .write_rdf(
                ts,
                sensor,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://example.org/TemperatureSensor",
                "http://example.org/graph/sensors",
            )
            .unwrap();

        storage
            .write_rdf(
                ts,
                sensor,
                "http://example.org/hasValue",
                &temp_value,
                "http://example.org/graph/readings",
            )
            .unwrap();
    }

    // Define Window: Width 200, Slide 100, Offset 500
    let window_def = WindowDefinition {
        window_name: "http://example.org/window/temp-sliding".to_string(),
        stream_name: "http://example.org/stream/temperature".to_string(),
        width: 200,
        slide: 100,
        offset: Some(500),
        start: None,
        end: None,
        window_type: WindowType::HistoricalSliding,
    };

    let mut operator = HistoricalSlidingWindowOperator::new(storage.clone(), window_def);

    // Window 1: [now-500, now-300]
    let w1 = operator.next().unwrap();
    assert!(w1.len() >= 2); // At least 2 events (type + value for first sensor)

    // Verify we got RDF events with proper IRIs
    let first_event = &w1[0];
    assert_eq!(first_event.timestamp, now - 500);

    // Window 2: [now-400, now-200]
    let w2 = operator.next().unwrap();
    assert!(w2.len() >= 2);

    // Window 3: [now-300, now-100]
    let w3 = operator.next().unwrap();
    assert!(w3.len() >= 2);

    let _ = fs::remove_dir_all(test_dir);
}

#[test]
fn test_historical_sliding_window_foaf_example() {
    let test_dir = "/tmp/janus_test_sliding_window_foaf";
    let _ = fs::remove_dir_all(test_dir);

    let config = create_test_config(test_dir);
    let storage = Arc::new(StreamingSegmentedStorage::new(config).unwrap());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // FOAF (Friend of a Friend) example
    let people = [
        ("http://example.org/person/alice", "Alice"),
        ("http://example.org/person/bob", "Bob"),
        ("http://example.org/person/charlie", "Charlie"),
    ];

    for i in 0..6 {
        let ts = now - (600 - (i * 100));
        let (person_iri, name) = people[(i % 3) as usize];

        // Person name
        storage
            .write_rdf(
                ts,
                person_iri,
                "http://xmlns.com/foaf/0.1/name",
                &format!("http://example.org/literal/{}", name),
                "http://example.org/graph/people",
            )
            .unwrap();

        // Person type
        storage
            .write_rdf(
                ts,
                person_iri,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://xmlns.com/foaf/0.1/Person",
                "http://example.org/graph/people",
            )
            .unwrap();
    }

    let window_def = WindowDefinition {
        window_name: "http://example.org/window/people-sliding".to_string(),
        stream_name: "http://example.org/stream/people".to_string(),
        width: 250,
        slide: 100,
        offset: Some(600),
        start: None,
        end: None,
        window_type: WindowType::HistoricalSliding,
    };

    let mut operator = HistoricalSlidingWindowOperator::new(storage.clone(), window_def);

    // First window should have data
    let w1 = operator.next().unwrap();
    assert!(!w1.is_empty(), "First window should contain events");

    // Second window should have data
    let w2 = operator.next().unwrap();
    assert!(!w2.is_empty(), "Second window should contain events");

    let _ = fs::remove_dir_all(test_dir);
}
