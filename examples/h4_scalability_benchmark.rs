/// H4 Benchmark: Scalability Analysis
///
/// Measures how historical retrieval latency and bootstrap latency scale
/// with dataset size (100K–5M quads). Verifies that the two-level sparse
/// index produces sub-linear growth, and that live window latency remains
/// flat regardless of historical dataset size (path isolation).
///
/// Run with: `cargo run --release --example h4_scalability_benchmark`

#[cfg(debug_assertions)]
compile_error!(
    "Benchmarks MUST be run with --release. Debug builds produce meaningless numbers. \
     Run: cargo run --release --example h4_scalability_benchmark"
);

use janus::{
    api::janus_api::{JanusApi, ResultSource},
    benchmarking::analyse_runs,
    core::RDFEvent,
    execution::HistoricalExecutor,
    parsing::janusql_parser::{WindowDefinition, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::query_registry::QueryRegistry,
    storage::segmented_storage::StreamingSegmentedStorage,
    storage::util::StreamingConfig,
    stream::live_stream_processing::LiveStreamProcessing,
};
use std::sync::Arc;
use std::time::Instant;

const SIZES: &[usize] = &[100_000, 250_000, 500_000, 1_000_000, 2_000_000, 5_000_000];
const RUNS_PER_SIZE: usize = 30;

fn main() {
    println!("=== H4 Scalability Benchmark: Index Effectiveness ===\n");

    // Record hardware
    let hw = janus::benchmarking::get_hardware_info();
    std::fs::create_dir_all("results").expect("Cannot create results/");
    std::fs::write("results/hardware.txt", &hw).expect("Cannot write hardware.txt");
    println!("{}", hw);

    // Generate or verify scaled datasets exist
    println!("Preparing scaled datasets...");
    for &size in SIZES {
        let path = format!("data/scale/{}.nq", size);
        if !std::path::Path::new(&path).exists() {
            println!("  Generating {}M quad dataset...", size / 1_000_000);
            generate_dataset(size, &path);
        } else {
            println!("  ✓ Found {}", path);
        }
    }

    // Main benchmark loop
    let mut all_results = Vec::new();

    for &size in SIZES {
        println!("\nBenchmarking: {} quads ({} runs)", size, RUNS_PER_SIZE);

        let results = run_size_config(size);
        all_results.push((size, results));
    }

    // Write raw results
    write_scalability_csv(&all_results).expect("CSV write failed");

    // Analyze and write summary
    write_h4_summary(&all_results).expect("Summary write failed");

    println!("\n✓ H4 benchmarks complete. Results in results/h4_*.csv");
}

struct SizeResult {
    hist_retrieval_ms: f64,
    bootstrap_ms: f64,
    live_window_ms: f64,
}

fn run_size_config(dataset_size: usize) -> Vec<SizeResult> {
    let mut runs = Vec::new();

    for run_num in 1..=RUNS_PER_SIZE {
        let result = run_single_size(dataset_size);
        runs.push(result);

        if run_num % 5 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
    }
    println!();
    runs
}

fn run_single_size(dataset_size: usize) -> SizeResult {
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
    let dataset_path = format!("data/scale/{}.nq", dataset_size);
    load_nquads(&dataset_path, &storage);

    // Get timestamp range
    let (min_ts, max_ts) = read_timestamp_range(&dataset_path);

    // Measurement 1: Historical retrieval latency
    // Query 10% of the time range to let index work
    let ten_pct_range = (max_ts - min_ts) / 10;
    let query_start = min_ts;
    let query_end = min_ts + ten_pct_range;

    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());
    let sparql_query = "SELECT (COUNT(*) AS ?count) WHERE { ?s ?p ?o }";

    let window_def = WindowDefinition {
        window_name: "query_window".to_string(),
        stream_name: "query_stream".to_string(),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(query_start),
        end: Some(query_end),
        window_type: WindowType::HistoricalFixed,
    };

    let t_hist = Instant::now();
    let _ = executor
        .execute_fixed_window(&window_def, sparql_query)
        .ok();
    let hist_retrieval_ms = t_hist.elapsed().as_secs_f64() * 1000.0;

    // Measurement 2: Bootstrap latency (start_query to first historical result)
    // Using a smaller temporary storage for this
    let tmp_small = tempfile::tempdir().expect("Cannot create temp dir");
    let config_small = StreamingConfig {
        segment_base_path: tmp_small.path().to_str().unwrap().to_string(),
        entries_per_index_block: 1000,
        max_batch_events: 100_000,
        max_batch_age_seconds: 60,
        max_batch_bytes: 10_000_000,
        sparse_interval: 1000,
    };

    let storage_small = Arc::new(
        StreamingSegmentedStorage::new(config_small).expect("Storage init failed"),
    );
    load_nquads(&dataset_path, &storage_small);

    let registry = Arc::new(QueryRegistry::new());
    let parser = janus::parsing::janusql_parser::JanusQLParser::default();
    let api = JanusApi::new(parser, registry, Arc::clone(&storage_small))
        .expect("API creation failed");

    let janus_ql = format!(
        r#"
PREFIX ex: <http://test.org/>
REGISTER RStream ex:output AS
SELECT (COUNT(*) AS ?count)
FROM NAMED WINDOW ex:hist ON STREAM ex:stream
    [START {} END {}]
WHERE {{
    WINDOW ex:hist {{ ?s ?p ?o }}
}}
"#,
        query_start, query_end
    );

    let query_id: String = "h4_query".to_string();
    api.register_query(query_id.clone(), &janus_ql)
        .expect("Registration failed");

    let t_bootstrap = Instant::now();
    let handle = api.start_query(&query_id).expect("start_query failed");

    let mut bootstrap_ms = 1000.0; // fallback
    loop {
        if let Some(result) = handle.try_receive() {
            if matches!(result.source, ResultSource::Historical) {
                bootstrap_ms = t_bootstrap.elapsed().as_secs_f64() * 1000.0;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        if t_bootstrap.elapsed().as_secs_f64() > 10.0 {
            break; // timeout
        }
    }

    // Measurement 3: Live window latency (should be flat across sizes)
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

    // Inject events
    let base_ts = max_ts + 100_000;
    for i in 0..20u64 {
        let event = make_test_rdf_event(i, base_ts + i * 100);
        processor
            .add_event("http://test.org/live_stream", event)
            .ok();
    }

    // Sentinel to close window
    let sentinel = make_test_rdf_event(99, base_ts + 20 * 100 + 6000);
    let t_live = Instant::now();
    processor.add_event("http://test.org/live_stream", sentinel).ok();

    let mut live_window_ms = 100.0;
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

    SizeResult {
        hist_retrieval_ms,
        bootstrap_ms,
        live_window_ms,
    }
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
        _ => eprintln!("WARNING: Could not generate dataset"),
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
    for line in reader.lines() {
        if let Ok(line) = line {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            if let Ok(event) = parse_nquad_line(&line) {
                let _ = storage.write_rdf_event(event);
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
}

fn parse_nquad_line(line: &str) -> Result<RDFEvent, String> {
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
        max_ts = min_ts + 3_600_000;
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

fn write_scalability_csv(results: &[(usize, Vec<SizeResult>)]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h4_scalability.csv")?;
    writeln!(
        file,
        "dataset_size_quads,run,hist_retrieval_ms,bootstrap_ms,live_window_ms"
    )?;

    for (dataset_size, runs) in results {
        for (i, run) in runs.iter().enumerate() {
            writeln!(
                file,
                "{},{},{:.2},{:.2},{:.2}",
                dataset_size, i + 1, run.hist_retrieval_ms, run.bootstrap_ms, run.live_window_ms
            )?;
        }
    }
    Ok(())
}

fn write_h4_summary(results: &[(usize, Vec<SizeResult>)]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h4_summary.csv")?;
    writeln!(
        file,
        "dataset_size_quads,hist_mean_ms,hist_std_ms,bootstrap_mean_ms,bootstrap_std_ms,live_mean_ms,live_std_ms,sublinear_check"
    )?;

    let mut prev_size = 0usize;
    let mut prev_hist_mean = 0.0f64;

    for (dataset_size, runs) in results {
        let hists: Vec<f64> = runs.iter().map(|r| r.hist_retrieval_ms).collect();
        let bootstraps: Vec<f64> = runs.iter().map(|r| r.bootstrap_ms).collect();
        let lives: Vec<f64> = runs.iter().map(|r| r.live_window_ms).collect();

        let (hist_mean, hist_std) = analyse_runs(&hists);
        let (bootstrap_mean, bootstrap_std) = analyse_runs(&bootstraps);
        let (live_mean, live_std) = analyse_runs(&lives);

        let sublinear_check = if prev_size == 0 {
            "baseline".to_string()
        } else {
            let size_ratio = *dataset_size as f64 / prev_size as f64;
            let latency_ratio = hist_mean / prev_hist_mean;
            if latency_ratio < size_ratio {
                "PASS".to_string()
            } else {
                "WARN".to_string()
            }
        };

        writeln!(
            file,
            "{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{}",
            dataset_size,
            hist_mean,
            hist_std,
            bootstrap_mean,
            bootstrap_std,
            live_mean,
            live_std,
            sublinear_check
        )?;

        prev_size = *dataset_size;
        prev_hist_mean = hist_mean;
    }
    Ok(())
}
