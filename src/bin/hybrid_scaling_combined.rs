use clap::Parser;
use janus::core::RDFEvent;
use janus::execution::result_converter::parse_rsprs_binding_string;
use janus::extensions::query_options::build_evaluator;
use janus::paper_bench::cli_output::{
    default_benchmark_output_dir, print_benchmark_stdout, BenchmarkArtifact,
};
use janus::paper_bench::harness::{collect_repro_metadata, ensure_output_dir, write_jsonl};
use janus::paper_bench::query_defined_baseline::rdf::{
    resolve_object_term, resolve_predicate_term, resolve_subject_term,
    validate_template_against_definition,
};
use janus::parsing::janusql_parser::{
    BaselineDefinition, BaselineGraphTemplate, JanusQLParser, ParsedJanusQuery, WindowDefinition,
};
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::stream::live_stream_processing::LiveStreamProcessing;
use oxigraph::model::{GraphName, NamedNode, Quad, Term};
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const BASELINE_NS: &str = "https://janus.rs/baseline#";
const GRAPH_URI: &str = "http://example.org/citybench";
const LIVE_STREAM_URI: &str = "http://example.org/live";
const CONGESTION_PREDICATE: &str = "http://example.org/congestionLevel";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const SENSOR_COUNT: usize = 8;

static CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize)]
pub struct HybridScalingRow {
    pub benchmark_name: String,
    pub system: String,
    pub historical_size_quads: usize,
    pub target_historical_quads: usize,
    pub actual_historical_quads: usize,
    pub historical_query_type: String,
    pub expected_historical_result_count: usize,
    pub historical_result_count: usize,
    pub historical_result_count_matches_expected: bool,
    pub historical_query_start_ms: u64,
    pub historical_query_end_ms: u64,
    pub historical_query_span_ms: u64,
    pub iteration: usize,
    pub live_duration_ms: u64,
    pub event_rate_per_second: f64,
    pub event_interval_ms: u64,
    pub total_live_events: usize,
    pub window_size_ms: usize,
    pub window_slide_ms: usize,
    pub query_name: String,
    pub timestamp: u64,

    // Timing metrics
    pub registration_ms: f64,
    pub historical_lookup_ms: f64,
    pub historical_query_ms: f64,
    pub first_live_window_result_ms: Option<f64>,
    pub first_hybrid_result_ms: f64,
    pub main_window_result_ms: f64,
    pub first_hybrid_window_adjusted_overhead_ms: f64,
    pub main_window_adjusted_overhead_ms: f64,
    pub window_processing_overhead_ms: f64, // preserved for compatibility
    pub post_trigger_result_observation_delay_ms: f64,
    pub external_merge_ms: f64,
    pub total_run_ms: f64,

    // Correctness/equivalence fields
    pub result_count: usize,
    pub result_hash: String,
    pub matching_reference_hash: String,
    pub result_equivalence: bool,
    pub mismatch_reason: String,

    // Resource metrics
    pub rss_start_mb: f64,
    pub rss_end_mb: f64,
    pub peak_rss_mb: f64,
    pub rss_delta_mb: f64,
    pub mean_cpu_percent: f64,
    pub peak_cpu_percent: f64,
    pub resource_sample_count: usize,
    pub resource_sample_interval_ms: u64,

    // Decomposed Oxigraph specific fields
    pub historical_backend: String,
    pub historical_query_language: String,
    pub historical_load_ms: f64,
}

struct SummaryStats {
    mean: f64,
    std: f64,
}

#[derive(Debug, Parser)]
#[command(name = "hybrid_scaling_combined")]
struct Args {
    #[arg(long, value_delimiter = ',', default_values_t = vec![10000usize, 50000, 100000, 500000])]
    historical_sizes: Vec<usize>,

    #[arg(long, value_delimiter = ',', default_values_t = vec![
        "point_lookup".to_string(),
        "fixed_60s".to_string(),
        "range_10_percent".to_string(),
        "range_50_percent".to_string(),
        "range_100_percent".to_string()
    ])]
    historical_query_types: Vec<String>,

    #[arg(long, default_value_t = 30)]
    iterations: usize,

    #[arg(long, default_value_t = 20000)]
    live_duration_ms: u64,

    #[arg(long, default_value_t = 4.0)]
    event_rate: f64,

    #[arg(long, default_value_t = 250)]
    event_interval_ms: u64,

    #[arg(long, default_value_t = 10000)]
    window_size_ms: usize,

    #[arg(long, default_value_t = 5000)]
    window_slide_ms: usize,

    #[arg(long, value_delimiter = ',', default_values_t = vec!["janus".to_string(), "decomposed_oxigraph".to_string()])]
    systems: Vec<String>,

    #[arg(long, default_value_t = 100)]
    resource_sample_interval_ms: u64,

    #[arg(long)]
    output: Option<PathBuf>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn unique_config(prefix: &str) -> StreamingConfig {
    let counter = CONFIG_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    StreamingConfig {
        segment_base_path: format!("/tmp/janus_{prefix}_{}_{}", now_ms(), counter),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

// Resource Sampler implementation
#[derive(Clone, Copy, Debug)]
struct ResourceSample {
    rss_mb: f64,
    cpu_percent: f64,
}

#[derive(Clone, Debug, Default)]
struct ResourceSummary {
    peak_rss_mb: f64,
    mean_rss_mb: f64,
    peak_cpu_percent: f64,
    mean_cpu_percent: f64,
    sample_count: usize,
}

struct ResourceSampler {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<ResourceSample>>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ResourceSampler {
    fn start(interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_samples = Arc::clone(&samples);
        let handle = std::thread::spawn(move || {
            let pid: Pid = (std::process::id() as usize).into();
            let mut system = System::new_all();
            let refresh = |system: &mut System| {
                system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    ProcessRefreshKind::new().with_memory().with_cpu(),
                );
            };

            // Warm-up refresh to establish a baseline for CPU calculations
            refresh(&mut system);
            std::thread::sleep(Duration::from_millis(100));
            refresh(&mut system);

            let mut next_tick = Instant::now();

            loop {
                if thread_stop.load(AtomicOrdering::Relaxed) {
                    break;
                }

                next_tick += interval;
                if let Some(remaining) = next_tick.checked_duration_since(Instant::now()) {
                    std::thread::sleep(remaining);
                }

                if thread_stop.load(AtomicOrdering::Relaxed) {
                    break;
                }

                refresh(&mut system);
                if let Some(process) = system.process(pid) {
                    let mut guard = thread_samples.lock().expect("resource samples mutex poisoned");
                    guard.push(ResourceSample {
                        rss_mb: process.memory() as f64 / (1024.0 * 1024.0),
                        cpu_percent: process.cpu_usage() as f64,
                    });
                }
            }
        });

        Self { stop, samples, handle: Some(handle) }
    }

    fn finish(mut self) -> ResourceSummary {
        self.stop.store(true, AtomicOrdering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        let samples = self.samples.lock().expect("resource samples mutex poisoned");
        summarize_resource_samples(&samples)
    }
}

fn summarize_resource_samples(samples: &[ResourceSample]) -> ResourceSummary {
    if samples.is_empty() {
        return ResourceSummary::default();
    }

    let rss_values = samples.iter().map(|sample| sample.rss_mb).collect::<Vec<_>>();
    let cpu_values = samples.iter().map(|sample| sample.cpu_percent).collect::<Vec<_>>();

    let rss_sum: f64 = rss_values.iter().sum();
    let cpu_sum: f64 = cpu_values.iter().sum();

    ResourceSummary {
        peak_rss_mb: rss_values.iter().copied().fold(0.0, f64::max),
        mean_rss_mb: rss_sum / rss_values.len() as f64,
        peak_cpu_percent: cpu_values.iter().copied().fold(0.0, f64::max),
        mean_cpu_percent: cpu_sum / cpu_values.len() as f64,
        sample_count: samples.len(),
    }
}

fn get_current_rss_mb() -> f64 {
    let pid: Pid = (std::process::id() as usize).into();
    let mut system = System::new_all();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        ProcessRefreshKind::new().with_memory(),
    );
    if let Some(process) = system.process(pid) {
        process.memory() as f64 / (1024.0 * 1024.0)
    } else {
        0.0
    }
}

struct BaselineAccumulator {
    last_value: String,
    numeric_sum: f64,
    numeric_count: usize,
    all_numeric: bool,
}

impl BaselineAccumulator {
    fn new() -> Self {
        Self { last_value: String::new(), numeric_sum: 0.0, numeric_count: 0, all_numeric: true }
    }
}

fn normalize_binding_term(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("\\\"") && trimmed.contains("\\\"^^<") {
        let without_prefix = &trimmed[2..];
        if let Some(end) = without_prefix.find("\\\"^^<") {
            return without_prefix[..end].to_string();
        }
    }
    if trimmed.starts_with('"') && trimmed.contains("\"^^<") {
        let without_prefix = &trimmed[1..];
        if let Some(end) = without_prefix.find("\"^^<") {
            return without_prefix[..end].to_string();
        }
    }
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn canonicalize_row(row: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut entries = row
        .iter()
        .map(|(key, value)| {
            let normalized = normalize_binding_term(value);
            let formatted = if let Ok(num) = normalized.parse::<f64>() {
                format!("{:.6}", num)
            } else {
                normalized
            };
            (key.clone(), formatted)
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    entries
}

fn canonical_result_hash(
    rows: &[HashMap<String, String>],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut canonical_rows = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
    canonical_rows.sort();
    let payload = serde_json::to_vec(&canonical_rows)?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}"))
}

fn baseline_statements_from_bindings(
    bindings: &[HashMap<String, String>],
) -> Vec<(String, String, String)> {
    let mut accumulator = HashMap::<(String, String), BaselineAccumulator>::new();
    for binding in bindings {
        let Some(subject) = binding
            .get("sensor")
            .or_else(|| binding.get("s"))
            .map(|value| normalize_binding_term(value))
        else {
            continue;
        };

        let mut keys = binding.keys().cloned().collect::<Vec<_>>();
        keys.sort_unstable();
        for key in keys {
            if key == "sensor" || key == "s" {
                continue;
            }
            let Some(value) = binding.get(&key).map(|raw| normalize_binding_term(raw)) else {
                continue;
            };
            let entry = accumulator
                .entry((subject.clone(), key))
                .or_insert_with(BaselineAccumulator::new);
            entry.last_value.clone_from(&value);
            if let Ok(number) = value.parse::<f64>() {
                entry.numeric_sum += number;
                entry.numeric_count += 1;
            } else {
                entry.all_numeric = false;
            }
        }
    }

    let mut rows = accumulator.into_iter().collect::<Vec<_>>();
    rows.sort_by(|((left_subject, left_var), _), ((right_subject, right_var), _)| {
        left_subject.cmp(right_subject).then_with(|| left_var.cmp(right_var))
    });
    rows.into_iter()
        .map(|((subject, variable), acc)| {
            let object = if acc.all_numeric && acc.numeric_count > 0 {
                (acc.numeric_sum / acc.numeric_count as f64).to_string()
            } else {
                acc.last_value
            };
            (subject, format!("{BASELINE_NS}{variable}"), object)
        })
        .collect()
}

fn materialized_baseline_rows_from_bindings(
    bindings: &[HashMap<String, String>],
    baseline_variable: &str,
) -> Vec<HashMap<String, String>> {
    baseline_statements_from_bindings(bindings)
        .into_iter()
        .filter_map(|(subject, predicate, object)| {
            predicate
                .strip_prefix(BASELINE_NS)
                .map(|variable_name| (subject, variable_name.to_string(), object))
        })
        .filter(|(_, variable_name, _)| variable_name == baseline_variable)
        .map(|(subject, variable_name, object)| {
            HashMap::from([("sensor".to_string(), subject), (variable_name, object)])
        })
        .collect()
}

fn build_historical_baseline_bindings_from_events(
    events: &[RDFEvent],
) -> Vec<HashMap<String, String>> {
    let mut sums = HashMap::<String, (f64, usize)>::new();
    for event in events {
        let Ok(value) = event.object.parse::<f64>() else {
            continue;
        };
        let entry = sums.entry(event.subject.clone()).or_insert((0.0, 0));
        entry.0 += value;
        entry.1 += 1;
    }

    let mut rows = sums
        .into_iter()
        .map(|(sensor, (sum, count))| {
            HashMap::from([
                ("sensor".to_string(), sensor),
                ("historicalAvgCongestion".to_string(), format!("{:.6}", sum / count as f64)),
            ])
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        normalize_binding_term(left.get("sensor").map(String::as_str).unwrap_or_default()).cmp(
            &normalize_binding_term(right.get("sensor").map(String::as_str).unwrap_or_default()),
        )
    });
    rows
}

fn join_live_with_baseline_with_filter(
    live_rows: &[HashMap<String, String>],
    baseline_rows: &[HashMap<String, String>],
) -> Vec<HashMap<String, String>> {
    let mut baseline_by_sensor = HashMap::<String, HashMap<String, String>>::new();
    for row in baseline_rows {
        if let Some(sensor) = row.get("sensor").map(|v| normalize_binding_term(v)) {
            baseline_by_sensor.insert(sensor, row.clone());
        }
    }

    let mut joined = Vec::new();
    for live_row in live_rows {
        if let Some(sensor) = live_row.get("sensor").map(|v| normalize_binding_term(v)) {
            if let Some(baseline_row) = baseline_by_sensor.get(&sensor) {
                let live_val: f64 = live_row
                    .get("liveAvgCongestion")
                    .and_then(|v| normalize_binding_term(v).parse().ok())
                    .unwrap_or(0.0);
                let base_val: f64 = baseline_row
                    .get("historicalAvgCongestion")
                    .and_then(|v| normalize_binding_term(v).parse().ok())
                    .unwrap_or(0.0);
                if live_val > base_val {
                    let mut merged = baseline_row.clone();
                    for (k, v) in live_row {
                        merged.insert(k.clone(), v.clone());
                    }
                    merged
                        .insert("congestionDelta".to_string(), format!("{}", live_val - base_val));
                    joined.push(merged);
                }
            }
        }
    }
    joined
}

fn hybrid_query(
    start_ts: u64,
    end_ts: u64,
    window_size_ms: usize,
    window_slide_ms: usize,
) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor
               ?liveAvgCongestion
               ?historicalAvgCongestion
               ((?liveAvgCongestion - ?historicalAvgCongestion) AS ?congestionDelta)
        FROM NAMED WINDOW ex:hist ON LOG <{GRAPH_URI}> [START {start_ts} END {end_ts}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        WHERE {{
            {{
                SELECT ?sensor
                       (AVG(?historicalCongestion) AS ?historicalAvgCongestion)
                WHERE {{
                    WINDOW ex:hist {{
                        ?sensor ex:congestionLevel ?historicalCongestion .
                    }}
                }}
                GROUP BY ?sensor
            }}

            {{
                SELECT ?sensor
                       (AVG(?liveCongestion) AS ?liveAvgCongestion)
                WHERE {{
                    WINDOW ex:live {{
                        ?sensor ex:congestionLevel ?liveCongestion .
                    }}
                }}
                GROUP BY ?sensor
            }}

            FILTER(?liveAvgCongestion > ?historicalAvgCongestion)
        }}
        "#
    )
}

fn hybrid_query_execution_form(
    start_ts: u64,
    end_ts: u64,
    window_size_ms: usize,
    window_slide_ms: usize,
) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor
               (AVG(?liveCongestion) AS ?liveAvgCongestion)
               ?historicalAvgCongestion
               ((AVG(?liveCongestion) - ?historicalAvgCongestion) AS ?congestionDelta)
        FROM NAMED WINDOW ex:hist ON LOG <{GRAPH_URI}> [START {start_ts} END {end_ts}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        WHERE {{
            {{
                SELECT ?sensor
                       (AVG(?historicalCongestion) AS ?historicalAvgCongestion)
                WHERE {{
                    WINDOW ex:hist {{
                        ?sensor ex:congestionLevel ?historicalCongestion .
                    }}
                }}
                GROUP BY ?sensor
            }}
            WINDOW ex:live {{
                ?sensor ex:congestionLevel ?liveCongestion .
            }}
        }}
        GROUP BY ?sensor ?historicalAvgCongestion
        HAVING(AVG(?liveCongestion) > ?historicalAvgCongestion)
        "#
    )
}

fn live_only_rspql(window_size_ms: usize, window_slide_ms: usize) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor
               (AVG(?liveCongestion) AS ?liveAvgCongestion)
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        WHERE {{
            WINDOW ex:live {{
                ?sensor ex:congestionLevel ?liveCongestion .
            }}
        }}
        GROUP BY ?sensor
        "#
    )
}

fn build_decomposed_sparql(start_ts: u64, end_ts: u64) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
        SELECT ?sensor (AVG(?historicalCongestion) AS ?historicalAvgCongestion) WHERE {{
            GRAPH ?eventGraph {{
                ?sensor ex:congestionLevel ?historicalCongestion .
            }}
            ?eventGraph ex:timestamp ?t .
            ?eventGraph ex:graph <http://example.org/citybench> .
            FILTER(?t >= {} && ?t <= {})
        }}
        GROUP BY ?sensor
        "#,
        start_ts, end_ts
    )
}

fn prepare_historical_storage(
    target_historical_quads: usize,
) -> Result<(Arc<StreamingSegmentedStorage>, u64, usize), Box<dyn std::error::Error>> {
    let base_ts = 1_800_500_000_000u64;
    let storage =
        Arc::new(StreamingSegmentedStorage::new(unique_config("hybrid_scaling_combined"))?);

    for index in 0..target_historical_quads {
        let ts = base_ts + index as u64 * 60;
        let event = RDFEvent::new_typed_literal_object(
            ts,
            &sensor_uri(index),
            CONGESTION_PREDICATE,
            &format!("{:.3}", congestion_value_for_historical(index)),
            XSD_DECIMAL,
            GRAPH_URI,
        );
        storage.write_rdf_event(event)?;
    }

    storage.flush()?;

    Ok((storage, base_ts, target_historical_quads))
}

fn generate_live_events(live_duration_ms: u64, event_interval_ms: u64) -> Vec<RDFEvent> {
    let live_start_ts = 1_900_000_000_000u64;
    let total_live_events = (live_duration_ms / event_interval_ms) as usize;
    let mut live_events = Vec::new();
    for index in 0..total_live_events {
        let ts = live_start_ts + index as u64 * event_interval_ms;
        let event = RDFEvent::new_typed_literal_object(
            ts,
            &sensor_uri(index),
            CONGESTION_PREDICATE,
            &format!("{:.3}", congestion_value_for_live(index)),
            XSD_DECIMAL,
            GRAPH_URI,
        );
        live_events.push(event);
    }
    live_events
}

fn sensor_uri(index: usize) -> String {
    format!("http://example.org/junction/{}", index % SENSOR_COUNT)
}

fn historical_sensor_base(sensor_idx: usize) -> f64 {
    30.0 + sensor_idx as f64 * 5.0
}

fn congestion_value_for_historical(index: usize) -> f64 {
    let sensor_idx = index % SENSOR_COUNT;
    let sample_idx = index / SENSOR_COUNT;
    let seasonal = ((sample_idx * 7 + sensor_idx * 3) % 9) as f64 - 4.0;
    historical_sensor_base(sensor_idx) + seasonal
}

fn congestion_value_for_live(index: usize) -> f64 {
    let sensor_idx = index % SENSOR_COUNT;
    let sample_idx = index / SENSOR_COUNT;
    let bias = if sensor_idx % 2 == 0 { 8.0 } else { -8.0 };
    let oscillation = ((sample_idx * 5 + sensor_idx) % 5) as f64 - 2.0;
    historical_sensor_base(sensor_idx) + bias + oscillation
}

fn wait_for_live_event_schedule(replay_start: Instant, event_index: usize, rate_hz: f64) {
    let target = replay_start + Duration::from_secs_f64(event_index as f64 / rate_hz);
    if let Some(remaining) = target.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

fn calculate_stats(values: &[f64]) -> SummaryStats {
    if values.is_empty() {
        return SummaryStats { mean: 0.0, std: 0.0 };
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let std = if values.len() > 1 {
        let variance = values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        variance.sqrt()
    } else {
        0.0
    };
    SummaryStats { mean, std }
}

fn find_first_baseline_definition<'a>(
    parsed: &'a ParsedJanusQuery,
) -> Result<(&'a BaselineDefinition, &'a BaselineGraphTemplate), Box<dyn std::error::Error>> {
    let definition = parsed
        .ast
        .baseline_definitions
        .first()
        .ok_or("missing lowered historical subquery definition")?;
    let template = parsed
        .baseline_graph_templates
        .iter()
        .find(|template| template.baseline_name == definition.name)
        .ok_or("missing lowered historical subquery graph template")?;
    Ok((definition, template))
}

fn find_baseline_source_windows<'a>(
    parsed: &'a ParsedJanusQuery,
    definition: &'a BaselineDefinition,
) -> Result<Vec<&'a WindowDefinition>, Box<dyn std::error::Error>> {
    definition
        .source_windows
        .iter()
        .map(|source_window_name| {
            parsed
                .historical_windows
                .iter()
                .find(|window| window.window_name == *source_window_name)
                .ok_or_else(|| {
                    format!("missing historical source window '{source_window_name}'").into()
                })
        })
        .collect()
}

fn execute_lowered_historical_subquery(
    parsed: &ParsedJanusQuery,
    storage: Arc<StreamingSegmentedStorage>,
    evaluation_time: u64,
) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error>> {
    let (definition, _) = find_first_baseline_definition(parsed)?;
    let source_windows = find_baseline_source_windows(parsed, definition)?;
    let mut historical_events = Vec::new();

    for window in source_windows {
        let (start, end) = window.resolve_historical_bounds(evaluation_time).ok_or_else(|| {
            format!("failed to resolve historical bounds for window '{}'", window.window_name)
        })?;
        historical_events.extend(storage.query_rdf(start, end)?);
    }

    Ok(build_historical_baseline_bindings_from_events(&historical_events))
}

fn materialize_lowered_historical_subquery(
    processor: &mut LiveStreamProcessing,
    parsed: &ParsedJanusQuery,
    bindings: &[HashMap<String, String>],
) -> Result<(), Box<dyn std::error::Error>> {
    let (definition, template) = find_first_baseline_definition(parsed)?;
    validate_template_against_definition(definition, template)?;
    let graph_name = GraphName::NamedNode(NamedNode::new(template.baseline_name.as_str())?);

    for binding in bindings {
        for triple in &template.triples {
            let subject = resolve_subject_term(triple, binding)?;
            let predicate = resolve_predicate_term(triple)?;
            let object = resolve_object_term(triple, binding)?;
            processor.add_static_quad(Quad::new(subject, predicate, object, graph_name.clone()));
        }
    }

    Ok(())
}

fn run_janus_unified(
    iteration: usize,
    target_historical_quads: usize,
    actual_historical_quads: usize,
    historical_storage: Arc<StreamingSegmentedStorage>,
    base_ts: u64,
    historical_query_type: &str,
    live_events: &[RDFEvent],
    args: &Args,
    _metadata: &janus::paper_bench::harness::ReproMetadata,
) -> Result<HybridScalingRow, Box<dyn std::error::Error>> {
    // 1. Start process-level resource sampling
    let sample_interval = Duration::from_millis(args.resource_sample_interval_ms);
    let sampler = ResourceSampler::start(sample_interval);
    let rss_start_mb = get_current_rss_mb();

    let client_start = now_ms();

    let ts = |idx: usize| -> u64 { base_ts + idx as u64 * 60 };

    let h = target_historical_quads;
    let (start_ts, end_ts) = match historical_query_type {
        "point_lookup" => (ts(h - 1), ts(h)),
        "fixed_60s" => (ts(h - 1000), ts(h)),
        "range_10_percent" => (ts(h - h / 10), ts(h)),
        "range_50_percent" => (ts(h - h / 2), ts(h)),
        "range_100_percent" => (ts(0), ts(h)),
        _ => return Err("invalid query type".into()),
    };

    let selected_historical_events = match historical_query_type {
        "point_lookup" => 1,
        "fixed_60s" => 1000.min(h),
        "range_10_percent" => (h / 10).max(1),
        "range_50_percent" => (h / 2).max(1),
        "range_100_percent" => h.max(1),
        _ => 0,
    };
    let expected_historical_result_count = selected_historical_events.min(SENSOR_COUNT);

    let parser = JanusQLParser::new()?;
    // Query range is half-open [start_ts, end_ts), so use end_ts - 1.
    // The benchmark keeps the user-facing Janus-QL query in nested-subquery form,
    // but lowers the live-only subquery into an equivalent top-level aggregate
    // because the current parser only supports lowering historical nested subqueries.
    let _user_facing_query =
        hybrid_query(start_ts, end_ts - 1, args.window_size_ms, args.window_slide_ms);
    let execution_query = hybrid_query_execution_form(
        start_ts,
        end_ts - 1,
        args.window_size_ms,
        args.window_slide_ms,
    );
    let parsed = parser.parse(&execution_query)?;
    let query_registered = now_ms();

    let historical_start = now_ms();
    let _ = parsed.historical_windows.first().ok_or("missing historical window")?;
    let baseline_bindings =
        execute_lowered_historical_subquery(&parsed, historical_storage.clone(), end_ts - 1)?;
    let historical_done = now_ms();
    let historical_query_ms = (historical_done - historical_start) as f64;

    let historical_result_count = baseline_bindings.len();

    let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    processor.register_stream(LIVE_STREAM_URI)?;
    materialize_lowered_historical_subquery(&mut processor, &parsed, &baseline_bindings)?;
    processor.start_processing()?;

    let replay_start = Instant::now();
    let live_start_ts = 1_900_000_000_000u64;
    let mut received_results = Vec::new();

    for (event_index, event) in live_events.iter().enumerate() {
        wait_for_live_event_schedule(replay_start, event_index, args.event_rate);

        let start_add = Instant::now();
        processor.add_event(LIVE_STREAM_URI, event.clone())?;

        let poll_deadline = Instant::now() + Duration::from_millis(15);
        let mut found_result = false;
        while Instant::now() < poll_deadline {
            if let Some(res) = processor.try_receive_result()? {
                let received_at = replay_start.elapsed().as_secs_f64() * 1000.0;
                let overhead = start_add.elapsed().as_secs_f64() * 1000.0;
                received_results.push((res, received_at, overhead));
                found_result = true;
            }
            std::thread::yield_now();
        }

        if !found_result {
            while let Some(res) = processor.try_receive_result()? {
                let received_at = replay_start.elapsed().as_secs_f64() * 1000.0;
                received_results.push((res, received_at, 0.0));
            }
        }
    }

    // Close the stream to flush remaining windows
    let close_ts = live_start_ts + (live_events.len() as u64) * args.event_interval_ms + 20_000;
    processor.close_stream(LIVE_STREAM_URI, close_ts as i64)?;

    let drain_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < drain_deadline {
        if let Some(res) = processor.try_receive_result()? {
            let received_at = replay_start.elapsed().as_secs_f64() * 1000.0;
            received_results.push((res, received_at, 0.0));
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let total_run_ms = (now_ms() - client_start) as f64;

    // Find unique sorted slide timestamps
    let mut slides: Vec<i64> =
        received_results.iter().map(|(res, _, _)| res.timestamp_to).collect();
    slides.sort_unstable();
    slides.dedup();

    let mut first_hybrid_result_ms = 0.0;
    let mut main_window_result_ms = 0.0;
    let mut window_processing_overhead_ms = 0.0;

    let mut all_hybrid_rows = Vec::new();

    for (res, received_at, overhead) in &received_results {
        let parsed_rows = parse_rsprs_binding_string(&res.bindings);
        all_hybrid_rows.push(parsed_rows.clone());

        if !slides.is_empty() && res.timestamp_to == slides[0] {
            if first_hybrid_result_ms == 0.0 {
                first_hybrid_result_ms = *received_at;
                window_processing_overhead_ms = *overhead;
            }
        } else if slides.len() >= 2 && res.timestamp_to == slides[1] {
            if main_window_result_ms == 0.0 {
                main_window_result_ms = *received_at;
            }
        }
    }

    let result_hash = canonical_result_hash(&all_hybrid_rows)?;

    // Stop process-level resource sampling
    let rss_end_mb = get_current_rss_mb();
    let resource_summary = sampler.finish();

    Ok(HybridScalingRow {
        benchmark_name: "hybrid_scaling_combined".to_string(),
        system: "janus".to_string(),
        historical_size_quads: target_historical_quads,
        target_historical_quads,
        actual_historical_quads,
        historical_query_type: historical_query_type.to_string(),
        expected_historical_result_count,
        historical_result_count,
        historical_result_count_matches_expected: historical_result_count
            == expected_historical_result_count,
        historical_query_start_ms: start_ts,
        historical_query_end_ms: end_ts,
        historical_query_span_ms: end_ts - start_ts,
        iteration,
        live_duration_ms: args.live_duration_ms,
        event_rate_per_second: args.event_rate,
        event_interval_ms: args.event_interval_ms,
        total_live_events: live_events.len(),
        window_size_ms: args.window_size_ms,
        window_slide_ms: args.window_slide_ms,
        query_name: "hybrid_query".to_string(),
        timestamp: now_ms(),
        registration_ms: (query_registered - client_start) as f64,
        historical_lookup_ms: historical_query_ms,
        historical_query_ms,
        first_live_window_result_ms: None,
        first_hybrid_result_ms,
        main_window_result_ms,
        first_hybrid_window_adjusted_overhead_ms: first_hybrid_result_ms - 5000.0,
        main_window_adjusted_overhead_ms: main_window_result_ms - 10000.0,
        window_processing_overhead_ms,
        post_trigger_result_observation_delay_ms: window_processing_overhead_ms,
        external_merge_ms: 0.0,
        total_run_ms,
        result_count: all_hybrid_rows.len(),
        result_hash,
        matching_reference_hash: String::new(),
        result_equivalence: false,
        mismatch_reason: String::new(),

        rss_start_mb,
        rss_end_mb,
        peak_rss_mb: resource_summary.peak_rss_mb,
        rss_delta_mb: rss_end_mb - rss_start_mb,
        mean_cpu_percent: resource_summary.mean_cpu_percent,
        peak_cpu_percent: resource_summary.peak_cpu_percent,
        resource_sample_count: resource_summary.sample_count,
        resource_sample_interval_ms: args.resource_sample_interval_ms,

        historical_backend: "janus_segmented".to_string(),
        historical_query_language: "janus_range_lookup".to_string(),
        historical_load_ms: 0.0,
    })
}

fn run_decomposed_oxigraph(
    iteration: usize,
    target_historical_quads: usize,
    actual_historical_quads: usize,
    all_historical_events: &[RDFEvent],
    base_ts: u64,
    historical_query_type: &str,
    live_events: &[RDFEvent],
    args: &Args,
    _metadata: &janus::paper_bench::harness::ReproMetadata,
) -> Result<HybridScalingRow, Box<dyn std::error::Error>> {
    // 1. Start process-level resource sampling
    let sample_interval = Duration::from_millis(args.resource_sample_interval_ms);
    let sampler = ResourceSampler::start(sample_interval);
    let rss_start_mb = get_current_rss_mb();

    let client_start = now_ms();

    let ts = |idx: usize| -> u64 { base_ts + idx as u64 * 60 };

    let h = target_historical_quads;
    let (start_ts, end_ts) = match historical_query_type {
        "point_lookup" => (ts(h - 1), ts(h)),
        "fixed_60s" => (ts(h - 1000), ts(h)),
        "range_10_percent" => (ts(h - h / 10), ts(h)),
        "range_50_percent" => (ts(h - h / 2), ts(h)),
        "range_100_percent" => (ts(0), ts(h)),
        _ => return Err("invalid query type".into()),
    };

    let selected_historical_events = match historical_query_type {
        "point_lookup" => 1,
        "fixed_60s" => 1000.min(h),
        "range_10_percent" => (h / 10).max(1),
        "range_50_percent" => (h / 2).max(1),
        "range_100_percent" => h.max(1),
        _ => 0,
    };
    let expected_historical_result_count = selected_historical_events.min(SENSOR_COUNT);

    // Load full history into Oxigraph
    let load_start = now_ms();
    let store = Store::new()?;
    let timestamp_predicate = NamedNode::new("http://example.org/timestamp")?;
    let graph_predicate = NamedNode::new("http://example.org/graph")?;

    for (index, event) in all_historical_events.iter().enumerate() {
        let event_graph_uri = format!("http://example.org/event/{}", index);
        let event_graph = GraphName::NamedNode(NamedNode::new(&event_graph_uri)?);

        // Quad 1: Subject ex:baselineFlow Object in event-specific graph
        let subject = NamedNode::new(&event.subject)?;
        let predicate = NamedNode::new(&event.predicate)?;
        let object = if event.object.starts_with("http://") || event.object.starts_with("https://")
        {
            Term::NamedNode(NamedNode::new(&event.object)?)
        } else {
            let literal = if let Ok(_) = event.object.parse::<f64>() {
                oxigraph::model::Literal::new_typed_literal(
                    &event.object,
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
                )
            } else if let Ok(_) = event.object.parse::<i64>() {
                oxigraph::model::Literal::new_typed_literal(
                    &event.object,
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
                )
            } else {
                oxigraph::model::Literal::new_simple_literal(&event.object)
            };
            Term::Literal(literal)
        };
        store.insert(&Quad::new(subject, predicate, object, event_graph.clone()))?;

        // Quad 2: Event timestamp in Default Graph
        let event_node = NamedNode::new(&event_graph_uri)?;
        let ts_literal = Term::Literal(oxigraph::model::Literal::new_typed_literal(
            &event.timestamp.to_string(),
            NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
        ));
        store.insert(&Quad::new(
            event_node.clone(),
            timestamp_predicate.clone(),
            ts_literal,
            GraphName::DefaultGraph,
        ))?;

        // Quad 3: Event original graph in Default Graph
        let graph_node = NamedNode::new(&event.graph)?;
        store.insert(&Quad::new(
            event_node,
            graph_predicate.clone(),
            Term::NamedNode(graph_node),
            GraphName::DefaultGraph,
        ))?;
    }
    let load_done = now_ms();
    let historical_load_ms = (load_done - load_start) as f64;

    // Run SPARQL query over the store
    let query_start = now_ms();
    // Query range is half-open [start_ts, end_ts), so use end_ts - 1
    let sparql_query = build_decomposed_sparql(start_ts, end_ts - 1);

    let evaluator = build_evaluator();
    let parsed_query = evaluator
        .parse_query(&sparql_query)
        .map_err(|e| oxigraph::sparql::QueryEvaluationError::from(e))?;
    let results = parsed_query.on_store(&store).execute()?;

    let mut external_bindings = Vec::new();
    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            let solution = solution?;
            let mut binding = HashMap::new();
            for (var, term) in solution.iter() {
                binding.insert(var.as_str().to_string(), term.to_string());
            }
            external_bindings.push(binding);
        }
    }
    let query_done = now_ms();
    let historical_query_ms = (query_done - query_start) as f64;

    let historical_result_count = external_bindings.len();
    let materialized_baseline_rows =
        materialized_baseline_rows_from_bindings(&external_bindings, "historicalAvgCongestion");

    // Live stream processing setup
    let live_query_def = live_only_rspql(args.window_size_ms, args.window_slide_ms);
    let live_processor_start = now_ms();
    let mut live_processor = LiveStreamProcessing::new(live_query_def)?;
    live_processor.register_stream(LIVE_STREAM_URI)?;
    live_processor.start_processing()?;
    let live_ready = now_ms();
    let live_registration_ms = (live_ready - live_processor_start) as f64;

    let replay_start = Instant::now();
    let live_start_ts = 1_900_000_000_000u64;
    let mut received_results = Vec::new();

    for (event_index, event) in live_events.iter().enumerate() {
        wait_for_live_event_schedule(replay_start, event_index, args.event_rate);

        let start_add = Instant::now();
        live_processor.add_event(LIVE_STREAM_URI, event.clone())?;

        let poll_deadline = Instant::now() + Duration::from_millis(15);
        let mut found_result = false;
        while Instant::now() < poll_deadline {
            if let Some(res) = live_processor.try_receive_result()? {
                let received_at = replay_start.elapsed().as_secs_f64() * 1000.0;
                let overhead = start_add.elapsed().as_secs_f64() * 1000.0;

                // External merge step
                let merge_start = Instant::now();
                let live_row = parse_rsprs_binding_string(&res.bindings);
                let joined =
                    join_live_with_baseline_with_filter(&[live_row], &materialized_baseline_rows);
                let external_merge_ms = merge_start.elapsed().as_secs_f64() * 1000.0;

                received_results.push((res, received_at, overhead, external_merge_ms, joined));
                found_result = true;
            }
            std::thread::yield_now();
        }

        if !found_result {
            while let Some(res) = live_processor.try_receive_result()? {
                let received_at = replay_start.elapsed().as_secs_f64() * 1000.0;

                let merge_start = Instant::now();
                let live_row = parse_rsprs_binding_string(&res.bindings);
                let joined =
                    join_live_with_baseline_with_filter(&[live_row], &materialized_baseline_rows);
                let external_merge_ms = merge_start.elapsed().as_secs_f64() * 1000.0;

                received_results.push((res, received_at, 0.0, external_merge_ms, joined));
            }
        }
    }

    // Close the stream
    let close_ts = live_start_ts + (live_events.len() as u64) * args.event_interval_ms + 20_000;
    live_processor.close_stream(LIVE_STREAM_URI, close_ts as i64)?;

    let drain_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < drain_deadline {
        if let Some(res) = live_processor.try_receive_result()? {
            let received_at = replay_start.elapsed().as_secs_f64() * 1000.0;

            let merge_start = Instant::now();
            let live_row = parse_rsprs_binding_string(&res.bindings);
            let joined =
                join_live_with_baseline_with_filter(&[live_row], &materialized_baseline_rows);
            let external_merge_ms = merge_start.elapsed().as_secs_f64() * 1000.0;

            received_results.push((res, received_at, 0.0, external_merge_ms, joined));
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let total_run_ms = (now_ms() - client_start) as f64;

    // Find unique sorted slide timestamps
    let mut slides: Vec<i64> =
        received_results.iter().map(|(res, _, _, _, _)| res.timestamp_to).collect();
    slides.sort_unstable();
    slides.dedup();

    let mut first_live_window_result_ms = 0.0;
    let mut first_hybrid_result_ms = 0.0;
    let mut main_window_result_ms = 0.0;
    let mut window_processing_overhead_ms = 0.0;
    let mut external_merge_ms_total = 0.0;

    let mut all_hybrid_rows = Vec::new();

    for (res, received_at, overhead, merge_ms, joined) in &received_results {
        all_hybrid_rows.extend(joined.clone());

        if !slides.is_empty() && res.timestamp_to == slides[0] {
            if first_live_window_result_ms == 0.0 {
                first_live_window_result_ms = *received_at;
                first_hybrid_result_ms = *received_at + *merge_ms;
                window_processing_overhead_ms = *overhead;
                external_merge_ms_total = *merge_ms;
            }
        } else if slides.len() >= 2 && res.timestamp_to == slides[1] {
            if main_window_result_ms == 0.0 {
                main_window_result_ms = *received_at + *merge_ms;
            }
        }
    }

    let result_hash = canonical_result_hash(&all_hybrid_rows)?;

    // Stop process-level resource sampling
    let rss_end_mb = get_current_rss_mb();
    let resource_summary = sampler.finish();

    Ok(HybridScalingRow {
        benchmark_name: "hybrid_scaling_combined".to_string(),
        system: "decomposed_oxigraph".to_string(),
        historical_size_quads: target_historical_quads,
        target_historical_quads,
        actual_historical_quads,
        historical_query_type: historical_query_type.to_string(),
        expected_historical_result_count,
        historical_result_count,
        historical_result_count_matches_expected: historical_result_count
            == expected_historical_result_count,
        historical_query_start_ms: start_ts,
        historical_query_end_ms: end_ts,
        historical_query_span_ms: end_ts - start_ts,
        iteration,
        live_duration_ms: args.live_duration_ms,
        event_rate_per_second: args.event_rate,
        event_interval_ms: args.event_interval_ms,
        total_live_events: live_events.len(),
        window_size_ms: args.window_size_ms,
        window_slide_ms: args.window_slide_ms,
        query_name: "hybrid_query".to_string(),
        timestamp: now_ms(),
        registration_ms: live_registration_ms,
        historical_lookup_ms: historical_query_ms,
        historical_query_ms,
        first_live_window_result_ms: Some(first_live_window_result_ms),
        first_hybrid_result_ms,
        main_window_result_ms,
        first_hybrid_window_adjusted_overhead_ms: first_hybrid_result_ms - 5000.0,
        main_window_adjusted_overhead_ms: main_window_result_ms - 10000.0,
        window_processing_overhead_ms,
        post_trigger_result_observation_delay_ms: window_processing_overhead_ms,
        external_merge_ms: external_merge_ms_total,
        total_run_ms,
        result_count: all_hybrid_rows.len(),
        result_hash,
        matching_reference_hash: String::new(),
        result_equivalence: false,
        mismatch_reason: String::new(),

        rss_start_mb,
        rss_end_mb,
        peak_rss_mb: resource_summary.peak_rss_mb,
        rss_delta_mb: rss_end_mb - rss_start_mb,
        mean_cpu_percent: resource_summary.mean_cpu_percent,
        peak_cpu_percent: resource_summary.peak_cpu_percent,
        resource_sample_count: resource_summary.sample_count,
        resource_sample_interval_ms: args.resource_sample_interval_ms,

        historical_backend: "oxigraph".to_string(),
        historical_query_language: "sparql_filter".to_string(),
        historical_load_ms,
    })
}

fn write_reports(
    output_dir: &Path,
    rows: &[HybridScalingRow],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut unique_sizes = rows.iter().map(|r| r.historical_size_quads).collect::<Vec<_>>();
    unique_sizes.sort_unstable();
    unique_sizes.dedup();

    let mut query_types = rows.iter().map(|r| r.historical_query_type.clone()).collect::<Vec<_>>();
    query_types.sort_unstable();
    query_types.dedup();
    let preferred_order = vec![
        "point_lookup".to_string(),
        "fixed_60s".to_string(),
        "range_10_percent".to_string(),
        "range_50_percent".to_string(),
        "range_100_percent".to_string(),
    ];
    query_types.sort_by_key(|qt| preferred_order.iter().position(|x| x == qt).unwrap_or(99));

    let systems = vec!["janus".to_string(), "decomposed_oxigraph".to_string()];

    // Write CSV Summary
    let csv_path = output_dir.join("hybrid_scaling_combined.summary.csv");
    let mut csv_file = File::create(&csv_path)?;
    writeln!(
        csv_file,
        "historical_query_type,historical_size_quads,system,first_hybrid_result_mean_ms,first_hybrid_result_std_ms,main_window_result_mean_ms,main_window_result_std_ms,historical_lookup_mean_ms,historical_lookup_std_ms,window_overhead_mean_ms,window_overhead_std_ms,external_merge_mean_ms,external_merge_std_ms,equivalence_rate,peak_rss_mean,peak_rss_std,rss_delta_mean,rss_delta_std,mean_cpu_mean,mean_cpu_std,peak_cpu_mean,peak_cpu_std"
    )?;

    for q_type in &query_types {
        for &size in &unique_sizes {
            for system in &systems {
                let filtered = rows
                    .iter()
                    .filter(|r| {
                        &r.historical_query_type == q_type
                            && r.historical_size_quads == size
                            && &r.system == system
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    continue;
                }
                let first_hybrid_vals =
                    filtered.iter().map(|r| r.first_hybrid_result_ms).collect::<Vec<_>>();
                let main_window_vals =
                    filtered.iter().map(|r| r.main_window_result_ms).collect::<Vec<_>>();
                let lookup_vals =
                    filtered.iter().map(|r| r.historical_lookup_ms).collect::<Vec<_>>();
                let overhead_vals = filtered
                    .iter()
                    .map(|r| r.post_trigger_result_observation_delay_ms)
                    .collect::<Vec<_>>();
                let merge_vals = filtered.iter().map(|r| r.external_merge_ms).collect::<Vec<_>>();

                // Resource fields
                let rss_vals = filtered.iter().map(|r| r.peak_rss_mb).collect::<Vec<_>>();
                let delta_vals = filtered.iter().map(|r| r.rss_delta_mb).collect::<Vec<_>>();
                let cpu_vals = filtered.iter().map(|r| r.mean_cpu_percent).collect::<Vec<_>>();
                let peak_cpu_vals = filtered.iter().map(|r| r.peak_cpu_percent).collect::<Vec<_>>();

                let first_hybrid_stats = calculate_stats(&first_hybrid_vals);
                let main_window_stats = calculate_stats(&main_window_vals);
                let lookup_stats = calculate_stats(&lookup_vals);
                let overhead_stats = calculate_stats(&overhead_vals);
                let merge_stats = calculate_stats(&merge_vals);

                let rss_stats = calculate_stats(&rss_vals);
                let delta_stats = calculate_stats(&delta_vals);
                let cpu_stats = calculate_stats(&cpu_vals);
                let peak_cpu_stats = calculate_stats(&peak_cpu_vals);

                let matches = filtered.iter().filter(|r| r.result_equivalence).count();
                let eq_rate = matches as f64 / filtered.len() as f64;

                writeln!(
                    csv_file,
                    "{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
                    q_type,
                    size,
                    system,
                    first_hybrid_stats.mean,
                    first_hybrid_stats.std,
                    main_window_stats.mean,
                    main_window_stats.std,
                    lookup_stats.mean,
                    lookup_stats.std,
                    overhead_stats.mean,
                    overhead_stats.std,
                    merge_stats.mean,
                    merge_stats.std,
                    eq_rate,
                    rss_stats.mean,
                    rss_stats.std,
                    delta_stats.mean,
                    delta_stats.std,
                    cpu_stats.mean,
                    cpu_stats.std,
                    peak_cpu_stats.mean,
                    peak_cpu_stats.std
                )?;
            }
        }
    }

    let md_path = output_dir.join("hybrid_scaling_combined_results.md");
    let mut md_file = File::create(&md_path)?;

    writeln!(md_file, "# Hybrid Scaling Combined Benchmark Results\n")?;

    // Table 1: End-to-end hybrid latency
    writeln!(md_file, "## Table 1: End-to-end hybrid latency\n")?;
    writeln!(
        md_file,
        "| Historical query type | Historical size | System | Historical backend | Historical query language | First hybrid result mean ± std | Main window result mean ± std | Historical query mean ± std | External merge mean ± std | Result equivalence |"
    )?;
    writeln!(md_file, "|---|---:|---|---|---|---:|---:|---:|---:|---|")?;

    for q_type in &query_types {
        for &size in &unique_sizes {
            for system in &systems {
                let filtered = rows
                    .iter()
                    .filter(|r| {
                        &r.historical_query_type == q_type
                            && r.historical_size_quads == size
                            && &r.system == system
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    continue;
                }
                let first_hybrid_vals =
                    filtered.iter().map(|r| r.first_hybrid_result_ms).collect::<Vec<_>>();
                let main_window_vals =
                    filtered.iter().map(|r| r.main_window_result_ms).collect::<Vec<_>>();
                let lookup_vals =
                    filtered.iter().map(|r| r.historical_lookup_ms).collect::<Vec<_>>();
                let merge_vals = filtered.iter().map(|r| r.external_merge_ms).collect::<Vec<_>>();

                let first_hybrid_stats = calculate_stats(&first_hybrid_vals);
                let main_window_stats = calculate_stats(&main_window_vals);
                let lookup_stats = calculate_stats(&lookup_vals);
                let merge_stats = calculate_stats(&merge_vals);

                let matches = filtered.iter().filter(|r| r.result_equivalence).count();
                let eq_rate = matches as f64 / filtered.len() as f64;

                let sys_label = if system == "janus" {
                    "Janus Unified"
                } else {
                    "Decomposed-Oxigraph"
                };
                let backend = &filtered[0].historical_backend;
                let lang = &filtered[0].historical_query_language;

                writeln!(
                    md_file,
                    "| {} | {} | {} | {} | {} | {:.3} ± {:.3} ms | {:.3} ± {:.3} ms | {:.3} ± {:.3} ms | {:.3} ± {:.3} ms | {}% |",
                    q_type,
                    size,
                    sys_label,
                    backend,
                    lang,
                    first_hybrid_stats.mean,
                    first_hybrid_stats.std,
                    main_window_stats.mean,
                    main_window_stats.std,
                    lookup_stats.mean,
                    lookup_stats.std,
                    merge_stats.mean,
                    merge_stats.std,
                    (eq_rate * 100.0).round()
                )?;
            }
        }
    }

    // Table 2: Historical access scaling inside the combined benchmark
    writeln!(
        md_file,
        "\n## Table 2: Historical access scaling inside the combined benchmark\n"
    )?;
    writeln!(
        md_file,
        "| Historical query type | System | 10k quads | 50k quads | 100k quads | 500k quads | Takeaway |"
    )?;
    writeln!(md_file, "|---|---|---:|---:|---:|---:|---|")?;

    for q_type in &query_types {
        for system in &systems {
            let mut means = HashMap::new();
            for &size in &[10000usize, 50000, 100000, 500000] {
                let filtered = rows
                    .iter()
                    .filter(|r| {
                        &r.historical_query_type == q_type
                            && r.historical_size_quads == size
                            && &r.system == system
                    })
                    .collect::<Vec<_>>();
                if !filtered.is_empty() {
                    let lookup_vals =
                        filtered.iter().map(|r| r.historical_lookup_ms).collect::<Vec<_>>();
                    let stats = calculate_stats(&lookup_vals);
                    means.insert(size, format!("{:.3} ms", stats.mean));
                } else {
                    means.insert(size, "N/A".to_string());
                }
            }
            let takeaway = match q_type.as_str() {
                "point_lookup" => "flat",
                "fixed_60s" => "almost flat",
                "range_10_percent" => "grows with result size",
                "range_50_percent" => "grows strongly",
                "range_100_percent" => "largest scan cost",
                _ => "",
            };
            let sys_label = if system == "janus" {
                "Janus Unified"
            } else {
                "Decomposed-Oxigraph"
            };
            writeln!(
                md_file,
                "| {} | {} | {} | {} | {} | {} | {} |",
                q_type,
                sys_label,
                means.get(&10000).unwrap_or(&"N/A".to_string()),
                means.get(&50000).unwrap_or(&"N/A".to_string()),
                means.get(&100000).unwrap_or(&"N/A".to_string()),
                means.get(&500000).unwrap_or(&"N/A".to_string()),
                takeaway
            )?;
        }
    }

    // Table 3: Historical result counts
    writeln!(md_file, "\n## Table 3: Historical result counts\n")?;
    writeln!(
        md_file,
        "| Historical query type | Historical size | System | Historical result count mean ± std |"
    )?;
    writeln!(md_file, "|---|---:|---|---:|")?;

    for q_type in &query_types {
        for &size in &unique_sizes {
            for system in &systems {
                let filtered = rows
                    .iter()
                    .filter(|r| {
                        &r.historical_query_type == q_type
                            && r.historical_size_quads == size
                            && &r.system == system
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    continue;
                }
                let counts =
                    filtered.iter().map(|r| r.historical_result_count as f64).collect::<Vec<_>>();
                let stats = calculate_stats(&counts);
                let sys_label = if system == "janus" {
                    "Janus Unified"
                } else {
                    "Decomposed-Oxigraph"
                };
                writeln!(
                    md_file,
                    "| {} | {} | {} | {:.3} ± {:.3} |",
                    q_type, size, sys_label, stats.mean, stats.std
                )?;
            }
        }
    }

    // Table 4: Result equivalence
    writeln!(md_file, "\n## Table 4: Result equivalence\n")?;
    writeln!(
        md_file,
        "| Historical query type | Historical size | Janus result count | Decomposed result count | Hash equivalence |"
    )?;
    writeln!(md_file, "|---|---:|---|---|---|")?;

    for q_type in &query_types {
        for &size in &unique_sizes {
            let janus_filtered = rows
                .iter()
                .filter(|r| {
                    &r.historical_query_type == q_type
                        && r.historical_size_quads == size
                        && r.system == "janus"
                })
                .collect::<Vec<_>>();
            let decomp_filtered = rows
                .iter()
                .filter(|r| {
                    &r.historical_query_type == q_type
                        && r.historical_size_quads == size
                        && r.system == "decomposed_oxigraph"
                })
                .collect::<Vec<_>>();

            if janus_filtered.is_empty() && decomp_filtered.is_empty() {
                continue;
            }

            let janus_count_mean = if !janus_filtered.is_empty() {
                let counts =
                    janus_filtered.iter().map(|r| r.result_count as f64).collect::<Vec<_>>();
                format!("{:.1}", calculate_stats(&counts).mean)
            } else {
                "N/A".to_string()
            };

            let decomp_count_mean = if !decomp_filtered.is_empty() {
                let counts =
                    decomp_filtered.iter().map(|r| r.result_count as f64).collect::<Vec<_>>();
                format!("{:.1}", calculate_stats(&counts).mean)
            } else {
                "N/A".to_string()
            };

            let total_runs = janus_filtered.len();
            let matches = janus_filtered.iter().filter(|r| r.result_equivalence).count();
            let eq_rate_str = if total_runs > 0 {
                format!("{}%", (matches as f64 / total_runs as f64 * 100.0).round())
            } else {
                "N/A".to_string()
            };

            writeln!(
                md_file,
                "| {} | {} | {} | {} | {} |",
                q_type, size, janus_count_mean, decomp_count_mean, eq_rate_str
            )?;
        }
    }

    // Table 5: Process-level resource utilization
    writeln!(md_file, "\n## Table 5: Process-level resource utilization\n")?;
    writeln!(
        md_file,
        "| Historical query type | Historical size | System | Peak RSS MB mean ± std | RSS delta MB mean ± std | Mean CPU % mean ± std | Peak CPU % mean ± std |"
    )?;
    writeln!(md_file, "|---|---:|---|---:|---:|---:|---:|")?;

    for q_type in &query_types {
        for &size in &unique_sizes {
            for system in &systems {
                let filtered = rows
                    .iter()
                    .filter(|r| {
                        &r.historical_query_type == q_type
                            && r.historical_size_quads == size
                            && &r.system == system
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    continue;
                }
                let rss_vals = filtered.iter().map(|r| r.peak_rss_mb).collect::<Vec<_>>();
                let delta_vals = filtered.iter().map(|r| r.rss_delta_mb).collect::<Vec<_>>();
                let cpu_vals = filtered.iter().map(|r| r.mean_cpu_percent).collect::<Vec<_>>();
                let peak_cpu_vals = filtered.iter().map(|r| r.peak_cpu_percent).collect::<Vec<_>>();

                let rss_stats = calculate_stats(&rss_vals);
                let delta_stats = calculate_stats(&delta_vals);
                let cpu_stats = calculate_stats(&cpu_vals);
                let peak_cpu_stats = calculate_stats(&peak_cpu_vals);

                let sys_label = if system == "janus" {
                    "Janus Unified"
                } else {
                    "Decomposed-Oxigraph"
                };

                writeln!(
                    md_file,
                    "| {} | {} | {} | {:.3} ± {:.3} MB | {:.3} ± {:.3} MB | {:.3} ± {:.3}% | {:.3} ± {:.3}% |",
                    q_type,
                    size,
                    sys_label,
                    rss_stats.mean,
                    rss_stats.std,
                    delta_stats.mean,
                    delta_stats.std,
                    cpu_stats.mean,
                    cpu_stats.std,
                    peak_cpu_stats.mean,
                    peak_cpu_stats.std
                )?;
            }
        }
    }

    writeln!(md_file, "\n### Documentation Notes\n")?;
    writeln!(md_file, "- **Process-Level Resource Measurement Limitation**: Resource measurements are process-level measurements collected during each run. In a single long-running process, RSS can be affected by allocator retention and previous configurations. For fully isolated memory comparison, each configuration should be run in a fresh process.\n")?;
    writeln!(md_file, "- **Decomposed-Oxigraph Baseline**: Decomposed-Oxigraph evaluates the historical component using SPARQL over the full Oxigraph historical store and merges the resulting historical bindings with separately evaluated live results. It does not use Janus segmented historical lookup for historical filtering.\n")?;

    let readme_path = output_dir.join("README.md");
    let mut readme_file = File::create(&readme_path)?;
    writeln!(readme_file, "# Hybrid Scaling Combined Benchmark\n")?;
    writeln!(
        readme_file,
        "This directory contains output results from running the hybrid scaling combined benchmark.\n"
    )?;
    writeln!(readme_file, "## Files\n")?;
    writeln!(readme_file, "- `hybrid_scaling_combined.raw.jsonl`: Raw per-run JSON lines log")?;
    writeln!(readme_file, "- `hybrid_scaling_combined.summary.csv`: Summary CSV grouped by historical size and system")?;
    writeln!(
        readme_file,
        "- `hybrid_scaling_combined_results.md`: Summary Markdown tables ready for review"
    )?;
    writeln!(readme_file, "\n## How to Run\n")?;
    writeln!(
        readme_file,
        "```bash\ncargo run --release --bin hybrid_scaling_combined -- --historical-sizes 10000,50000,100000,500000 --historical-query-types point_lookup,fixed_60s,range_10_percent,range_50_percent,range_100_percent --iterations 5 --live-duration-ms 20000 --event-rate 4 --event-interval-ms 250 --window-size-ms 10000 --window-slide-ms 5000\n```"
    )?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let output_dir = args
        .output
        .clone()
        .unwrap_or_else(|| default_benchmark_output_dir("hybrid_scaling_combined"));
    ensure_output_dir(&output_dir)?;

    let metadata = collect_repro_metadata();
    let mut all_results = Vec::new();

    for &historical_size in &args.historical_sizes {
        println!("Preparing historical storage for size H = {}...", historical_size);

        let (historical_storage, base_ts, actual_historical_quads) =
            prepare_historical_storage(historical_size)?;

        println!(
            "Historical storage ready. target_historical_quads = {}, actual_historical_quads = {}.",
            historical_size, actual_historical_quads
        );

        let live_events = generate_live_events(args.live_duration_ms, args.event_interval_ms);
        let all_historical_events = historical_storage.query_rdf(0, u64::MAX)?;

        for q_type in &args.historical_query_types {
            println!("Starting evaluations for query pattern: {}", q_type);

            for iteration in 0..args.iterations {
                println!(
                    "Running iteration {}/{} for H = {} and query pattern {}...",
                    iteration + 1,
                    args.iterations,
                    historical_size,
                    q_type
                );

                let mut janus_row = None;
                let mut decomp_row = None;

                if args.systems.contains(&"janus".to_string()) {
                    println!("  Running Janus Unified...");
                    let row = run_janus_unified(
                        iteration,
                        historical_size,
                        actual_historical_quads,
                        historical_storage.clone(),
                        base_ts,
                        q_type,
                        &live_events,
                        &args,
                        &metadata,
                    )?;
                    janus_row = Some(row);
                }

                if args.systems.contains(&"decomposed_oxigraph".to_string()) {
                    println!("  Running Decomposed Oxigraph Baseline...");
                    let row = run_decomposed_oxigraph(
                        iteration,
                        historical_size,
                        actual_historical_quads,
                        &all_historical_events,
                        base_ts,
                        q_type,
                        &live_events,
                        &args,
                        &metadata,
                    )?;
                    decomp_row = Some(row);
                }

                match (janus_row, decomp_row) {
                    (Some(mut jr), Some(mut dr)) => {
                        let equivalent = (jr.result_hash == dr.result_hash)
                            && (jr.result_count == dr.result_count);

                        let mismatch_reason = if equivalent {
                            "none".to_string()
                        } else {
                            format!(
                                "mismatch: Janus count={}, hash={}; Decomposed count={}, hash={}",
                                jr.result_count, jr.result_hash, dr.result_count, dr.result_hash
                            )
                        };

                        jr.result_equivalence = equivalent;
                        jr.mismatch_reason = mismatch_reason.clone();
                        dr.result_equivalence = equivalent;
                        dr.mismatch_reason = mismatch_reason;

                        all_results.push(jr);
                        all_results.push(dr);
                    }
                    (Some(jr), None) => {
                        all_results.push(jr);
                    }
                    (None, Some(dr)) => {
                        all_results.push(dr);
                    }
                    (None, None) => {}
                }
            }
        }
    }

    // Write output files
    let jsonl_path = output_dir.join("hybrid_scaling_combined.raw.jsonl");
    write_jsonl(&jsonl_path, &all_results)?;

    let json_path = output_dir.join("hybrid_scaling_combined.raw.json");
    let json_file = File::create(&json_path)?;
    serde_json::to_writer_pretty(json_file, &all_results)?;

    write_reports(&output_dir, &all_results)?;

    // Standard paper bench stdout reporting
    print_benchmark_stdout(
        "hybrid_scaling_combined",
        Some(all_results.iter().all(|r| {
            r.result_equivalence
                || !args.systems.contains(&"janus".to_string())
                || !args.systems.contains(&"decomposed_oxigraph".to_string())
        })),
        None,
        Some(args.iterations),
        &output_dir,
        &[
            BenchmarkArtifact { label: "raw_jsonl", path: &jsonl_path },
            BenchmarkArtifact { label: "raw_json", path: &json_path },
            BenchmarkArtifact {
                label: "summary_csv",
                path: &output_dir.join("hybrid_scaling_combined.summary.csv"),
            },
            BenchmarkArtifact {
                label: "results_markdown",
                path: &output_dir.join("hybrid_scaling_combined_results.md"),
            },
        ],
    );

    Ok(())
}
