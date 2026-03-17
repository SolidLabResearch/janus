/// H2 Benchmark: Anomaly Detection Correctness
///
/// Measures the accuracy and latency of detecting anomalies in a live stream
/// by comparing against a historical baseline through a unified Janus-QL query.
///
/// Run with: `cargo run --release --example h2_correctness_benchmark`

#[cfg(debug_assertions)]
compile_error!(
    "Benchmarks MUST be run with --release. Debug builds produce meaningless numbers. \
     Run: cargo run --release --example h2_correctness_benchmark"
);

use janus::{
    api::janus_api::{JanusApi, ResultSource},
    benchmarking::analyse_runs,
    core::RDFEvent,
    parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry,
    storage::segmented_storage::StreamingSegmentedStorage,
    storage::util::StreamingConfig,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

const SEEDS: usize = 5;
const ANOMALY_SPEC: &str = "data/anomalies/spec_20.json";
const HISTORICAL_DATA: &str = "data/citybench/historical_25min.nq";
const LIVE_DATA: &str = "data/citybench/live_5min.nq";

fn main() {
    println!("=== H2 Correctness Benchmark: Anomaly Detection ===\n");

    // Record hardware
    let hw = janus::benchmarking::get_hardware_info();
    std::fs::create_dir_all("results").expect("Cannot create results/");
    std::fs::write("results/hardware.txt", &hw).expect("Cannot write hardware.txt");
    println!("{}", hw);

    // Load anomaly spec for all seeds
    let spec = std::fs::read_to_string(ANOMALY_SPEC)
        .expect("Cannot read anomaly spec");
    let spec_json: Value = serde_json::from_str(&spec)
        .expect("Invalid JSON in spec");

    let mut all_detection_results = Vec::new();
    let mut all_summaries = Vec::new();

    // Run with different seeds
    for seed in 0..SEEDS {
        println!("\n--- Seed {} ---", seed);

        let live_path = format!("data/anomalies/live_seed_{}.nq", seed);
        let gt_path = format!("data/anomalies/gt_seed_{}.json", seed);

        // Inject anomalies with this seed
        inject_anomalies_with_seed(seed, &live_path, &gt_path);

        // Load ground truth
        let gt = std::fs::read_to_string(&gt_path)
            .expect("Cannot read ground truth");
        let gt_json: Value = serde_json::from_str(&gt)
            .expect("Invalid ground truth JSON");

        // Run H2 test
        let (detection_results, summary) = run_h2_single(seed, &gt_json);

        all_detection_results.extend(detection_results);
        all_summaries.push(summary);
    }

    // Write per-anomaly detection results
    write_detection_csv(&all_detection_results).expect("CSV write failed");

    // Write aggregate summary
    write_h2_summary(&all_summaries).expect("Summary write failed");

    println!("\n✓ H2 benchmarks complete. Results in results/h2_*.csv");
}

#[derive(Clone)]
struct DetectionResult {
    seed: usize,
    anomaly_id: String,
    anomaly_type: String,
    injection_ts: u64,
    detection_ts: Option<u64>,
    latency_ms: Option<u64>,
    step_ms: u64,
    within_step: bool,
    detected: bool,
}

#[derive(Clone)]
struct H2Summary {
    anomaly_type: String,
    detection_rate: f64,
    mean_latency_ms: f64,
    std_latency_ms: f64,
    within_step_rate: f64,
}

fn inject_anomalies_with_seed(seed: usize, output_path: &str, gt_path: &str) {
    let status = std::process::Command::new("python3")
        .args(&[
            "scripts/inject_anomalies.py",
            "--input",
            LIVE_DATA,
            "--spec",
            ANOMALY_SPEC,
            "--output",
            output_path,
            "--ground-truth",
            gt_path,
            "--seed",
            &seed.to_string(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => println!("  ✓ Injected anomalies for seed {}", seed),
        _ => eprintln!("WARNING: Could not inject anomalies for seed {}", seed),
    }
}

fn run_h2_single(seed: usize, ground_truth: &Value) -> (Vec<DetectionResult>, H2Summary) {
    // Create fresh storage
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
        StreamingSegmentedStorage::new(config).expect("Storage init failed"),
    );

    // Load historical data
    println!("  Loading historical data...");
    load_nquads(HISTORICAL_DATA, &storage);

    // Get timestamp range
    let (hist_min_ts, hist_max_ts) = read_timestamp_range(HISTORICAL_DATA);
    println!("  Historical range: {} to {}", hist_min_ts, hist_max_ts);

    // Create registry and parser
    let registry = Arc::new(QueryRegistry::new());
    let parser = JanusQLParser::default();
    let api = JanusApi::new(parser, registry, Arc::clone(&storage))
        .expect("API creation failed");

    // Register query
    let query_id: String = "h2_query".to_string();
    let janus_ql = format!(
        r#"
PREFIX ssn: <http://purl.oclc.org/NET/ssnx/ssn#>
PREFIX ex: <http://citybench.org/>
REGISTER RStream ex:output AS
SELECT (AVG(?val) AS ?avgVal) (COUNT(?val) AS ?count)
FROM NAMED WINDOW ex:hist ON STREAM ex:sensors
    [START {} END {}]
FROM NAMED WINDOW ex:live ON STREAM ex:sensors
    [RANGE 60000 STEP 30000]
WHERE {{
    WINDOW ex:hist {{ ?sensor ssn:hasValue ?val }}
    WINDOW ex:live {{ ?sensor ssn:hasValue ?val }}
}}
"#,
        hist_min_ts, hist_max_ts
    );

    api.register_query(query_id.clone(), &janus_ql)
        .expect("Registration failed");

    // Start query and measure bootstrap
    println!("  Starting query...");
    let t_start = Instant::now();
    let handle = api
        .start_query(&query_id)
        .expect("start_query failed");

    let mut hist_bootstrap_ms = 0.0;
    let mut hist_avg = 0.0;
    let mut hist_count = 0u64;

    // Wait for historical result
    loop {
        if let Some(result) = handle.try_receive() {
            if matches!(result.source, ResultSource::Historical) {
                hist_bootstrap_ms = t_start.elapsed().as_secs_f64() * 1000.0;
                // Extract avgVal from bindings
                if let Some(binding) = result.bindings.first() {
                    if let Some(avg_str) = binding.get("avgVal") {
                        hist_avg = avg_str.parse::<f64>().unwrap_or(0.0);
                    }
                    if let Some(count_str) = binding.get("count") {
                        hist_count = count_str.parse::<u64>().unwrap_or(0);
                    }
                }
                println!("  ✓ Bootstrap: {:.1}ms, hist_avg: {:.2}", hist_bootstrap_ms, hist_avg);
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Retrieve ground truth anomalies
    let anomalies: Vec<(String, String, u64)> = if let Some(arr) = ground_truth.get("anomalies").and_then(|v| v.as_array()) {
        arr.iter()
            .map(|a| {
                (
                    a.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    a.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    a.get("injection_timestamp").and_then(|v| v.as_u64()).unwrap_or(0),
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    // Replay live data with anomalies
    println!("  Replaying live stream...");
    let live_path = format!("data/anomalies/live_seed_{}.nq", seed);
    let mut detected: HashMap<String, (u64, u64)> = HashMap::new(); // anomaly_id -> (detection_ts, latency_ms)

    replay_live_stream(&live_path, &api, &query_id, &handle, hist_avg, &anomalies, &mut detected);

    // Compile results
    let mut detection_results = Vec::new();
    let window_step_ms = 30_000u64;

    for (anomaly_id, anomaly_type, injection_ts) in anomalies {
        let (detection_ts, latency_ms) = detected
            .get(&anomaly_id)
            .copied()
            .unwrap_or((0, 0));

        let detected = detection_ts > 0;
        let within_step = latency_ms <= window_step_ms;

        detection_results.push(DetectionResult {
            seed,
            anomaly_id,
            anomaly_type,
            injection_ts,
            detection_ts: if detected { Some(detection_ts) } else { None },
            latency_ms: if detected { Some(latency_ms) } else { None },
            step_ms: window_step_ms,
            within_step,
            detected,
        });
    }

    // Compute summary statistic for this seed
    let detected_count = detection_results.iter().filter(|r| r.detected).count();
    let detection_rate = detected_count as f64 / detection_results.len() as f64;

    let latencies: Vec<u64> = detection_results
        .iter()
        .filter_map(|r| r.latency_ms)
        .collect();
    let (mean_lat, std_lat) = if !latencies.is_empty() {
        let mean = latencies.iter().sum::<u64>() as f64 / latencies.len() as f64;
        let var = latencies.iter()
            .map(|&l| (l as f64 - mean).powi(2))
            .sum::<f64>() / latencies.len() as f64;
        (mean, var.sqrt())
    } else {
        (0.0, 0.0)
    };

    let within_step_count = detection_results.iter().filter(|r| r.within_step).count();
    let within_step_rate = within_step_count as f64 / detection_results.len() as f64;

    let summary = H2Summary {
        anomaly_type: "overall".to_string(),
        detection_rate,
        mean_latency_ms: mean_lat,
        std_latency_ms: std_lat,
        within_step_rate,
    };

    (detection_results, summary)
}

fn replay_live_stream(
    path: &str,
    api: &JanusApi,
    query_id: &str,
    handle: &janus::api::janus_api::QueryHandle,
    hist_avg: f64,
    anomalies: &[(String, String, u64)],
    detected: &mut HashMap<String, (u64, u64)>,
) {
    use std::io::{BufRead, BufReader};

    if let Ok(file) = std::fs::File::open(path) {
        let reader = BufReader::new(file);
        let mut event_count = 0;

        for line in reader.lines() {
            if let Ok(line) = line {
                if line.trim().is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Ok(event) = parse_nquad_line(&line) {
                    let _ = api.push_event(&query_id.to_string(), "http://citybench.org/sensors", event.clone());

                    // Poll for live results
                    if let Some(result) = handle.try_receive() {
                        if matches!(result.source, ResultSource::Live) {
                            if let Some(binding) = result.bindings.first() {
                                if let Some(avg_str) = binding.get("avgVal") {
                                    if let Ok(live_avg) = avg_str.parse::<f64>() {
                                        // Check for deviation >10%
                                        if is_deviation(live_avg, hist_avg) {
                                            // Try to match to an anomaly
                                            for (anom_id, _, injection_ts) in anomalies {
                                                if !detected.contains_key(anom_id) && event.timestamp >= *injection_ts {
                                                    let latency = event.timestamp.saturating_sub(*injection_ts);
                                                    detected.insert(anom_id.clone(), (event.timestamp, latency));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    event_count += 1;
                }
            }
        }
        println!("  ✓ Replayed {} events", event_count);
    }
}

fn is_deviation(live_avg: f64, hist_avg: f64) -> bool {
    if hist_avg == 0.0 {
        return false;
    }
    ((live_avg - hist_avg) / hist_avg).abs() > 0.10
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

// ============================================================================
// CSV Output

fn write_detection_csv(results: &[DetectionResult]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h2_detection.csv")?;
    writeln!(
        file,
        "seed,anomaly_id,anomaly_type,injection_ts,detection_ts,latency_ms,step_ms,within_step,detected"
    )?;

    for result in results {
        writeln!(
            file,
            "{},{},{},{},{},{},{},{},{}",
            result.seed,
            result.anomaly_id,
            result.anomaly_type,
            result.injection_ts,
            result.detection_ts.map_or("".to_string(), |t| t.to_string()),
            result.latency_ms.map_or("".to_string(), |t| t.to_string()),
            result.step_ms,
            if result.within_step { "true" } else { "false" },
            if result.detected { "true" } else { "false" },
        )?;
    }
    Ok(())
}

fn write_h2_summary(summaries: &[H2Summary]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("results/h2_summary.csv")?;
    writeln!(
        file,
        "anomaly_type,detection_rate,mean_latency_ms,std_latency_ms,within_step_rate"
    )?;

    let (mean_det_rate, mean_lat, mean_std_lat, mean_within): (f64, f64, f64, f64) = if !summaries.is_empty() {
        let n = summaries.len() as f64;
        (
            summaries.iter().map(|s| s.detection_rate).sum::<f64>() / n,
            summaries.iter().map(|s| s.mean_latency_ms).sum::<f64>() / n,
            summaries.iter().map(|s| s.std_latency_ms).sum::<f64>() / n,
            summaries.iter().map(|s| s.within_step_rate).sum::<f64>() / n,
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    writeln!(
        file,
        "overall,{:.2},{:.2},{:.2},{:.2}",
        mean_det_rate, mean_lat, mean_std_lat, mean_within
    )?;

    Ok(())
}
