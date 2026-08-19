mod support;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use janus::{
    execution::historical_executor::HistoricalExecutor,
    parsing::janusql_parser::{SourceKind, WindowDefinition, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::segmented_storage::StreamingSegmentedStorage,
};
use std::sync::Arc;
use support::{populate_storage, unique_config, GRAPH_URI};

/// Write N events at timestamps [1000, 1000+N) into a fresh storage.
/// These land in the in-memory batch buffer — no flush needed before querying.
fn setup(n: usize) -> (Arc<StreamingSegmentedStorage>, WindowDefinition) {
    let storage = StreamingSegmentedStorage::new(unique_config("historical_fixed")).unwrap();
    populate_storage(&storage, n, 1_000, 1, GRAPH_URI);
    let window = WindowDefinition {
        window_name: "w".to_string(),
        source_kind: SourceKind::Log,
        source_name: GRAPH_URI.to_string(),
        width: n as u64,
        slide: n as u64,
        offset: None,
        start: Some(1_000),
        end: Some(1_000 + n as u64 - 1),
        window_type: WindowType::HistoricalFixed,
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
