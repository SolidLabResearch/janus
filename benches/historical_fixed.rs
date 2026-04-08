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
        segment_base_path: format!("/tmp/janus_bench_fixed_{}_{}", ts, id),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

/// Write N events at timestamps [1000, 1000+N) into a fresh storage.
/// These land in the in-memory batch buffer — no flush needed before querying.
fn setup(n: usize) -> (Arc<StreamingSegmentedStorage>, WindowDefinition) {
    let storage = StreamingSegmentedStorage::new(unique_config()).unwrap();
    for i in 0..n as u64 {
        storage
            .write_rdf(
                1_000 + i,
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
        width: n as u64,
        slide: n as u64,
        offset: None,
        start: Some(1_000),
        end: Some(1_000 + n as u64 - 1),
        window_type: WindowType::HistoricalFixed,
    };
    (Arc::new(storage), window)
}

const SPARQL: &str = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }";

fn historical_fixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("historical/fixed_window");

    for &n in &[100usize, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::new("events", n), &n, |b, &n| {
            b.iter_batched(
                || setup(n),
                |(storage, window)| {
                    let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
                    black_box(executor.execute_fixed_window(&window, SPARQL).unwrap())
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, historical_fixed);
criterion_main!(benches);
