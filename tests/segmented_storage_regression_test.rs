use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn create_storage(storage_dir: &Path) -> StreamingSegmentedStorage {
    StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 1_000,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1_000_000,
        sparse_interval: 1,
        entries_per_index_block: 1,
    })
    .expect("failed to create storage")
}

fn segment_ids(storage_dir: &Path) -> BTreeSet<u64> {
    let mut ids = BTreeSet::new();

    for entry in fs::read_dir(storage_dir).expect("failed to read storage directory") {
        let entry = entry.expect("failed to read storage entry");
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };

        let Some(id_str) = name.strip_prefix("segment-").and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };

        ids.insert(id_str.parse().expect("failed to parse segment id"));
    }

    ids
}

fn query_all_events(
    storage: &StreamingSegmentedStorage,
) -> Vec<(u64, String, String, String, String)> {
    storage
        .query_rdf(0, u64::MAX)
        .expect("failed to query storage")
        .into_iter()
        .map(|event| (event.timestamp, event.subject, event.predicate, event.object, event.graph))
        .collect()
}

#[test]
fn test_storage_immediate_flushes_create_distinct_segment_files() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");
    let storage = create_storage(&storage_dir);

    storage
        .write_rdf(
            1_000,
            "http://example.org/sensor/1",
            "http://example.org/observedValue",
            "21",
            "http://example.org/graph/a",
        )
        .expect("failed to write first event");
    storage
        .write_rdf(
            1_100,
            "http://example.org/sensor/2",
            "http://example.org/observedValue",
            "22",
            "http://example.org/graph/a",
        )
        .expect("failed to write second event");
    storage.flush().expect("first flush should succeed");

    storage
        .write_rdf(
            2_000,
            "http://example.org/sensor/3",
            "http://example.org/observedValue",
            "23",
            "http://example.org/graph/b",
        )
        .expect("failed to write third event");
    storage
        .write_rdf(
            2_100,
            "http://example.org/sensor/4",
            "http://example.org/observedValue",
            "24",
            "http://example.org/graph/b",
        )
        .expect("failed to write fourth event");
    storage.flush().expect("second flush should succeed");

    let ids = segment_ids(&storage_dir);
    assert_eq!(ids.len(), 2, "expected two distinct segment ids");
    for id in &ids {
        assert!(
            storage_dir.join(format!("segment-{id}.idx")).exists(),
            "missing index file for segment {id}"
        );
    }

    let actual = query_all_events(&storage);
    let expected = vec![
        (
            1_000,
            "http://example.org/sensor/1".to_string(),
            "http://example.org/observedValue".to_string(),
            "21".to_string(),
            "http://example.org/graph/a".to_string(),
        ),
        (
            1_100,
            "http://example.org/sensor/2".to_string(),
            "http://example.org/observedValue".to_string(),
            "22".to_string(),
            "http://example.org/graph/a".to_string(),
        ),
        (
            2_000,
            "http://example.org/sensor/3".to_string(),
            "http://example.org/observedValue".to_string(),
            "23".to_string(),
            "http://example.org/graph/b".to_string(),
        ),
        (
            2_100,
            "http://example.org/sensor/4".to_string(),
            "http://example.org/observedValue".to_string(),
            "24".to_string(),
            "http://example.org/graph/b".to_string(),
        ),
    ];
    assert_eq!(actual, expected);
}

#[test]
fn test_storage_rapid_flush_loop_keeps_all_segments_queryable() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");
    let storage = create_storage(&storage_dir);

    let mut expected = Vec::new();
    for i in 0..8u64 {
        let ts = 10_000 + i;
        let subject = format!("http://example.org/device/{i}");
        let object = format!("http://example.org/value/{i}");
        let graph = format!("http://example.org/graph/{}", i % 2);

        storage
            .write_rdf(ts, &subject, "http://example.org/reading", &object, &graph)
            .expect("failed to write event");
        storage.flush().expect("flush should succeed");

        expected.push((ts, subject, "http://example.org/reading".to_string(), object, graph));
    }

    let ids = segment_ids(&storage_dir);
    assert_eq!(ids.len(), expected.len(), "segment ids should stay unique");
    for id in &ids {
        assert!(
            storage_dir.join(format!("segment-{id}.log")).exists(),
            "missing data file for segment {id}"
        );
        assert!(
            storage_dir.join(format!("segment-{id}.idx")).exists(),
            "missing index file for segment {id}"
        );
    }

    assert_eq!(query_all_events(&storage), expected);
}

#[test]
fn test_dictionary_atomic_writes_cleanup() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");
    fs::create_dir_all(&storage_dir).unwrap();

    let config = StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 100,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1_000_000,
        sparse_interval: 1,
        entries_per_index_block: 1,
    };

    {
        let mut storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
        storage.start_background_flushing();

        storage.write_rdf(1000, "http://a", "http://b", "10", "http://g").unwrap();
        storage.flush().unwrap();
        storage.shutdown().unwrap();
    }

    let dict_path = storage_dir.join("dictionary.bin");
    assert!(dict_path.exists());

    for entry in fs::read_dir(&storage_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name_str = name.to_str().unwrap();
        assert!(!name_str.contains(".tmp"), "Found leftover temp file: {}", name_str);
    }

    {
        let mut storage = StreamingSegmentedStorage::new(config).unwrap();
        storage.start_background_flushing();

        let events = query_all_events(&storage);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].1, "http://a");

        storage
            .write_rdf(2000, "http://new_subj", "http://b", "20", "http://g")
            .unwrap();
        storage.flush().unwrap();
        storage.shutdown().unwrap();
    }
}

#[test]
fn test_shutdown_race_safety() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");
    fs::create_dir_all(&storage_dir).unwrap();

    let config = StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 5,
        max_batch_age_seconds: 1,
        max_batch_bytes: 10_000,
        sparse_interval: 1,
        entries_per_index_block: 1,
    };

    let event_count = 20;
    {
        let mut storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
        storage.start_background_flushing();

        for i in 0..event_count {
            let ts = 1000 + i;
            let subject = format!("http://example.org/sensor/{}", i);
            storage
                .write_rdf(ts, &subject, "http://example.org/val", "12", "http://g")
                .unwrap();

            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        storage.shutdown().unwrap();
    }

    {
        let storage = StreamingSegmentedStorage::new(config).unwrap();
        let events = query_all_events(&storage);

        assert_eq!(events.len(), event_count as usize, "Mismatch in event count after shutdown");

        let mut timestamps = std::collections::HashSet::new();
        for event in &events {
            assert!(timestamps.insert(event.0), "Duplicate event timestamp found: {}", event.0);
        }
    }
}

#[test]
fn opening_persisted_segments_without_dictionary_fails() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");
    let config = StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 100,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1_000_000,
        sparse_interval: 1,
        entries_per_index_block: 2,
    };

    {
        let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
        storage.write_rdf(1_000, "http://a", "http://b", "value", "http://g").unwrap();
        storage.flush().unwrap();
    }

    fs::remove_file(storage_dir.join("dictionary.bin")).unwrap();
    let err = match StreamingSegmentedStorage::new(config) {
        Ok(_) => panic!("missing dictionary must fail"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("persisted segment data"));
    assert!(err.to_string().contains("dictionary"));
}

#[test]
fn opening_persisted_segments_with_corrupt_dictionary_fails() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");
    let config = StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 100,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1_000_000,
        sparse_interval: 1,
        entries_per_index_block: 2,
    };

    {
        let storage = StreamingSegmentedStorage::new(config.clone()).unwrap();
        storage.write_rdf(1_000, "http://a", "http://b", "value", "http://g").unwrap();
        storage.flush().unwrap();
    }

    fs::write(storage_dir.join("dictionary.bin"), b"not a dictionary").unwrap();
    let err = match StreamingSegmentedStorage::new(config) {
        Ok(_) => panic!("corrupt dictionary must fail"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("persisted segment data"));
    assert!(err.to_string().contains("readable dictionary"));
}
