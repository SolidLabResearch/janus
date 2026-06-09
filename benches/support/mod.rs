#![allow(dead_code)]

use janus::{
    core::RDFEvent,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
    stream::live_stream_processing::LiveStreamProcessing,
};
use rsp_rs::BindingWithTimestamp;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const GRAPH_URI: &str = "http://example.org/graph1";
pub const STREAM_URI: &str = "http://example.org/stream1";
pub const TEMPERATURE_PREDICATE: &str = "http://example.org/temperature";
pub const BASELINE_PREDICATE: &str = "http://example.org/baselineTemperature";

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn unique_config(prefix: &str) -> StreamingConfig {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    StreamingConfig {
        segment_base_path: format!("/tmp/janus_bench_{prefix}_{ts}_{id}"),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

pub fn make_sensor_event(timestamp_ms: u64, index: u64, graph: &str) -> RDFEvent {
    RDFEvent::new(
        timestamp_ms,
        &format!("http://example.org/sensor{}", index % 5),
        TEMPERATURE_PREDICATE,
        &format!("{}", 20 + (index % 10)),
        graph,
    )
}

pub fn populate_storage(
    storage: &StreamingSegmentedStorage,
    event_count: usize,
    start_timestamp_ms: u64,
    step_ms: u64,
    graph: &str,
) {
    for i in 0..event_count as u64 {
        let event = make_sensor_event(start_timestamp_ms + i * step_ms, i, graph);
        storage.write_rdf_event(event).unwrap();
    }
}

pub fn recent_base_timestamp(offset_ms: u64) -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    now.saturating_sub(offset_ms)
}

pub fn wait_for_live_result(
    processor: &LiveStreamProcessing,
    timeout: Duration,
) -> BindingWithTimestamp {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(result) = processor.try_receive_result().unwrap() {
            return result;
        }

        assert!(
            Instant::now() < deadline,
            "benchmark timed out waiting for a live result after {:?}",
            timeout
        );
        std::thread::yield_now();
    }
}
