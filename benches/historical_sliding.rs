mod support;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use janus::{
    execution::historical_executor::HistoricalExecutor,
    parsing::janusql_parser::{SourceKind, WindowDefinition, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::segmented_storage::StreamingSegmentedStorage,
};
use std::sync::Arc;
use support::{populate_storage, recent_base_timestamp, unique_config, GRAPH_URI};

// Window config: OFFSET=10_000ms, RANGE=2_000ms, SLIDE=1_000ms
// SlidingWindowIterator scans [now-10000, now] with 8 overlapping windows.
// Data is written at [now-8000, now-2000] — solidly within the scan range.
const OFFSET_MS: u64 = 10_000;
const RANGE_MS: u64 = 2_000;
const SLIDE_MS: u64 = 1_000;
const DATA_START_BEFORE_NOW_MS: u64 = 8_000;
const DATA_SPAN_MS: u64 = 6_000;

fn setup(n: usize) -> (Arc<StreamingSegmentedStorage>, WindowDefinition) {
    let start_ts = recent_base_timestamp(DATA_START_BEFORE_NOW_MS);
    let storage = StreamingSegmentedStorage::new(unique_config("historical_sliding")).unwrap();
    let step_ms = (DATA_SPAN_MS / n.max(1) as u64).max(1);
    populate_storage(&storage, n, start_ts, step_ms, GRAPH_URI);
    let window = WindowDefinition {
        window_name: "w".to_string(),
        source_kind: SourceKind::Log,
        source_name: GRAPH_URI.to_string(),
        width: RANGE_MS,
        slide: SLIDE_MS,
        offset: Some(OFFSET_MS),
        start: None,
        end: None,
        window_type: WindowType::HistoricalSliding,
    };
    (Arc::new(storage), window)
}

const SPARQL: &str = r#"
    PREFIX ex: <http://example.org/>
    SELECT ?sensor ?temp
    WHERE {
        GRAPH ex:graph1 {
            ?sensor ex:temperature ?temp .
        }
    }
"#;

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
