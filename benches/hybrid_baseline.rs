mod support;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use janus::{core::RDFEvent, stream::live_stream_processing::LiveStreamProcessing};
use std::time::Duration;
use support::{make_sensor_event, wait_for_live_result, BASELINE_PREDICATE, STREAM_URI};

const HYBRID_RSPQL: &str = r#"
    PREFIX ex: <http://example.org/>
    REGISTER RStream <output> AS
    SELECT ?sensor ?liveTemp ?baselineTemp
    FROM NAMED WINDOW ex:live ON STREAM ex:stream1 [RANGE 10000 STEP 1000]
    WHERE {
        WINDOW ex:live {
            ?sensor ex:temperature ?liveTemp .
        }
        ?sensor ex:baselineTemperature ?baselineTemp .
    }
"#;

fn setup_processor() -> LiveStreamProcessing {
    let mut proc = LiveStreamProcessing::new(HYBRID_RSPQL.to_string()).unwrap();
    proc.register_stream(STREAM_URI).unwrap();

    for i in 0..5u64 {
        proc.add_static_data(RDFEvent::new(
            0,
            &format!("http://example.org/sensor{i}"),
            BASELINE_PREDICATE,
            &format!("{}", 20 + i),
            "",
        ))
        .unwrap();
    }

    proc.start_processing().unwrap();
    proc
}

fn hybrid_baseline_join(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid/baseline_join");
    group.sample_size(20);

    for &n in &[1usize, 10, 100] {
        group.bench_with_input(BenchmarkId::new("events_per_window", n), &n, |b, &n| {
            b.iter_batched(
                setup_processor,
                |proc| {
                    let n64 = n as u64;
                    for i in 0..n64 {
                        let ts = if n64 > 1 { i * 9_000 / (n64 - 1) } else { 0 };
                        proc.add_event(STREAM_URI, make_sensor_event(ts, i, "")).unwrap();
                    }

                    proc.close_stream(STREAM_URI, 20_000).unwrap();
                    let result = wait_for_live_result(&proc, Duration::from_secs(10));
                    black_box(result)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, hybrid_baseline_join);
criterion_main!(benches);
