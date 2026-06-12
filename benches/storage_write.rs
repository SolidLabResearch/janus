mod support;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use support::{make_sensor_event, unique_config, GRAPH_URI};

fn storage_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage/write_throughput");

    for &n in &[100usize, 1_000, 10_000, 100_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || StreamingSegmentedStorage::new(unique_config("write")).unwrap(),
                |storage| {
                    for i in 0..n as u64 {
                        let event = make_sensor_event(1_000 + i, i, GRAPH_URI);
                        storage.write_rdf_event(black_box(event)).unwrap();
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
