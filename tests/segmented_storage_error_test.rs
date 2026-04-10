use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::fs;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_background_flush_failure_surfaces_as_storage_error() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().join("storage");

    let mut storage = StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 1,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1024 * 1024,
        sparse_interval: 10,
        entries_per_index_block: 100,
    })
    .expect("failed to create storage");

    storage.start_background_flushing();

    fs::remove_dir_all(&storage_dir).expect("failed to remove storage directory");

    storage
        .write_rdf(
            1_000,
            "http://example.org/sensor1",
            "http://example.org/temperature",
            "21",
            "http://example.org/graph1",
        )
        .expect("initial write should succeed before background failure is observed");

    thread::sleep(Duration::from_millis(250));

    let err = storage
        .query(0, 2_000)
        .expect_err("query should surface the background flush failure");

    assert!(err.to_string().contains("Background flush failed"), "unexpected error: {err}");

    let shutdown_err = storage
        .shutdown()
        .expect_err("shutdown should also surface the background flush failure");
    assert!(
        shutdown_err.to_string().contains("Background flush failed"),
        "unexpected shutdown error: {shutdown_err}"
    );
}
