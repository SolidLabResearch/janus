use janus::parsing::rdf_parser::parse_rdf_line;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use tempfile::TempDir;

fn test_config(base_path: &str) -> StreamingConfig {
    StreamingConfig {
        segment_base_path: base_path.to_string(),
        max_batch_events: 1000,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1_000_000,
        sparse_interval: 1,
        entries_per_index_block: 1,
    }
}

#[test]
fn test_storage_roundtrip_preserves_typed_integer_datatype() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let base_path = temp_dir.path().join("storage");

    {
        let mut storage =
            StreamingSegmentedStorage::new(test_config(base_path.to_string_lossy().as_ref()))
                .expect("failed to create storage");

        let event = parse_rdf_line(
            r#"<sensor1> <hasValue> "23"^^<http://www.w3.org/2001/XMLSchema#integer> ."#,
            false,
        )
        .expect("failed to parse RDF line");

        storage.write_rdf_event(event).expect("failed to write event");
        storage.flush().expect("failed to flush storage");
        storage.shutdown().expect("failed to shutdown storage");
    }

    let reopened =
        StreamingSegmentedStorage::new(test_config(base_path.to_string_lossy().as_ref()))
            .expect("failed to reopen storage");
    let events = reopened.query_rdf(0, u64::MAX).expect("failed to query storage");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].object, "23");
    assert!(events[0].object_is_literal);
    assert_eq!(
        events[0].object_datatype.as_deref(),
        Some("http://www.w3.org/2001/XMLSchema#integer")
    );
}

#[test]
fn test_storage_roundtrip_preserves_object_term_identity() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let base_path = temp_dir.path().join("storage");

    {
        let mut storage =
            StreamingSegmentedStorage::new(test_config(base_path.to_string_lossy().as_ref()))
                .expect("failed to create storage");

        let lines = [
            r#"<s1> <p> "23"^^<http://www.w3.org/2001/XMLSchema#integer> ."#,
            r#"<s2> <p> "23"^^<http://www.w3.org/2001/XMLSchema#decimal> ."#,
            r#"<s3> <p> "23" ."#,
            r#"<s4> <p> <urn:23> ."#,
        ];

        for line in lines {
            let event = parse_rdf_line(line, false).expect("failed to parse RDF line");
            storage.write_rdf_event(event).expect("failed to write event");
        }

        storage.flush().expect("failed to flush storage");
        storage.shutdown().expect("failed to shutdown storage");
    }

    let reopened =
        StreamingSegmentedStorage::new(test_config(base_path.to_string_lossy().as_ref()))
            .expect("failed to reopen storage");
    let events = reopened.query_rdf(0, u64::MAX).expect("failed to query storage");

    assert_eq!(events.len(), 4);

    assert!(events.iter().any(|event| {
        event.subject == "s1"
            && event.object == "23"
            && event.object_is_literal
            && event.object_datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#integer")
    }));
    assert!(events.iter().any(|event| {
        event.subject == "s2"
            && event.object == "23"
            && event.object_is_literal
            && event.object_datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#decimal")
    }));
    assert!(events.iter().any(|event| {
        event.subject == "s3"
            && event.object == "23"
            && event.object_is_literal
            && event.object_datatype.is_none()
    }));
    assert!(events.iter().any(|event| {
        event.subject == "s4"
            && event.object == "urn:23"
            && !event.object_is_literal
            && event.object_datatype.is_none()
    }));
}
