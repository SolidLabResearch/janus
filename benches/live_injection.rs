use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use janus::{core::RDFEvent, stream::live_stream_processing::LiveStreamProcessing};
use std::time::Instant;

const STREAM_URI: &str = "http://example.org/stream1";

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

fn make_event(timestamp_ms: u64, i: u64) -> RDFEvent {
    RDFEvent::new(
        timestamp_ms,
        &format!("http://example.org/sensor{}", i % 5),
        "http://saref.etsi.org/core/hasValue",
        &format!("{}", 20 + (i % 10)),
        "",
    )
}

/// Wait for the first live result with a 10-second hard deadline.
/// Panics with a clear message if nothing arrives — indicates the RSP engine
/// is not emitting results for the injected events.
fn wait_for_result(proc: &LiveStreamProcessing) -> rsp_rs::BindingWithTimestamp {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(result) = proc.try_receive_result().unwrap() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "live_injection: no result within 10s — RSP engine did not emit for injected events"
        );
        std::thread::yield_now();
    }
}

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
                        proc.add_event(STREAM_URI, make_event(ts, i)).unwrap();
                    }
                    // Sentinel at 20_000 ms closes all open windows
                    proc.add_event(STREAM_URI, make_event(20_000, 999)).unwrap();
                    // Block until first result arrives
                    black_box(wait_for_result(&proc))
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, live_injection);
criterion_main!(benches);
