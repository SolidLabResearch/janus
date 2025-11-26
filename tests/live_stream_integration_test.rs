use janus::core::RDFEvent;
use janus::stream::live_stream_processing::LiveStreamProcessing;
use std::thread;
use std::time::Duration;

#[test]
fn test_simple_window_query() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 5000 STEP 1000]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.start_processing().unwrap();

    // Add events
    for i in 0..10 {
        let event = RDFEvent::new(
            (i * 500) as u64,
            &format!("http://example.org/subject{}", i),
            "http://example.org/predicate",
            &format!("object{}", i),
            "",
        );
        processor.add_event("http://example.org/stream1", event).unwrap();
    }

    // Close stream to trigger final windows
    processor.close_stream("http://example.org/stream1", 10000).unwrap();

    // Wait for processing
    thread::sleep(Duration::from_millis(500));

    // Collect results
    let results = processor.collect_results(None).unwrap();

    assert!(!results.is_empty(), "Should receive results from window closures");
    println!("Received {} results", results.len());
}

#[test]
fn test_iot_sensor_streaming() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?sensor ?reading
        FROM NAMED WINDOW ex:sensorWindow ON STREAM ex:sensors [RANGE 2000 STEP 500]
        WHERE {
            WINDOW ex:sensorWindow { ?sensor ex:hasReading ?reading }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/sensors").unwrap();
    processor.start_processing().unwrap();

    // Simulate sensor readings
    let sensors = ["sensor1", "sensor2", "sensor3"];
    for i in 0..15 {
        let sensor = sensors[i % sensors.len()];
        let reading = 20 + (i % 10);

        let event = RDFEvent::new(
            (i * 200) as u64,
            &format!("http://example.org/{}", sensor),
            "http://example.org/hasReading",
            &format!("{}", reading),
            "",
        );

        processor.add_event("http://example.org/sensors", event).unwrap();
    }

    processor.close_stream("http://example.org/sensors", 5000).unwrap();
    thread::sleep(Duration::from_millis(500));

    let results = processor.collect_results(None).unwrap();
    assert!(!results.is_empty(), "Should receive sensor results");

    // Verify result structure
    for result in results.iter().take(3) {
        assert!(result.timestamp_from >= 0);
        assert!(result.timestamp_to > result.timestamp_from);
        assert!(result.bindings.contains("sensor"));
        assert!(result.bindings.contains("reading"));
    }
}

#[test]
fn test_multiple_streams_registration() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 200]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();

    // Register same stream multiple times (should be idempotent)
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();

    let registered = processor.get_registered_streams();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0], "http://example.org/stream1");
}

#[test]
fn test_window_timing() {
    // Test that windows close at correct intervals
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 300]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.start_processing().unwrap();

    // Add events at specific times
    let timestamps = [0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
    for (i, &ts) in timestamps.iter().enumerate() {
        let event = RDFEvent::new(
            ts,
            &format!("http://example.org/s{}", i),
            "http://example.org/p",
            &format!("o{}", i),
            "",
        );
        processor.add_event("http://example.org/stream1", event).unwrap();
    }

    processor.close_stream("http://example.org/stream1", 3000).unwrap();
    thread::sleep(Duration::from_millis(500));

    let results = processor.collect_results(None).unwrap();

    // With STEP=300, we should get windows closing at 300, 600, 900, 1200, etc.
    assert!(results.len() >= 3, "Should have at least 3 window closures");

    // Check timestamp ranges
    for result in &results {
        let range = result.timestamp_to - result.timestamp_from;
        assert_eq!(range, 1000, "Window range should be 1000ms");
    }
}

#[test]
fn test_empty_window() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 500]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.start_processing().unwrap();

    // Just close stream without adding events
    processor.close_stream("http://example.org/stream1", 2000).unwrap();
    thread::sleep(Duration::from_millis(300));

    let results = processor.collect_results(None).unwrap();

    // Empty windows may or may not emit results depending on implementation
    // This test just verifies it doesn't crash
    println!("Empty window test: {} results", results.len());
}

#[test]
fn test_processing_state_management() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT *
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 200]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();

    // Check initial state
    assert!(!processor.is_processing());

    // Start processing
    processor.start_processing().unwrap();
    assert!(processor.is_processing());

    // Try to start again (should fail)
    let result = processor.start_processing();
    assert!(result.is_err());
}

#[test]
fn test_unregistered_stream_error() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT *
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 200]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.start_processing().unwrap();

    // Try to add event to unregistered stream
    let event = RDFEvent::new(1000, "http://example.org/s", "http://example.org/p", "o", "");

    let result = processor.add_event("http://example.org/stream1", event);
    assert!(result.is_err());
}

#[test]
fn test_literal_and_uri_objects() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 2000 STEP 500]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.start_processing().unwrap();

    // Add event with URI object
    let event1 = RDFEvent::new(
        100,
        "http://example.org/alice",
        "http://example.org/knows",
        "http://example.org/bob",
        "",
    );
    processor.add_event("http://example.org/stream1", event1).unwrap();

    // Add event with literal object
    let event2 = RDFEvent::new(200, "http://example.org/alice", "http://example.org/age", "30", "");
    processor.add_event("http://example.org/stream1", event2).unwrap();

    processor.close_stream("http://example.org/stream1", 3000).unwrap();
    thread::sleep(Duration::from_millis(500));

    let results = processor.collect_results(None).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_rapid_event_stream() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.start_processing().unwrap();

    // Add 50 events rapidly
    for i in 0..50 {
        let event = RDFEvent::new(
            (i * 20) as u64, // Every 20ms
            &format!("http://example.org/s{}", i),
            "http://example.org/p",
            &format!("o{}", i),
            "",
        );
        processor.add_event("http://example.org/stream1", event).unwrap();
    }

    processor.close_stream("http://example.org/stream1", 2000).unwrap();
    thread::sleep(Duration::from_millis(500));

    let results = processor.collect_results(None).unwrap();

    // With STEP=100ms over 1000ms of data, should have ~10 window closures
    assert!(results.len() >= 5, "Should have multiple results from rapid stream");
}

#[test]
fn test_result_collection_methods() {
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 300]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();
    processor.start_processing().unwrap();

    for i in 0..10 {
        let event = RDFEvent::new(
            (i * 100) as u64,
            &format!("http://example.org/s{}", i),
            "http://example.org/p",
            &format!("o{}", i),
            "",
        );
        processor.add_event("http://example.org/stream1", event).unwrap();
    }

    processor.close_stream("http://example.org/stream1", 2000).unwrap();
    thread::sleep(Duration::from_millis(500));

    // Test try_receive
    let mut try_count = 0;
    while let Ok(Some(_)) = processor.try_receive_result() {
        try_count += 1;
        if try_count > 100 {
            break; // Safety limit
        }
    }

    assert!(try_count > 0, "try_receive should get some results");

    // Test collect_results with limit
    let limited = processor.collect_results(Some(2)).unwrap();
    assert!(limited.len() <= 2, "Should respect max_results limit");
}
