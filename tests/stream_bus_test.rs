//! Stream Bus Integration Tests
//!
//! These tests verify the stream bus functionality including:
//! - RDF line parsing (N-Triples/N-Quads format)
//! - File reading and event processing
//! - Storage integration
//! - Metrics tracking
//! - Rate limiting

use janus::parsing::rdf_parser;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::stream_bus::{BrokerType, StreamBus, StreamBusConfig};
use std::fs::{self, File};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

fn setup_test_environment(test_name: &str) -> std::io::Result<String> {
    let test_dir = format!("test_data_stream_bus_{}", test_name);
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir)?;
    fs::create_dir_all(format!("{}/storage", &test_dir))?;
    Ok(test_dir)
}

fn cleanup_test_environment(test_dir: &str) {
    let _ = fs::remove_dir_all(test_dir);
}

fn create_test_storage(test_dir: &str) -> std::io::Result<Arc<StreamingSegmentedStorage>> {
    let config = StreamingConfig {
        max_batch_events: 1000,
        max_batch_age_seconds: 1,
        max_batch_bytes: 1_000_000,
        sparse_interval: 100,
        entries_per_index_block: 10,
        segment_base_path: format!("{}/storage", test_dir),
    };

    let mut storage = StreamingSegmentedStorage::new(config)?;
    storage.start_background_flushing();
    Ok(Arc::new(storage))
}

fn create_test_rdf_file(path: &str, content: &str) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

#[test]
fn test_parse_ntriples_basic() {
    let test_dir = setup_test_environment("parse_ntriples_basic").unwrap();

    let config = StreamBusConfig {
        input_file: "test.nt".to_string(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, storage);

    let line = "<http://example.org/sensor1> <http://example.org/temperature> \"23.5\" <http://example.org/graph1> .";
    let event = rdf_parser::parse_rdf_line(line, true);

    assert!(event.is_ok());
    let event = event.unwrap();
    assert_eq!(event.subject, "http://example.org/sensor1");
    assert_eq!(event.predicate, "http://example.org/temperature");
    assert_eq!(event.object, "23.5");
    assert_eq!(event.graph, "http://example.org/graph1");

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_parse_ntriples_without_graph() {
    let test_dir = setup_test_environment("parse_ntriples_without_graph").unwrap();

    let config = StreamBusConfig {
        input_file: "test.nt".to_string(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, storage);

    let line = "<http://example.org/alice> <http://example.org/knows> <http://example.org/bob> .";
    let event = rdf_parser::parse_rdf_line(line, true);

    assert!(event.is_ok());
    let event = event.unwrap();
    assert_eq!(event.subject, "http://example.org/alice");
    assert_eq!(event.predicate, "http://example.org/knows");
    assert_eq!(event.object, "http://example.org/bob");
    assert_eq!(event.graph, "");

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_parse_invalid_rdf_line() {
    let test_dir = setup_test_environment("parse_invalid_rdf_line").unwrap();

    let config = StreamBusConfig {
        input_file: "test.nt".to_string(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, storage);

    let invalid_line = "<http://example.org/subject> <http://example.org/predicate>";
    let result = rdf_parser::parse_rdf_line(invalid_line, true);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid object format"));

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_storage_only_mode() {
    let test_dir = setup_test_environment("storage_only_mode").unwrap();

    let test_file = format!("{}/test_storage.nq", &test_dir);
    let rdf_data = r#"<http://example.org/sensor1> <http://example.org/temperature> "20.5" <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/temperature> "21.3" <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/temperature> "22.1" <http://example.org/graph1> .
"#;

    create_test_rdf_file(&test_file, rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();

    let bus = StreamBus::new(config, Arc::clone(&storage));
    let metrics = bus.start().unwrap();

    assert_eq!(metrics.events_read, 3);
    assert_eq!(metrics.events_stored, 3);
    assert_eq!(metrics.storage_errors, 0);

    std::thread::sleep(Duration::from_millis(100));

    let query_results = storage.query_rdf(0, u64::MAX).unwrap();
    assert_eq!(query_results.len(), 3);
    cleanup_test_environment(&test_dir);
}

#[test]
fn test_empty_lines_and_comments_skipped() {
    let test_dir = setup_test_environment("empty_lines_comments").unwrap();

    let test_file = format!("{}/test_comments.nq", &test_dir);
    let rdf_data = r#"# This is a comment
<http://example.org/sensor1> <http://example.org/temperature> "20.5" <http://example.org/graph1> .

# Another comment
<http://example.org/sensor2> <http://example.org/temperature> "21.3" <http://example.org/graph1> .

"#;

    create_test_rdf_file(&test_file, rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, Arc::clone(&storage));
    let metrics = bus.start().unwrap();

    assert_eq!(metrics.events_read, 2);
    assert_eq!(metrics.events_stored, 2);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_rate_limiting() {
    let test_dir = setup_test_environment("rate_limiting").unwrap();

    let test_file = format!("{}/test_rate.nq", &test_dir);
    let mut rdf_data = String::new();
    for i in 0..20 {
        rdf_data.push_str(&format!(
            "<http://example.org/sensor{}> <http://example.org/temperature> \"{}\" <http://example.org/graph1> .\n",
            i, 20 + i
        ));
    }

    create_test_rdf_file(&test_file, &rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 100,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, Arc::clone(&storage));

    let start = std::time::Instant::now();
    let metrics = bus.start().unwrap();
    let elapsed = start.elapsed();

    assert_eq!(metrics.events_read, 20);
    assert!(elapsed.as_millis() >= 150);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_metrics_calculation() {
    let metrics = janus::stream_bus::StreamBusMetrics {
        events_read: 100,
        events_published: 95,
        events_stored: 98,
        publish_errors: 5,
        storage_errors: 2,
        elapsed_seconds: 2.0,
    };

    assert_eq!(metrics.events_per_second(), 50.0);
    assert_eq!(metrics.publish_success_rate(), 95.0);
    assert_eq!(metrics.storage_success_rate(), 98.0);
}

#[test]
fn test_metrics_zero_events() {
    let metrics = janus::stream_bus::StreamBusMetrics {
        events_read: 0,
        events_published: 0,
        events_stored: 0,
        publish_errors: 0,
        storage_errors: 0,
        elapsed_seconds: 0.0,
    };

    assert_eq!(metrics.events_per_second(), 0.0);
    assert_eq!(metrics.publish_success_rate(), 0.0);
    assert_eq!(metrics.storage_success_rate(), 0.0);
}

#[test]
fn test_stop_signal() {
    let test_dir = setup_test_environment("stop_signal").unwrap();

    let test_file = format!("{}/test_stop.nq", &test_dir);
    let mut rdf_data = String::new();
    for i in 0..1000 {
        rdf_data.push_str(&format!(
            "<http://example.org/sensor{}> <http://example.org/temperature> \"{}\" <http://example.org/graph1> .\n",
            i, 20 + i
        ));
    }

    create_test_rdf_file(&test_file, &rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 50,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, Arc::clone(&storage));

    let handle = bus.start_async();

    std::thread::sleep(Duration::from_millis(100));
    bus.stop();

    let metrics = handle.join().unwrap().unwrap();

    assert!(metrics.events_read < 1000);
    assert!(metrics.events_read > 0);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_file_loop_mode() {
    let test_dir = setup_test_environment("file_loop_mode").unwrap();

    let test_file = format!("{}/test_loop.nq", &test_dir);
    let rdf_data = r#"<http://example.org/sensor1> <http://example.org/temperature> "20.5" <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/temperature> "21.3" <http://example.org/graph1> .
"#;

    create_test_rdf_file(&test_file, rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 100,
        loop_file: true,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, Arc::clone(&storage));

    let handle = bus.start_async();

    std::thread::sleep(Duration::from_millis(100));
    bus.stop();

    let metrics = handle.join().unwrap().unwrap();

    assert!(metrics.events_read > 2);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_timestamp_parsing() {
    let test_dir = setup_test_environment("timestamp_parsing").unwrap();

    let config_with_timestamps = StreamBusConfig {
        input_file: "test.nq".to_string(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus_with_ts = StreamBus::new(config_with_timestamps, Arc::clone(&storage));

    let line = "<http://example.org/sensor1> <http://example.org/temperature> \"23.5\" <http://example.org/graph1> .";
    let event = rdf_parser::parse_rdf_line(line, true).unwrap();
    assert!(event.timestamp > 0);

    let config_without_timestamps = StreamBusConfig {
        input_file: "test.nq".to_string(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: false,
        kafka_config: None,
        mqtt_config: None,
    };

    let bus_without_ts = StreamBus::new(config_without_timestamps, storage);

    let line_with_ts = "1234567890 <http://example.org/sensor1> <http://example.org/ts> \"value\" <http://example.org/graph1> .";
    let event = rdf_parser::parse_rdf_line(line_with_ts, false).unwrap();
    assert_eq!(event.timestamp, 1234567890);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_kafka_config_default() {
    use janus::stream_bus::KafkaConfig;

    let config = KafkaConfig::default();
    assert_eq!(config.bootstrap_servers, "localhost:9092");
    assert_eq!(config.client_id, "janus_stream_bus");
    assert_eq!(config.message_timeout_ms, "5000");
}

#[test]
fn test_mqtt_config_default() {
    use janus::stream_bus::MqttConfig;

    let config = MqttConfig::default();
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 1883);
    assert_eq!(config.client_id, "janus_stream_bus");
    assert_eq!(config.keep_alive_secs, 30);
}

#[test]
fn test_broker_type_variants() {
    use janus::stream_bus::BrokerType;

    let kafka = BrokerType::Kafka;
    let mqtt = BrokerType::Mqtt;
    let none = BrokerType::None;

    assert!(matches!(kafka, BrokerType::Kafka));
    assert!(matches!(mqtt, BrokerType::Mqtt));
    assert!(matches!(none, BrokerType::None));
}

#[test]
fn test_error_display() {
    use janus::stream_bus::StreamBusError;

    let file_error = StreamBusError::FileError("test error".to_string());
    assert_eq!(format!("{}", file_error), "File Error: test error");

    let broker_error = StreamBusError::BrokerError("connection failed".to_string());
    assert_eq!(format!("{}", broker_error), "Broker Error: connection failed");

    let config_error = StreamBusError::ConfigError("missing config".to_string());
    assert_eq!(format!("{}", config_error), "Config Error: missing config");
}

#[test]
fn test_malformed_rdf_lines_handling() {
    let test_dir = setup_test_environment("malformed_rdf_lines").unwrap();

    let test_file = format!("{}/test_malformed.nq", &test_dir);
    let rdf_data = "<http://example.org/sensor1> <http://example.org/temperature> \"20.5\" <http://example.org/graph1> .\nthis is not valid rdf\n<http://example.org/sensor2> <http://example.org/temperature> \"21.3\" <http://example.org/graph1> .\n<incomplete line\n<http://example.org/sensor3> <http://example.org/temperature> \"22.1\" <http://example.org/graph1> .\n<http://example.org/sensor4> <http://example.org/temperature> \"23.7\" <http://example.org/graph1> .";

    create_test_rdf_file(&test_file, rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();
    let bus = StreamBus::new(config, Arc::clone(&storage));
    let metrics = bus.start().unwrap();

    assert_eq!(metrics.events_read, 4);
    assert_eq!(metrics.events_stored, 4);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_large_file_processing() {
    let test_dir = setup_test_environment("large_file_processing").unwrap();

    let test_file = format!("{}/test_large.nq", &test_dir);
    let mut rdf_data = String::new();

    for i in 0..500 {
        rdf_data.push_str(&format!(
            "<http://example.org/sensor{}> <http://example.org/temperature> \"{}\" <http://example.org/graph1> .\n",
            i, 20.0 + (i as f64 * 0.1)
        ));
    }

    create_test_rdf_file(&test_file, &rdf_data).unwrap();

    let config = StreamBusConfig {
        input_file: test_file.clone(),
        broker_type: BrokerType::None,
        topics: vec![],
        rate_of_publishing: 0,
        loop_file: false,
        add_timestamps: true,
        kafka_config: None,
        mqtt_config: None,
    };

    let storage = create_test_storage(&test_dir).unwrap();

    let bus = StreamBus::new(config, Arc::clone(&storage));
    let metrics = bus.start().unwrap();

    assert_eq!(metrics.events_read, 500);
    assert_eq!(metrics.events_stored, 500);
    assert_eq!(metrics.storage_errors, 0);

    std::thread::sleep(Duration::from_secs(2));

    let query_results = storage.query_rdf(0, u64::MAX).unwrap();
    assert_eq!(query_results.len(), 500);
    cleanup_test_environment(&test_dir);
}
