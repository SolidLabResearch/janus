use janus::parsing::janusql_parser::{WindowDefinition, WindowType};
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::stream::operators::historical_fixed_window::HistoricalFixedWindowOperator;
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
fn test_historical_fixed_window_with_real_iris() {
    let test_dir = "/tmp/janus_test_fixed_window_iris";
    let _ = fs::remove_dir_all(test_dir);

    let config = create_test_config(test_dir);
    let storage = Arc::new(StreamingSegmentedStorage::new(config).unwrap());

    // Write events with real RDF IRIs representing IoT device events
    let devices = [
        "http://example.org/device/thermostat1",
        "http://example.org/device/thermostat2",
        "http://example.org/device/thermostat3",
    ];

    for i in 1u64..=6 {
        let ts = i * 100;
        let device = devices[((i - 1) % 3) as usize];
        let temp = format!("http://example.org/temperature/{}", 18 + i);

        // Device type
        storage
            .write_rdf(
                ts,
                device,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://example.org/Thermostat",
                "http://example.org/graph/devices",
            )
            .unwrap();

        // Temperature reading
        storage
            .write_rdf(
                ts,
                device,
                "http://example.org/hasTemperature",
                &temp,
                "http://example.org/graph/readings",
            )
            .unwrap();

        // Location
        storage
            .write_rdf(
                ts,
                device,
                "http://example.org/locatedIn",
                &format!("http://example.org/room/{}", (i % 3) + 1),
                "http://example.org/graph/locations",
            )
            .unwrap();
    }

    // Define Fixed Window: [200, 500]
    let window_def = WindowDefinition {
        window_name: "http://example.org/window/temp-fixed".to_string(),
        stream_name: "http://example.org/stream/temperature".to_string(),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(200),
        end: Some(500),
        window_type: WindowType::HistoricalFixed,
    };

    let mut operator = HistoricalFixedWindowOperator::new(storage.clone(), window_def);

    // Should yield once with events in [200, 500]
    let w1 = operator.next().unwrap();

    // We wrote 3 triples per timestamp, and timestamps 200, 300, 400, 500 are in range
    // So we expect 4 timestamps * 3 triples = 12 events
    assert_eq!(w1.len(), 12);

    // Verify timestamps are in range
    assert_eq!(w1[0].timestamp, 200);
    assert_eq!(w1[w1.len() - 1].timestamp, 500);

    // Should not yield again
    assert!(operator.next().is_none());

    let _ = fs::remove_dir_all(test_dir);
}

#[test]
fn test_historical_fixed_window_semantic_web() {
    let test_dir = "/tmp/janus_test_fixed_window_semantic";
    let _ = fs::remove_dir_all(test_dir);

    let config = create_test_config(test_dir);
    let storage = Arc::new(StreamingSegmentedStorage::new(config).unwrap());

    // Semantic web example: Publications and authors
    let publications = [
        ("http://example.org/publication/paper1", "Semantic Streams"),
        ("http://example.org/publication/paper2", "RDF Processing"),
        ("http://example.org/publication/paper3", "Knowledge Graphs"),
    ];

    let authors = [
        "http://example.org/author/smith",
        "http://example.org/author/jones",
        "http://example.org/author/brown",
    ];

    for i in 1u64..=6 {
        let ts = i * 100;
        let (pub_iri, title) = publications[((i - 1) % 3) as usize];
        let author = authors[((i - 1) % 3) as usize];

        // Publication title
        storage
            .write_rdf(
                ts,
                pub_iri,
                "http://purl.org/dc/terms/title",
                &format!("http://example.org/literal/{}", title.replace(" ", "_")),
                "http://example.org/graph/publications",
            )
            .unwrap();

        // Publication author
        storage
            .write_rdf(
                ts,
                pub_iri,
                "http://purl.org/dc/terms/creator",
                author,
                "http://example.org/graph/publications",
            )
            .unwrap();

        // Publication type
        storage
            .write_rdf(
                ts,
                pub_iri,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "http://purl.org/ontology/bibo/AcademicArticle",
                "http://example.org/graph/publications",
            )
            .unwrap();
    }

    // Query publications from timestamp 150 to 450
    let window_def = WindowDefinition {
        window_name: "http://example.org/window/publications-fixed".to_string(),
        stream_name: "http://example.org/stream/publications".to_string(),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(150),
        end: Some(450),
        window_type: WindowType::HistoricalFixed,
    };

    let mut operator = HistoricalFixedWindowOperator::new(storage.clone(), window_def);

    let w1 = operator.next().unwrap();

    // Timestamps 200, 300, 400 are in range [150, 450]
    // 3 timestamps * 3 triples = 9 events
    assert_eq!(w1.len(), 9);

    // Verify all events are within the time range
    for event in &w1 {
        assert!(event.timestamp >= 150 && event.timestamp <= 450);
    }

    assert!(operator.next().is_none());

    let _ = fs::remove_dir_all(test_dir);
}
