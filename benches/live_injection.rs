mod support;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use janus::stream::live_stream_processing::LiveStreamProcessing;
use std::time::Duration;
use support::{make_sensor_event, wait_for_live_result, STREAM_URI};

// RSP-QL query: 10s range, 1s step window over stream1
const RSPQL: &str = r#"
    PREFIX ex: <http://example.org/>
    REGISTER RStream <output> AS
    SELECT ?s ?p ?o
    FROM NAMED WINDOW ex:w ON STREAM ex:stream1 [RANGE 10000 STEP 1000]
    WHERE {
        WINDOW ex:w { ?s ?p ?o }
    }
"#;

fn live_injection(c: &mut Criterion) {
    let mut group = c.benchmark_group("live/event_injection");
    // Lower sample size: each iteration spawns an RSP engine thread
    group.sample_size(20);

    for &n in &[1usize, 10, 100] {
        group.bench_with_input(BenchmarkId::new("events_per_window", n), &n, |b, &n| {
            b.iter_batched(
                || {
                    let mut proc = LiveStreamProcessing::new(RSPQL.to_string()).unwrap();
                    proc.register_stream(STREAM_URI).unwrap();
                    proc.start_processing().unwrap();
                    proc
                },
                |proc| {
                    // Spread N events evenly across [0, 9000] ms (inside the RANGE 10000 window)
                    let n64 = n as u64;
                    for i in 0..n64 {
                        let ts = if n64 > 1 { i * 9_000 / (n64 - 1) } else { 0 };
                        proc.add_event(STREAM_URI, make_sensor_event(ts, i, "")).unwrap();
                    }
                    // Sentinel at 20_000 ms closes all open windows
                    proc.add_event(STREAM_URI, make_sensor_event(20_000, 999, "")).unwrap();
                    // Block until first result arrives
                    black_box(wait_for_live_result(&proc, Duration::from_secs(10)))
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, live_injection);
criterion_main!(benches);
