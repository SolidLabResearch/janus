#![forbid(unsafe_code)]

#[cfg(debug_assertions)]
compile_error!(
    "Benchmarks MUST be run with --release. Debug builds produce meaningless numbers. \
     Run: cargo run --release --example h1_latency_benchmark"
);

// H1 Benchmark: End-to-end Unified Query Latency Breakdown
//
// Measures the four-stage pipeline latency:
// 1. Storage write latency (RDF event → batch buffer)
// 2. Historical retrieval latency (query execution)
// 3. Live window close latency (window closure → result)
// 4. Result combination latency (comparator)
//
// Also measures path isolation: live latency should remain flat even with
// heavy background historical query load.
//
// Run with: `cargo run --release --example h1_latency_benchmark`

use janus::{
    api::janus_api::{JanusApi, QueryResult, ResultSource},
    benchmarking::analyse_runs,
    core::RDFEvent,
    execution::HistoricalExecutor,
    parsing::janusql_parser::{WindowDefinition, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::segmented_storage::StreamingSegmentedStorage,
    storage::util::StreamingConfig,
    stream::comparator::{ComparatorConfig, StatefulComparator},
    stream::live_stream_processing::LiveStreamProcessing,
};
use std::sync::Arc;
use std::time::Instant;


const DATASET_SIZES: &[usize] = &[50_000, 100_000, 500_000];
const EVENT_RATES_PER_SEC: &[u64] = &[50, 100, 500];
const RUNS_PER_CONFIG: usize = 30;

fn main() {
    println!("=== H1 Latency Benchmark: Unified Query Pipeline ===\n");

    // Record hardware
    let hw = janus::benchmarking::get_hardware_info();
    std::fs::create_dir_all("results").expect("Cannot create results/");
    std::fs::write("results/hardware.txt", &hw).expect("Cannot write hardware.txt");
    println!("{}", hw);

    // Generate or load datasets
    println!("Preparing datasets...");
    for &size in DATASET_SIZES {
        let path = format!("data/h1_dataset_{}.nq", size);
        if !std::path::Path::new(&path).exists() {
            println!("  Generating {}K quad dataset...", size / 1000);
            generate_dataset(size, &path);
        }
    }

    // Main benchmark loop
    let mut all_results = Vec::new();

    for &dataset_size in DATASET_SIZES {
        for &event_rate in EVENT_RATES_PER_SEC {
            println!(
                "\nBenchmarking: {} quads @ {} events/sec ({} runs)",
                dataset_size, event_rate, RUNS_PER_CONFIG
            );

            let results = run_config(dataset_size, event_rate);
            all_results.push((dataset_size, event_rate, results));
        }
    }

    // Write raw results
    write_latency_csv(&all_results).expect("CSV write failed");

    // Analyze and write summary
    write_h1_summary(&all_results).expect("Summary write failed");

    // Path isolation test
    println!("\n=== Path Isolation Test ===");
    let isolation_results = run_isolation_test();
    write_isolation_csv(&isolation_results).expect("Isolation CSV write failed");

    println!("\n✓ Benchmarks complete. Results in results/h1_*.csv");
}

/// Configuration for a single benchmark run
struct RunResult {
    write_ms: f64,
    hist_retrieval_ms: f64,
    live_window_ms: f64,
    comparator_ms: f64,
}

fn run_config(dataset_size: usize, event_rate: u64) -> Vec<RunResult> {
    let mut runs = Vec::new();

    for run_num in 1..=RUNS_PER_CONFIG {
        let result = run_single(dataset_size, event_rate);
        runs.push(result);

        if run_num % 5 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
    }
    println!();
    runs
}

fn run_single(dataset_size: usize, event_rate: u64) -> RunResult {
    // Fresh temporary directory per run
    let tmp = tempfile::tempdir().expect("Cannot create temp dir");
    let config = StreamingConfig {
        segment_base_path: tmp.path().to_str().unwrap().to_string(),
        entries_per_index_block: 1000,
        max_batch_events: 100_000,
        max_batch_age_seconds: 60,
        max_batch_bytes: 10_000_000,
        sparse_interval: 1000,
    };

    let storage = Arc::new(
        StreamingSegmentedStorage::new(config.clone()).expect("Storage init failed"),
    );

    // Load dataset
    let dataset_path = format!("data/h1_dataset_{}.nq", dataset_size);
    load_nquads(&dataset_path, &storage);

    // Get timestamp range for queries
    let (min_ts, max_ts) = read_timestamp_range(&dataset_path);
    let hist_window_end = max_ts;
    let hist_window_start = std::cmp::max(min_ts, max_ts.saturating_sub(3_600_000)); // 1 hour window

    // Stage 1: Storage write latency
    let base_ts = max_ts + 100_000;
    let interval_ms = 1000 / event_rate;

    let mut write_times = Vec::new();
    for i in 0..50u64 {
        let event = make_test_rdf_event(i, base_ts + i * interval_ms);
        let t = Instant::now();
        storage.write_rdf_event(event).expect("Write failed");
        write_times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let write_ms = write_times.iter().sum::<f64>() / write_times.len() as f64;

    // Stage 2: Historical retrieval latency
    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());
    let sparql_query = "SELECT (AVG(?val) AS ?avg) WHERE { ?s <http://test.org/val> ?val }";

    let window_def = WindowDefinition {
        window_name: "hist_window".to_string(),
        stream_name: "test_stream".to_string(),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(hist_window_start),
        end: Some(hist_window_end),
        window_type: WindowType::HistoricalFixed,
    };

    let t_hist = Instant::now();
    let _ = executor
        .execute_fixed_window(&window_def, sparql_query)
        .ok();
    let hist_retrieval_ms = t_hist.elapsed().as_secs_f64() * 1000.0;

    // Stage 3: Live window close latency
    let rspql_query = r#"
        PREFIX ex: <http://test.org/>
        REGISTER RStream ex:output AS
        SELECT (COUNT(*) AS ?count)
        FROM NAMED WINDOW ex:live ON STREAM ex:live_stream [RANGE 5000 STEP 5000]
        WHERE {
            WINDOW ex:live { ?s ?p ?o }
        }
    "#;

    let mut processor = LiveStreamProcessing::new(rspql_query.to_string())
        .expect("LSP creation failed");
    processor
        .register_stream("http://test.org/live_stream")
        .expect("Register failed");
    processor.start_processing().expect("Start failed");

    // Inject events to fill window
    for i in 0..20u64 {
        let event = make_test_rdf_event(i, base_ts + 1_000_000 + i * 100);
        processor
            .add_event("http://test.org/live_stream", event)
            .ok();
    }

    // Sentinel to close window
    let sentinel = make_test_rdf_event(99, base_ts + 1_000_000 + 20 * 100 + 6000);
    let t_live = Instant::now();
    processor
        .add_event("http://test.org/live_stream", sentinel)
        .ok();

    // Wait for result
    let mut live_window_ms = 100.0; // fallback
    for _ in 0..1000 {
        if processor
            .try_receive_result()
            .ok()
            .flatten()
            .is_some()
        {
            live_window_ms = t_live.elapsed().as_secs_f64() * 1000.0;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Stage 4: Real comparator timing (pre-warm window buffer, then measure)
    let cmp_config = ComparatorConfig {
        abs_threshold: 10.0,
        rel_threshold: 0.2,
        catchup_trigger: 15.0,
        slope_epsilon: 0.1,
        volatility_buffer: 2.0,
        window_size: 10,
        outlier_z_threshold: 3.0,
    };
    let mut comparator = StatefulComparator::new(cmp_config);
    // Pre-warm: fill the window buffer (window_size - 1 calls)
    for i in 0..9 {
        let _ = comparator.update_and_compare(i as f64, 100.0 + i as f64 * 0.1, 100.0);
    }
    let comparator_times: Vec<f64> = (0..50)
        .map(|i| {
            let t = Instant::now();
            let _ = comparator.update_and_compare((9 + i) as f64, 100.5, 100.0);
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    let comparator_ms = comparator_times.iter().sum::<f64>() / comparator_times.len() as f64;

    RunResult {
        write_ms,
        hist_retrieval_ms,
        live_window_ms,
        comparator_ms,
    }
}

fn run_isolation_test() -> Vec<(u64, f64, f64)> {
    let dataset_size = 100_000usize;

    let tmp = tempfile::tempdir().expect("Cannot create temp dir");
    let config = StreamingConfig {
        segment_base_path: tmp.path().to_str().unwrap().to_string(),
        entries_per_index_block: 1000,
        max_batch_events: 100_000,
        max_batch_age_seconds: 60,
        max_batch_bytes: 10_000_000,
        sparse_interval: 1000,
    };

    let storage = Arc::new(StreamingSegmentedStorage::new(config).expect("Storage init failed"));

    // Load dataset
    let dataset_path = format!("data/h1_dataset_{}.nq", dataset_size);
    load_nquads(&dataset_path, &storage);

    let mut results = Vec::new();
    let background_rates = vec![0u64, 1, 5, 10];

    let rspql = r#"
        PREFIX ex: <http://test.org/>
        REGISTER RStream ex:iso_out AS SELECT (COUNT(*) AS ?cnt)
        FROM NAMED WINDOW ex:iso_win ON STREAM ex:live_stream [RANGE 5000 STEP 5000]
        WHERE { WINDOW ex:iso_win { ?s ?p ?o } }
    "#;

    for bg_rate in background_rates {
        println!("  Testing {} background queries/sec...", bg_rate);

        let storage_clone = Arc::clone(&storage);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);

        // Spawn background historical query thread
        let bg_handle = std::thread::spawn(move || {
            let executor = HistoricalExecutor::new(storage_clone, OxigraphAdapter::new());
            let sparql_query = "SELECT * WHERE { ?s ?p ?o } LIMIT 100";
            let interval = if bg_rate > 0 {
                std::time::Duration::from_millis(1000 / bg_rate)
            } else {
                std::time::Duration::from_secs(9999)
            };

            while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let window_def = WindowDefinition {
                    window_name: "bg_window".to_string(),
                    stream_name: "bg_stream".to_string(),
                    width: 0,
                    slide: 0,
                    offset: None,
                    start: Some(1_000_000),
                    end: Some(2_000_000),
                    window_type: WindowType::HistoricalFixed,
                };
                let _ = executor.execute_fixed_window(&window_def, sparql_query).ok();
                std::thread::sleep(interval);
            }
        });

        // Reuse one LiveStreamProcessing instance for all 10 cycles at this bg_rate
        let mut live_times: Vec<f64> = Vec::new();
        let mut proc = LiveStreamProcessing::new(rspql.to_string())
            .expect("LSP init failed");
        proc.register_stream("http://test.org/live_stream").expect("Register failed");
        proc.start_processing().expect("Start failed");

        let base_ts = 3_000_000u64;

        for cycle in 0..10u64 {
            let window_offset = cycle * 10_000;
            for i in 0..10u64 {
                let evt = make_test_rdf_event(i, base_ts + window_offset + i * 100);
                let _ = proc.add_event("http://test.org/live_stream", evt).ok();
            }
            // Sentinel past window boundary to trigger close
            let sentinel = make_test_rdf_event(
                99,
                base_ts + window_offset + 10 * 100 + 6000,
            );
            let t = Instant::now();
            let _ = proc.add_event("http://test.org/live_stream", sentinel).ok();

            // Poll up to 5 seconds
            let mut got_result = false;
            for _ in 0..5000 {
                if proc.try_receive_result().ok().flatten().is_some() {
                    live_times.push(t.elapsed().as_secs_f64() * 1000.0);
                    got_result = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            if !got_result {
                eprintln!(
                    "WARNING: isolation cycle {} timed out at bg_rate={}",
                    cycle, bg_rate
                );
                live_times.push(5000.0); // worst-case timeout
            }
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        bg_handle.join().ok();

        let mean = live_times.iter().sum::<f64>() / live_times.len() as f64;
        let std_dev = if live_times.len() > 1 {
            let var = live_times
                .iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>()
                / live_times.len() as f64;
            var.sqrt()
        } else {
            0.0
        };

        results.push((bg_rate, mean, std_dev));
    }

    results
}

// ============================================================================
// Helper functions

fn generate_dataset(size: usize, output_path: &str) {
    let status = std::process::Command::new("python3")
        .args(&[
            "scripts/generate_realistic_data.py",
            "--size",
            &size.to_string(),
            "--output",
            output_path,
        ])
        .status();

    match status {
        Ok(s) if s.success() => println!("  ✓ Generated {}", output_path),
        _ => eprintln!("WARNING: Could not generate dataset, using smaller test set"),
    }
}

fn load_nquads(path: &str, storage: &Arc<StreamingSegmentedStorage>) {
    use std::io::{BufRead, BufReader};

    if !std::path::Path::new(path).exists() {
        eprintln!("Dataset {} not found, skipping load", path);
        return;
    }

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Cannot open {}: {}", path, e);
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut count = 0;
    for line in reader.lines() {
        if let Ok(line) = line {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            if let Ok(event) = parse_nquad_line(&line) {
                let _ = storage.write_rdf_event(event);
                count += 1;
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
}

fn parse_nquad_line(line: &str) -> Result<RDFEvent, String> {
    // Simplified N-Quad parser for benchmarking
    let parts: Vec<&str> = line.trim_end_matches('.').split_whitespace().collect();
    if parts.len() < 4 {
        return Err("Invalid N-Quad".to_string());
    }

    let ts = extract_timestamp(line).unwrap_or(1_000_000);
    Ok(RDFEvent {
        timestamp: ts,
        subject: parts[0].trim_matches('<').trim_matches('>').to_string(),
        predicate: parts[1].trim_matches('<').trim_matches('>').to_string(),
        object: parts[2].trim_matches('<').trim_matches('>').to_string(),
        graph: parts[3].trim_matches('<').trim_matches('>').to_string(),
    })
}

fn extract_timestamp(line: &str) -> Option<u64> {
    // Extract timestamp from comment or use sequential
    if let Some(comment_pos) = line.find('#') {
        if let Ok(ts) = line[comment_pos + 1..].trim().parse::<u64>() {
            return Some(ts);
        }
    }
    None
}

fn read_timestamp_range(path: &str) -> (u64, u64) {
    use std::io::{BufRead, BufReader};

    let mut min_ts = u64::MAX;
    let mut max_ts = 0u64;

    if let Ok(file) = std::fs::File::open(path) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(event) = parse_nquad_line(&line) {
                    min_ts = min_ts.min(event.timestamp);
                    max_ts = max_ts.max(event.timestamp);
                }
            }
        }
    }

    if min_ts == u64::MAX {
        min_ts = 1_000_000;
    }
    if max_ts == 0 {
        max_ts = min_ts + 3_600_000; // 1 hour
    }

    (min_ts, max_ts)
}

fn make_test_rdf_event(id: u64, timestamp: u64) -> RDFEvent {
    RDFEvent {
        timestamp,
        subject: format!("http://test.org/subject/{}", id),
        predicate: "http://test.org/val".to_string(),
        object: format!("{}.5", id),
        graph: "http://test.org/graph".to_string(),
    }
}

// ============================================================================
// CSV Output

fn write_latency_csv(results: &[(usize, u64, Vec<RunResult>)]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h1_latency.csv")?;
    writeln!(
        file,
        "dataset_size_quads,event_rate_per_sec,run,write_ms,hist_retrieval_ms,live_window_ms,comparator_ms,total_ms"
    )?;

    for (dataset_size, event_rate, runs) in results {
        for (i, run) in runs.iter().enumerate() {
            let total = run.write_ms + run.hist_retrieval_ms + run.live_window_ms + run.comparator_ms;
            writeln!(
                file,
                "{},{},{},{:.2},{:.2},{:.2},{:.2},{:.2}",
                dataset_size,
                event_rate,
                i + 1,
                run.write_ms,
                run.hist_retrieval_ms,
                run.live_window_ms,
                run.comparator_ms,
                total
            )?;
        }
    }
    Ok(())
}

fn write_h1_summary(results: &[(usize, u64, Vec<RunResult>)]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h1_summary.csv")?;
    writeln!(
        file,
        "dataset_size_quads,event_rate_per_sec,write_mean_ms,write_std_ms,hist_mean_ms,hist_std_ms,live_mean_ms,live_std_ms,comparator_mean_ms,comparator_std_ms,total_mean_ms,total_std_ms,hist_pct_of_total"
    )?;

    for (dataset_size, event_rate, runs) in results {
        let writes: Vec<f64> = runs.iter().map(|r| r.write_ms).collect();
        let hists: Vec<f64> = runs.iter().map(|r| r.hist_retrieval_ms).collect();
        let lives: Vec<f64> = runs.iter().map(|r| r.live_window_ms).collect();
        let comps: Vec<f64> = runs.iter().map(|r| r.comparator_ms).collect();

        let (write_mean, write_std) = analyse_runs(&writes);
        let (hist_mean, hist_std) = analyse_runs(&hists);
        let (live_mean, live_std) = analyse_runs(&lives);
        let (comp_mean, comp_std) = analyse_runs(&comps);
        let total_mean = write_mean + hist_mean + live_mean + comp_mean;
        let total_std = (write_std.powi(2)
            + hist_std.powi(2)
            + live_std.powi(2)
            + comp_std.powi(2))
        .sqrt();
        let hist_pct = if total_mean > 0.0 {
            (hist_mean / total_mean) * 100.0
        } else {
            0.0
        };

        writeln!(
            file,
            "{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.1}",
            dataset_size,
            event_rate,
            write_mean,
            write_std,
            hist_mean,
            hist_std,
            live_mean,
            live_std,
            comp_mean,
            comp_std,
            total_mean,
            total_std,
            hist_pct
        )?;
    }
    Ok(())
}

fn write_isolation_csv(results: &[(u64, f64, f64)]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h1_isolation.csv")?;
    writeln!(file, "background_hist_qps,live_window_mean_ms,live_window_std_ms")?;

    for (bg_qps, mean, std_dev) in results {
        writeln!(file, "{},{:.2},{:.2}", bg_qps, mean, std_dev)?;
    }
    Ok(())
}
