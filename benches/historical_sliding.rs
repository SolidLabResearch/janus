use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use janus::{
    execution::historical_executor::HistoricalExecutor,
    parsing::janusql_parser::{SourceKind, WindowDefinition, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_config() -> StreamingConfig {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    StreamingConfig {
        segment_base_path: format!("/tmp/janus_bench_sliding_{}_{}", ts, id),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

// Window config: OFFSET=10_000ms, RANGE=2_000ms, SLIDE=1_000ms
// SlidingWindowIterator scans [now-10000, now] with 8 overlapping windows.
// Data is written at [now-8000, now-2000] — solidly within the scan range.
const OFFSET_MS: u64 = 10_000;
const RANGE_MS: u64 = 2_000;
const SLIDE_MS: u64 = 1_000;
const DATA_START_BEFORE_NOW_MS: u64 = 8_000;
const DATA_SPAN_MS: u64 = 6_000;

fn setup(n: usize) -> (Arc<StreamingSegmentedStorage>, WindowDefinition) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let storage = StreamingSegmentedStorage::new(unique_config()).unwrap();
    let n64 = n as u64;
    for i in 0..n64 {
        let ts = now - DATA_START_BEFORE_NOW_MS + i * DATA_SPAN_MS / n64.max(1);
        storage
            .write_rdf(
                ts,
                &format!("http://example.org/sensor{}", i % 5),
                "http://saref.etsi.org/core/hasValue",
                &format!("{}", 20 + (i % 10)),
                "http://example.org/graph",
            )
            .unwrap();
    }
    let window = WindowDefinition {
        window_name: "w".to_string(),
        source_kind: SourceKind::Stream,
        stream_name: "http://example.org/stream".to_string(),
        width: RANGE_MS,
        slide: SLIDE_MS,
        offset: Some(OFFSET_MS),
        start: None,
        end: None,
        window_type: WindowType::HistoricalSliding,
    };
    (Arc::new(storage), window)
}

const SPARQL: &str = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }";

fn historical_sliding(c: &mut Criterion) {
    let mut group = c.benchmark_group("historical/sliding_window");

    for &n in &[100usize, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::new("events", n), &n, |b, &n| {
            b.iter_batched(
                || setup(n),
                |(storage, window)| {
                    let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
                    // Collect all window results — the iterator is finite and exits naturally
                    let results: Vec<_> =
                        executor.execute_sliding_windows(&window, SPARQL).collect();
                    black_box(results)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, historical_sliding);
criterion_main!(benches);
