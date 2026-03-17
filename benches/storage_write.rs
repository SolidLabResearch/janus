use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use janus::storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_config() -> StreamingConfig {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    StreamingConfig {
        segment_base_path: format!("/tmp/janus_bench_write_{}_{}", ts, id),
        // Large thresholds so no flush happens during measurement
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

fn storage_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage/write_throughput");

    for &n in &[100usize, 1_000, 10_000, 100_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || StreamingSegmentedStorage::new(unique_config()).unwrap(),
                |storage| {
                    for i in 0..n as u64 {
                        storage
                            .write_rdf(
                                black_box(1_000 + i),
                                &format!("http://example.org/sensor{}", i % 5),
                                "http://saref.etsi.org/core/hasValue",
                                &format!("{}", 20 + (i % 10)),
                                "http://example.org/graph",
                            )
                            .unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, storage_write);
criterion_main!(benches);
