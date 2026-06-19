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
