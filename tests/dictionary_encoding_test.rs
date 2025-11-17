//! Dictionary Encoding Integration Tests
//!
//! These tests verify the dictionary-based encoding system for RDF terms.
//!
//! **Important**: The dictionary stores the actual URI/literal strings WITHOUT RDF syntax:
//! - URIs: stored as "https://example.org/resource" (not "<https://example.org/resource>")
//! - Literals: stored as the value string (e.g., "23.5" or "2025-11-05T10:30:00Z")
//! - Datatypes: stored separately as URIs (e.g., "http://www.w3.org/2001/XMLSchema#double")
//!
//! The RDF syntax (angle brackets, quotes, ^^datatype) is handled by the RDF parser/serializer,
//! not by the dictionary encoding layer. This keeps the dictionary implementation clean and
//! format-agnostic.
//!
//! Example RDF triple in Turtle syntax:
//! ```turtle
//! <https://rsp.js/event1> <http://www.w3.org/ns/saref#hasValue> "23.5"^^<http://www.w3.org/2001/XMLSchema#double> .
//! ```
//!
//! Is stored in the dictionary as 4 separate entries:
//! - Subject ID → "https://rsp.js/event1"
//! - Predicate ID → "http://www.w3.org/ns/saref#hasValue"
//! - Object ID → "23.5" (the literal value)
//! - Datatype ID → "http://www.w3.org/2001/XMLSchema#double" (if needed)

use janus::core::encoding::{decode_record, encode_record, RECORD_SIZE};
use janus::storage::indexing::dictionary::Dictionary;
use janus::storage::indexing::sparse::{build_sparse_index, SparseReader};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Simple log writer for test purposes (legacy indexing module was deleted)
struct LogWriter {
    log_file: File,
    record_count: u64,
}

impl LogWriter {
    fn create(path: &str) -> std::io::Result<Self> {
        let log_file = File::create(path)?;
        Ok(Self { log_file, record_count: 0 })
    }

    fn append_record(
        &mut self,
        timestamp: u64,
        subject: u32,
        predicate: u32,
        object: u32,
        graph: u32,
    ) -> std::io::Result<()> {
        let mut buffer = [0u8; RECORD_SIZE];
        encode_record(&mut buffer, timestamp, subject, predicate, object, graph);
        self.log_file.write_all(&buffer)?;
        self.record_count += 1;
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.log_file.flush()
    }
}

#[test]
fn test_rdf_syntax_to_dictionary_mapping() {
    let mut dict = Dictionary::new();

    // RDF Triple in Turtle syntax:
    // <https://rsp.js/event1> <http://www.w3.org/ns/saref#hasValue> "23.5"^^<http://www.w3.org/2001/XMLSchema#double> <https://example.org/graph> .
    //
    // The parser would extract these components and store them WITHOUT RDF syntax:

    // Subject: <https://rsp.js/event1> → stored as the URI string
    let subject = "https://rsp.js/event1";
    let subject_id = dict.encode(subject);

    // Predicate: <http://www.w3.org/ns/saref#hasValue> → stored as the URI string
    let predicate = "http://www.w3.org/ns/saref#hasValue";
    let predicate_id = dict.encode(predicate);

    // Object: "23.5"^^xsd:double → stored as the literal value "23.5"
    let object = "23.5";
    let object_id = dict.encode(object);

    // Datatype: ^^<http://www.w3.org/2001/XMLSchema#double> → stored as URI string
    let datatype = "http://www.w3.org/2001/XMLSchema#double";
    let datatype_id = dict.encode(datatype);

    // Graph: <https://example.org/graph> → stored as the URI string
    let graph = "https://example.org/graph";
    let graph_id = dict.encode(graph);

    // Verify all components are stored correctly
    assert_eq!(dict.decode(subject_id), Some(subject));
    assert_eq!(dict.decode(predicate_id), Some(predicate));
    assert_eq!(dict.decode(object_id), Some(object));
    assert_eq!(dict.decode(datatype_id), Some(datatype));
    assert_eq!(dict.decode(graph_id), Some(graph));

    // In a real system, you'd also store metadata about which IDs are literals vs URIs
    // and what datatype each literal has. This test just demonstrates the string storage.
}

#[test]
fn test_rdf_literal_datatypes() {
    let mut dict = Dictionary::new();

    // Example RDF triples with different literal types:
    //
    // Triple 1: <event1> <hasTimestamp> "2025-11-05T10:30:00Z"^^xsd:dateTime
    let timestamp_value = "2025-11-05T10:30:00Z";
    let timestamp_datatype = "http://www.w3.org/2001/XMLSchema#dateTime";

    // Triple 2: <event1> <hasTemperature> "23.5"^^xsd:double
    let temp_value = "23.5";
    let temp_datatype = "http://www.w3.org/2001/XMLSchema#double";

    // Triple 3: <event1> <hasCount> "42"^^xsd:integer
    let count_value = "42";
    let count_datatype = "http://www.w3.org/2001/XMLSchema#integer";

    // Triple 4: <event1> <hasLabel> "Sensor Reading"^^xsd:string
    let label_value = "Sensor Reading";
    let label_datatype = "http://www.w3.org/2001/XMLSchema#string";

    // Store all values and datatypes in dictionary
    let timestamp_val_id = dict.encode(timestamp_value);
    let timestamp_dt_id = dict.encode(timestamp_datatype);

    let temp_val_id = dict.encode(temp_value);
    let temp_dt_id = dict.encode(temp_datatype);

    let count_val_id = dict.encode(count_value);
    let count_dt_id = dict.encode(count_datatype);

    let label_val_id = dict.encode(label_value);
    let label_dt_id = dict.encode(label_datatype);

    // Verify all are stored correctly
    assert_eq!(dict.decode(timestamp_val_id), Some(timestamp_value));
    assert_eq!(dict.decode(timestamp_dt_id), Some(timestamp_datatype));

    assert_eq!(dict.decode(temp_val_id), Some(temp_value));
    assert_eq!(dict.decode(temp_dt_id), Some(temp_datatype));

    assert_eq!(dict.decode(count_val_id), Some(count_value));
    assert_eq!(dict.decode(count_dt_id), Some(count_datatype));

    assert_eq!(dict.decode(label_val_id), Some(label_value));
    assert_eq!(dict.decode(label_dt_id), Some(label_datatype));

    // Note: Datatype URIs are reused across multiple literals
    // E.g., many literals will have ^^xsd:double as their datatype
    assert_eq!(temp_dt_id, dict.encode(temp_datatype)); // Same ID when requested again
}

#[test]
fn test_dictionary_basic_operations() {
    let mut dict = Dictionary::new();

    // Test get_or_insert with real RDF URIs
    let uri1 = "https://rsp.js/event1";
    let uri2 = "http://www.w3.org/ns/saref#hasTimestamp";
    let uri3 = "http://example.org/sensor/temperature";
    let uri4 = "http://www.w3.org/ns/ssn#observedBy";

    // First insertion should return ID 0
    let id1 = dict.encode(uri1);
    assert_eq!(id1, 0);

    // Subsequent insertions should return sequential IDs
    let id2 = dict.encode(uri2);
    assert_eq!(id2, 1);

    let id3 = dict.encode(uri3);
    assert_eq!(id3, 2);

    let id4 = dict.encode(uri4);
    assert_eq!(id4, 3);

    // Requesting same URI should return same ID
    let id1_again = dict.encode(uri1);
    assert_eq!(id1_again, id1);

    // Test retrieval
    assert_eq!(dict.decode(id1), Some(uri1));
    assert_eq!(dict.decode(id2), Some(uri2));
    assert_eq!(dict.decode(id3), Some(uri3));
    assert_eq!(dict.decode(id4), Some(uri4));

    // Test invalid ID
    assert_eq!(dict.decode(999), None);

    // Test length
    assert_eq!(dict.id_to_uri.len(), 4);
    assert!(!dict.id_to_uri.is_empty());
}

#[test]
fn test_dictionary_persistence() -> std::io::Result<()> {
    let test_dir = "target/test_data/dict_persistence";
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir)?;

    let dict_path = Path::new(test_dir).join("test_dict.bin");

    // Create and populate dictionary
    let mut dict = Dictionary::new();
    let uris = [
        "https://example.org/resource/event001",
        "http://www.w3.org/ns/saref#hasValue",
        "http://www.w3.org/2001/XMLSchema#dateTime",
        "https://solid.ti.rw.fau.de/public/ns/stream#",
    ];

    let ids: Vec<u32> = uris.iter().map(|uri| dict.encode(uri)).collect();

    // Save to file
    dict.save_to_file(&dict_path)?;

    // Load from file
    let loaded_dict = Dictionary::load_from_file(&dict_path)?;

    // Verify all URIs are preserved with correct IDs
    for (i, uri) in uris.iter().enumerate() {
        assert_eq!(loaded_dict.decode(ids[i]), Some(*uri));
    }

    assert_eq!(loaded_dict.id_to_uri.len(), uris.len());

    Ok(())
}

#[test]
fn test_rdf_event_encoding_with_dictionary() {
    let mut dict = Dictionary::new();

    // RDF Quad in N-Quads syntax would look like:
    // <https://rsp.js/event/sensor-reading-001> <http://www.w3.org/ns/saref#hasTimestamp> "2025-11-05T10:30:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <https://solid.ti.rw.fau.de/public/ns/stream#default> .
    //
    // But we store the actual string values WITHOUT syntax markers:

    let subject_uri = "https://rsp.js/event/sensor-reading-001";
    let predicate_uri = "http://www.w3.org/ns/saref#hasTimestamp";
    let object_uri = "2025-11-05T10:30:00Z"; // The literal value itself
    let graph_uri = "https://solid.ti.rw.fau.de/public/ns/stream#default";

    // Map URIs to IDs
    let timestamp: u64 = 1699181400;
    let subject_id = dict.encode(subject_uri);
    let predicate_id = dict.encode(predicate_uri);
    let object_id = dict.encode(object_uri);
    let graph_id = dict.encode(graph_uri);

    // Encode record with IDs
    let mut buffer = [0u8; RECORD_SIZE];
    encode_record(&mut buffer, timestamp, subject_id, predicate_id, object_id, graph_id);

    // Decode record
    let (dec_timestamp, dec_subject, dec_predicate, dec_object, dec_graph) = decode_record(&buffer);

    // Verify IDs are correctly encoded/decoded
    assert_eq!(dec_timestamp, timestamp);
    assert_eq!(dec_subject, subject_id);
    assert_eq!(dec_predicate, predicate_id);
    assert_eq!(dec_object, object_id);
    assert_eq!(dec_graph, graph_id);

    // Resolve IDs back to URIs
    assert_eq!(dict.decode(dec_subject), Some(subject_uri));
    assert_eq!(dict.decode(dec_predicate), Some(predicate_uri));
    assert_eq!(dict.decode(dec_object), Some(object_uri));
    assert_eq!(dict.decode(dec_graph), Some(graph_uri));
}

#[test]
fn test_iot_sensor_events_with_dictionary() -> std::io::Result<()> {
    let test_dir = "target/test_data/iot_sensor";
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir)?;

    let log_path = format!("{}/iot_sensor.log", test_dir);
    let mut dict = Dictionary::new();

    // Define common IoT RDF predicates and graph URIs
    let predicates = [
        "http://www.w3.org/ns/saref#hasTimestamp",
        "http://www.w3.org/ns/saref#hasValue",
        "http://www.w3.org/ns/ssn#observedBy",
        "http://www.w3.org/ns/sosa#observedProperty",
    ];

    // Map predicates to IDs first (these will be reused)
    let predicate_ids: Vec<u32> = predicates.iter().map(|p| dict.encode(p)).collect();

    let graph_uri = "https://solid.ti.rw.fau.de/public/ns/stream#iot";
    let graph_id = dict.encode(graph_uri);

    // Create log writer
    let mut writer = LogWriter::create(&log_path)?;

    // Generate 100 IoT sensor events with unique event IDs but shared predicates
    for i in 0..100 {
        let timestamp = 1699181400 + i;

        // Each event has unique subject (sensor reading ID)
        let subject_uri = format!("https://rsp.js/event/sensor-reading-{:03}", i);
        let subject_id = dict.encode(&subject_uri);

        // Rotate through predicates (demonstrating reuse)
        let predicate_id = predicate_ids[(i % predicate_ids.len() as u64) as usize];

        // Unique object (sensor value)
        let object_uri = format!("value-{}", i * 10);
        let object_id = dict.encode(&object_uri);

        writer.append_record(timestamp, subject_id, predicate_id, object_id, graph_id)?;
    }

    writer.flush()?;

    // Verify dictionary statistics
    // We should have:
    // - 100 unique subjects
    // - 4 predicates (reused)
    // - 100 unique objects
    // - 1 graph URI
    // Total: 205 unique URIs
    assert_eq!(dict.id_to_uri.len(), 205);

    // Verify predicate reuse - predicates should have low IDs (0-3)
    for (i, pred) in predicates.iter().enumerate() {
        assert_eq!(dict.encode(pred), i as u32);
    }

    Ok(())
}

#[test]
fn test_sparse_index_with_dictionary_integration() -> std::io::Result<()> {
    let test_dir = "target/test_data/sparse_integration";
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir)?;

    let log_path = format!("{}/indexed_sensor.log", test_dir);
    let index_path = format!("{}/indexed_sensor.idx", test_dir);
    let dict_path = format!("{}/indexed_sensor_dict.bin", test_dir);

    let mut dict = Dictionary::new();

    // Define RDF components
    let predicates =
        ["http://www.w3.org/ns/saref#hasTimestamp", "http://www.w3.org/ns/saref#hasValue"];

    let predicate_ids: Vec<u32> = predicates.iter().map(|p| dict.encode(p)).collect();

    let graph_uri = "https://example.org/graph/sensors";
    let graph_id = dict.encode(graph_uri);

    // Create log with 1000 events
    let mut writer = LogWriter::create(&log_path)?;

    for i in 0..1000 {
        let timestamp = i;
        let subject_uri = format!("https://rsp.js/event/{:04}", i);
        let subject_id = dict.encode(&subject_uri);
        let predicate_id = predicate_ids[(i % 2) as usize];
        let object_uri = format!("reading-{}", i);
        let object_id = dict.encode(&object_uri);

        writer.append_record(timestamp, subject_id, predicate_id, object_id, graph_id)?;
    }

    writer.flush()?;

    // Save dictionary BEFORE building index
    dict.save_to_file(Path::new(&dict_path))?;

    // Build sparse index (without dictionary parameter since we saved it separately)
    build_sparse_index(&log_path, &index_path, &100)?;

    // Load dictionary and reader
    let (reader, loaded_dict) = SparseReader::open_with_dictionary(&index_path, &dict_path, 100)?;

    // Query a range and verify results
    let results = reader.query_resolved(&log_path, &loaded_dict, 100, 199)?;

    // Should get 100 events (timestamps 100-199)
    assert_eq!(results.len(), 100);

    // Verify first result has resolved URIs
    assert!(results[0].subject.starts_with("https://rsp.js/event/"));
    assert!(results[0].predicate.starts_with("http://www.w3.org/ns/saref#"));
    assert!(results[0].object.starts_with("reading-"));
    assert_eq!(results[0].graph, graph_uri);

    // Verify timestamps are in order
    for (i, event) in results.iter().enumerate() {
        assert_eq!(event.timestamp, 100 + i as u64);
    }

    Ok(())
}

#[test]
fn test_large_uri_handling() {
    let mut dict = Dictionary::new();

    // Test with very long URIs (realistic for RDF)
    let long_uri = format!(
        "https://solid.ti.rw.fau.de/public/2025/11/05/sensors/building-3/floor-2/room-205/temperature-sensor-{}/reading-{}",
        "TMP-4532-XYZ-9871-ABC-DEF",
        "measurement-with-very-long-identifier-12345678901234567890"
    );

    let id = dict.encode(&long_uri);
    assert_eq!(id, 0);

    // Verify retrieval works
    assert_eq!(dict.decode(id), Some(long_uri.as_str()));

    // Test that we can handle many long URIs
    for i in 0..100 {
        let uri = format!(
            "https://example.org/very/long/path/to/resource/{}/subresource/{}/final-resource-{}",
            i,
            i * 2,
            i * 3
        );
        dict.encode(&uri);
    }

    assert_eq!(dict.id_to_uri.len(), 101);
}

#[test]
fn test_rdf_namespace_reuse() {
    let mut dict = Dictionary::new();

    // Common RDF namespace URIs that should be reused
    let common_namespaces = [
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        "http://www.w3.org/2000/01/rdf-schema#",
        "http://www.w3.org/2001/XMLSchema#",
        "http://www.w3.org/ns/saref#",
        "http://www.w3.org/ns/ssn#",
        "http://www.w3.org/ns/sosa#",
    ];

    // Map each namespace
    let namespace_ids: Vec<u32> = common_namespaces.iter().map(|ns| dict.encode(ns)).collect();

    // Create 1000 events that all use these namespaces
    for i in 0..1000 {
        let event_uri = format!("https://rsp.js/event/{}", i);
        dict.encode(&event_uri);

        // Reference one of the common namespaces
        let ns_id = namespace_ids[i % namespace_ids.len()];
        assert!(dict.decode(ns_id).is_some());
    }

    // Dictionary should have: 6 namespaces + 1000 events = 1006 entries
    assert_eq!(dict.id_to_uri.len(), 1006);

    // Verify namespace IDs are unchanged (demonstrating reuse)
    for (i, ns) in common_namespaces.iter().enumerate() {
        assert_eq!(dict.encode(ns), namespace_ids[i]);
    }
}

#[test]
fn test_event_resolution_workflow() -> std::io::Result<()> {
    let test_dir = "target/test_data/event_resolution";
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir)?;

    let log_path = format!("{}/resolution_test.log", test_dir);
    let mut dict = Dictionary::new();

    // Create realistic RDF event
    let event_uris = vec![
        (
            1699181400u64,
            "https://rsp.js/event/temp-reading-001",
            "http://www.w3.org/ns/saref#hasValue",
            "23.5",
            "https://example.org/graph/sensors",
        ),
        (
            1699181401u64,
            "https://rsp.js/event/temp-reading-002",
            "http://www.w3.org/ns/saref#hasValue",
            "24.1",
            "https://example.org/graph/sensors",
        ),
        (
            1699181402u64,
            "https://rsp.js/event/humidity-reading-001",
            "http://www.w3.org/ns/saref#hasValue",
            "65.0",
            "https://example.org/graph/sensors",
        ),
    ];

    // Write events with dictionary encoding
    let mut writer = LogWriter::create(&log_path)?;

    for (timestamp, subject, predicate, object, graph) in &event_uris {
        let subject_id = dict.encode(subject);
        let predicate_id = dict.encode(predicate);
        let object_id = dict.encode(object);
        let graph_id = dict.encode(graph);

        writer.append_record(*timestamp, subject_id, predicate_id, object_id, graph_id)?;
    }

    writer.flush()?;

    // Read back and resolve
    let mut log_file = std::fs::File::open(&log_path)?;
    use std::io::Read;

    for (timestamp, subject, predicate, object, graph) in &event_uris {
        let mut buffer = [0u8; RECORD_SIZE];
        log_file.read_exact(&mut buffer)?;

        let (dec_ts, dec_subj_id, dec_pred_id, dec_obj_id, dec_graph_id) = decode_record(&buffer);

        // Verify timestamp
        assert_eq!(dec_ts, *timestamp);

        // Resolve IDs to URIs
        assert_eq!(dict.decode(dec_subj_id), Some(*subject));
        assert_eq!(dict.decode(dec_pred_id), Some(*predicate));
        assert_eq!(dict.decode(dec_obj_id), Some(*object));
        assert_eq!(dict.decode(dec_graph_id), Some(*graph));
    }

    Ok(())
}

#[test]
fn test_dictionary_space_savings() {
    let mut dict = Dictionary::new();

    // Calculate space used by raw URIs
    let uris = [
        "https://solid.ti.rw.fau.de/public/ns/stream#event001",
        "http://www.w3.org/ns/saref#hasTimestamp",
        "2025-11-05T10:30:00Z",
        "https://solid.ti.rw.fau.de/public/ns/stream#default",
    ];

    let raw_size: usize = uris.iter().map(|u| u.len()).sum();

    // With dictionary, we store 8 bytes per ID
    let ids: Vec<u32> = uris.iter().map(|u| dict.encode(u)).collect();
    let encoded_size = ids.len() * 8; // 8 bytes per u64

    println!("Raw URIs size: {} bytes", raw_size);
    println!("Encoded IDs size: {} bytes", encoded_size);
    println!("Space savings per record: {} bytes", raw_size - encoded_size);

    // For 1000 records reusing same URIs:
    let records = 1000;
    let raw_total = raw_size * records;
    let encoded_total = encoded_size * records + raw_size; // IDs + dictionary overhead

    println!("\nFor {} records:", records);
    println!("Raw storage: {} bytes", raw_total);
    println!(
        "Dictionary storage: {} bytes (IDs) + {} bytes (dictionary)",
        encoded_size * records,
        raw_size
    );
    println!("Total with dictionary: {} bytes", encoded_total);
    println!(
        "Space saved: {} bytes ({:.1}% reduction)",
        raw_total - encoded_total,
        (1.0 - encoded_total as f64 / raw_total as f64) * 100.0
    );

    // Verify space savings
    assert!(encoded_total < raw_total);
}

#[test]
fn test_complete_rdf_quad_with_datatype() {
    let mut dict = Dictionary::new();

    // Complete RDF quad in N-Quads syntax:
    // <https://rsp.js/event/temp-sensor-001> <http://www.w3.org/ns/saref#hasValue> "23.5"^^<http://www.w3.org/2001/XMLSchema#double> <https://example.org/graph/sensors> .
    //
    // This quad has 5 components that get stored in the dictionary:

    let components = vec![
        ("subject", "https://rsp.js/event/temp-sensor-001"),
        ("predicate", "http://www.w3.org/ns/saref#hasValue"),
        ("object_value", "23.5"), // Just the literal value
        ("object_datatype", "http://www.w3.org/2001/XMLSchema#double"), // Datatype as separate URI
        ("graph", "https://example.org/graph/sensors"),
    ];

    // Store all components and get their IDs
    let mut component_ids = std::collections::HashMap::new();
    for (name, value) in &components {
        let id = dict.encode(value);
        component_ids.insert(*name, id);
        println!("{}: '{}' → ID {}", name, value, id);
    }

    // In the actual record, we'd store:
    // - timestamp (u64)
    // - subject_id (u64)
    // - predicate_id (u64)
    // - object_value_id (u64)
    // - graph_id (u64)
    //
    // The object_datatype_id would be stored in a separate metadata structure
    // that tracks which object IDs are literals and what their datatypes are.

    // Verify retrieval
    assert_eq!(dict.decode(component_ids["subject"]), Some(components[0].1));
    assert_eq!(dict.decode(component_ids["predicate"]), Some(components[1].1));
    assert_eq!(dict.decode(component_ids["object_value"]), Some(components[2].1));
    assert_eq!(dict.decode(component_ids["object_datatype"]), Some(components[3].1));
    assert_eq!(dict.decode(component_ids["graph"]), Some(components[4].1));

    // Another quad with the same datatype:
    // <https://rsp.js/event/humidity-sensor-001> <http://www.w3.org/ns/saref#hasValue> "65.2"^^<http://www.w3.org/2001/XMLSchema#double> <https://example.org/graph/sensors> .

    let subject2 = "https://rsp.js/event/humidity-sensor-001";
    let value2 = "65.2";

    let _subject2_id = dict.encode(subject2);
    let _value2_id = dict.encode(value2);

    // These components are REUSED (same ID returned):
    let predicate2_id = dict.encode("http://www.w3.org/ns/saref#hasValue");
    let datatype2_id = dict.encode("http://www.w3.org/2001/XMLSchema#double");
    let graph2_id = dict.encode("https://example.org/graph/sensors");

    // Verify reuse
    assert_eq!(predicate2_id, component_ids["predicate"]);
    assert_eq!(datatype2_id, component_ids["object_datatype"]);
    assert_eq!(graph2_id, component_ids["graph"]);

    // Dictionary has: 5 original components + 2 new (subject2, value2) = 7 total
    assert_eq!(dict.id_to_uri.len(), 7);

    println!("\n✓ Demonstrated RDF datatype handling with dictionary encoding");
    println!("✓ Showed URI reuse across multiple quads (predicate, datatype, graph)");
    println!("✓ Dictionary size: {} entries for 2 complete RDF quads", dict.id_to_uri.len());
}
