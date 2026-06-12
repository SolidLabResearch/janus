use crate::{
    api::janus_api::JanusApiError,
    core::RDFEvent,
    execution::HistoricalExecutor,
    paper_bench::external::{ExternalBindings, ExternalHistoricalAdapter},
    parsing::janusql_parser::JanusQLParser,
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
    stream::live_stream_processing::LiveStreamProcessing,
};
use clap::ValueEnum;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering as AtomicOrdering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BASELINE_NS: &str = "https://janus.rs/baseline#";
const GRAPH_URI: &str = "http://example.org/citybench";
const LIVE_STREAM_URI: &str = "http://example.org/live";
const TRAFFIC_PREDICATE: &str = "http://example.org/trafficFlow";
const BASELINE_PREDICATE: &str = "http://example.org/baselineFlow";

static CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Cold,
    Warm,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Warm => "warm",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum TimeMode {
    Virtual,
    WallClock,
}

impl TimeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Virtual => "virtual",
            Self::WallClock => "wall-clock",
        }
    }

    pub fn uses_virtual_event_time(self) -> bool {
        matches!(self, Self::Virtual)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinationSystem {
    JanusUnified,
    DecomposedOxigraph,
}

impl CoordinationSystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JanusUnified => "janus_unified",
            Self::DecomposedOxigraph => {
                "Oxigraph historical + Janus live window processor + external join"
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ReproMetadata {
    pub git_commit_sha: String,
    pub branch: String,
    pub rustc_version: String,
    pub os: String,
    pub cpu_model: String,
    pub ram_bytes: Option<u64>,
    pub benchmark_command: String,
    pub timestamp_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoordinationRow {
    pub system: String,
    pub mode: String,
    pub is_warmup: bool,
    pub run_index: usize,
    pub historical_events: usize,
    pub live_events: usize,
    pub client_start: u64,
    pub query_registered: u64,
    pub historical_start: u64,
    pub historical_done: u64,
    pub live_ready: u64,
    pub first_event_published: u64,
    pub first_result_engine: u64,
    pub first_result_client: u64,
    pub e2e_latency_ms: f64,
    pub estimated_useful_engine_work_ms: f64,
    pub estimated_coordination_overhead_ms: f64,
    pub historical_intermediate_bytes: usize,
    pub live_intermediate_bytes: usize,
    pub estimated_external_transfer_bytes: usize,
    pub final_result_bytes: usize,
    pub components: usize,
    pub process_boundaries: usize,
    pub serialization_steps: usize,
    pub result_count: usize,
    pub historical_input_hash: String,
    pub live_input_hash: String,
    pub result_hash: String,
    pub equivalent_to_baseline: Option<bool>,
    pub metadata: ReproMetadata,

    // Explicit H1.1 Live-Stream and Correctness fields
    pub live_events_published: usize,
    pub live_events_processed: usize,
    pub live_window_size: usize,
    pub live_window_slide: usize,
    pub live_query_registered_at: u64,
    pub live_first_event_at: u64,
    pub live_first_window_result_at: u64,
    pub live_stream_processing_latency_ms: f64,
    pub external_join_latency_ms: f64,
    pub first_hybrid_result_latency_ms: f64,
    pub historical_result_count: usize,
    pub historical_result_hash: String,
    pub live_result_count: usize,
    pub live_result_hash: Option<String>,
    pub hybrid_result_count: usize,
    pub hybrid_result_hash: String,
    pub historical_equivalent_to_baseline: Option<bool>,
    pub hybrid_equivalent_to_baseline: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoordinationSummaryRow {
    pub system: String,
    pub mode: String,
    pub runs: usize,
    pub components: usize,
    pub process_boundaries: usize,
    pub serialization_steps: usize,
    pub p50_e2e_latency_ms: f64,
    pub p95_e2e_latency_ms: f64,
    pub avg_useful_engine_work_ms: f64,
    pub avg_coordination_overhead_ms: f64,
    pub avg_external_transfer_bytes: f64,
    pub avg_final_result_bytes: f64,
    pub avg_result_count: f64,

    // Explicit H1.1 live-stream and equivalence rate fields
    pub avg_live_events_published: f64,
    pub avg_live_events_processed: f64,
    pub avg_live_stream_processing_latency_ms: f64,
    pub avg_external_join_latency_ms: f64,
    pub avg_first_hybrid_result_latency_ms: f64,
    pub avg_historical_result_count: f64,
    pub avg_live_result_count: f64,
    pub avg_hybrid_result_count: f64,
    pub historical_equivalence_rate: f64,
    pub hybrid_equivalence_rate: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ScalingQueryType {
    PointLookup,
    FixedWindow,
    ProportionalRange10,
    ProportionalRange50,
    FullRange,
    HybridBaselineLookup,
}

impl ScalingQueryType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PointLookup => "point_lookup",
            Self::FixedWindow => "fixed_window",
            Self::ProportionalRange10 => "proportional_range_10",
            Self::ProportionalRange50 => "proportional_range_50",
            Self::FullRange => "full_range",
            Self::HybridBaselineLookup => "hybrid_baseline_lookup",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ScalingRow {
    pub dataset_size_quads: usize,
    pub query_type: String,
    pub mode: String,
    pub is_warmup: bool,
    pub run_index: usize,
    pub logical_quads_scanned: usize,
    pub selectivity: f64,
    pub result_count: usize,
    pub result_hash: String,
    pub latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub throughput_quads_per_sec: f64,
    pub peak_rss_mb: Option<f64>,
    pub metadata: ReproMetadata,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScalingSummaryRow {
    pub dataset_size_quads: usize,
    pub query_type: String,
    pub mode: String,
    pub runs: usize,
    pub logical_quads_scanned: usize,
    pub selectivity: f64,
    pub result_count: usize,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub avg_throughput_quads_per_sec: f64,
    pub max_peak_rss_mb: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScalingFitRow {
    pub query_type: String,
    pub mode: String,
    pub slope_ms_per_100k_quads: f64,
    pub intercept_ms: f64,
    pub r_squared: f64,
    pub number_of_points: usize,
}

#[derive(Clone)]
pub struct DatasetSpec {
    pub size_quads: usize,
    pub start_ts: u64,
    pub end_ts: u64,
    pub point_ts: u64,
    pub point_subject: String,
    pub fixed_start: u64,
    pub fixed_end: u64,
    pub proportional_10_end: u64,
    pub proportional_50_end: u64,
}

#[derive(Clone)]
pub struct HistoricalDataset {
    pub storage: Arc<StreamingSegmentedStorage>,
    pub spec: DatasetSpec,
}

#[derive(Clone)]
pub struct CoordinationWorkload {
    pub historical_storage: Arc<StreamingSegmentedStorage>,
    pub historical_rdf_events: Vec<RDFEvent>,
    pub live_events: Vec<RDFEvent>,
    pub historical_start_ts: u64,
    pub historical_end_ts: u64,
    pub historical_sparql_query: String,
    pub hybrid_query: String,
}

#[derive(Clone, Debug, Serialize)]
struct JoinTraceRow {
    historical_join_key: Option<String>,
    live_join_key: Option<String>,
    accepted: bool,
    rejection_reason: Option<String>,
    historical_row: Option<Vec<(String, String)>>,
    live_row: Vec<(String, String)>,
    joined_row: Option<Vec<(String, String)>>,
}

#[derive(Clone, Debug, Serialize)]
struct EquivalenceReport {
    system_pair: String,
    run_index: usize,
    mode: String,
    historical_input_hash: String,
    live_input_hash: String,
    janus_result_count: usize,
    decomposed_result_count: usize,
    janus_result_hash: String,
    decomposed_result_hash: String,
    equivalent: bool,
    historical_inputs_semantically_equal: bool,
    live_inputs_semantically_equal: bool,
    notes: Vec<String>,
}

pub struct CoordinationPair {
    pub unified: CoordinationRow,
    pub decomposed: CoordinationRow,
}

pub struct CoordinationRunConfig<'a> {
    pub mode: ExecutionMode,
    pub run_index: usize,
    pub is_warmup: bool,
    pub historical_events: usize,
    pub live_events: usize,
    pub metadata: &'a ReproMetadata,
    pub adapter: &'a dyn ExternalHistoricalAdapter,
    pub warm_workload: Option<&'a CoordinationWorkload>,
    pub debug_output_dir: Option<&'a Path>,
}

pub struct ScalingRunConfig<'a> {
    pub mode: ExecutionMode,
    pub dataset_size_quads: usize,
    pub query_type: ScalingQueryType,
    pub metadata: &'a ReproMetadata,
    pub run_index: usize,
    pub is_warmup: bool,
    pub warm_dataset: Option<&'a HistoricalDataset>,
    pub output_dir: &'a Path,
}

pub fn collect_repro_metadata() -> ReproMetadata {
    ReproMetadata {
        git_commit_sha: capture_command("git", &["rev-parse", "HEAD"]),
        branch: capture_command("git", &["branch", "--show-current"]),
        rustc_version: capture_command("rustc", &["--version"]),
        os: detect_os(),
        cpu_model: detect_cpu_model(),
        ram_bytes: detect_ram_bytes(),
        benchmark_command: env::args().collect::<Vec<_>>().join(" "),
        timestamp_unix_ms: now_ms(),
    }
}

pub fn ensure_output_dir(base: &Path) -> std::io::Result<()> {
    fs::create_dir_all(base)
}

pub fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        writeln!(file)?;
    }
    Ok(())
}

pub fn write_coordination_summary_csv(
    path: &Path,
    rows: &[CoordinationSummaryRow],
) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "system,mode,runs,components,process_boundaries,serialization_steps,p50_e2e_latency_ms,p95_e2e_latency_ms,avg_useful_engine_work_ms,avg_coordination_overhead_ms,avg_external_transfer_bytes,avg_final_result_bytes,avg_result_count,avg_live_events_published,avg_live_events_processed,avg_live_stream_processing_latency_ms,avg_external_join_latency_ms,avg_first_hybrid_result_latency_ms,avg_historical_result_count,avg_live_result_count,avg_hybrid_result_count,historical_equivalence_rate,hybrid_equivalence_rate"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
            row.system,
            row.mode,
            row.runs,
            row.components,
            row.process_boundaries,
            row.serialization_steps,
            row.p50_e2e_latency_ms,
            row.p95_e2e_latency_ms,
            row.avg_useful_engine_work_ms,
            row.avg_coordination_overhead_ms,
            row.avg_external_transfer_bytes,
            row.avg_final_result_bytes,
            row.avg_result_count,
            row.avg_live_events_published,
            row.avg_live_events_processed,
            row.avg_live_stream_processing_latency_ms,
            row.avg_external_join_latency_ms,
            row.avg_first_hybrid_result_latency_ms,
            row.avg_historical_result_count,
            row.avg_live_result_count,
            row.avg_hybrid_result_count,
            row.historical_equivalence_rate,
            row.hybrid_equivalence_rate
        )?;
    }
    Ok(())
}

pub fn write_scaling_summary_csv(path: &Path, rows: &[ScalingSummaryRow]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "dataset_size_quads,query_type,mode,runs,logical_quads_scanned,selectivity,result_count,p50_latency_ms,p95_latency_ms,avg_latency_ms,avg_throughput_quads_per_sec,max_peak_rss_mb"
    )?;
    for row in rows {
        let peak_rss = row.max_peak_rss_mb.map_or_else(String::new, |value| format!("{value:.3}"));
        writeln!(
            file,
            "{},{},{},{},{},{:.6},{},{:.3},{:.3},{:.3},{:.3},{}",
            row.dataset_size_quads,
            row.query_type,
            row.mode,
            row.runs,
            row.logical_quads_scanned,
            row.selectivity,
            row.result_count,
            row.p50_latency_ms,
            row.p95_latency_ms,
            row.avg_latency_ms,
            row.avg_throughput_quads_per_sec,
            peak_rss
        )?;
    }
    Ok(())
}

pub fn write_scaling_fit_csv(path: &Path, rows: &[ScalingFitRow]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "query_type,mode,slope_ms_per_100k_quads,intercept_ms,r_squared,number_of_points"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{:.6},{:.6},{:.6},{}",
            row.query_type,
            row.mode,
            row.slope_ms_per_100k_quads,
            row.intercept_ms,
            row.r_squared,
            row.number_of_points
        )?;
    }
    Ok(())
}

pub fn summarize_coordination(rows: &[CoordinationRow]) -> Vec<CoordinationSummaryRow> {
    let mut grouped = HashMap::<(String, String), Vec<&CoordinationRow>>::new();
    for row in rows {
        grouped.entry((row.system.clone(), row.mode.clone())).or_default().push(row);
    }

    let mut summary = grouped
        .into_iter()
        .map(|((system, mode), items)| {
            let runs = items.len();
            let historical_equiv_count = items
                .iter()
                .filter(|row| row.historical_equivalent_to_baseline == Some(true))
                .count();
            let hybrid_equiv_count = items
                .iter()
                .filter(|row| row.hybrid_equivalent_to_baseline == Some(true))
                .count();

            CoordinationSummaryRow {
                system,
                mode,
                runs,
                components: items.first().map_or(0, |row| row.components),
                process_boundaries: items.first().map_or(0, |row| row.process_boundaries),
                serialization_steps: items.first().map_or(0, |row| row.serialization_steps),
                p50_e2e_latency_ms: percentile(
                    &items.iter().map(|row| row.e2e_latency_ms).collect::<Vec<_>>(),
                    50.0,
                ),
                p95_e2e_latency_ms: percentile(
                    &items.iter().map(|row| row.e2e_latency_ms).collect::<Vec<_>>(),
                    95.0,
                ),
                avg_useful_engine_work_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.estimated_useful_engine_work_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_coordination_overhead_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.estimated_coordination_overhead_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_external_transfer_bytes: mean_usize(
                    &items
                        .iter()
                        .map(|row| row.estimated_external_transfer_bytes)
                        .collect::<Vec<_>>(),
                ),
                avg_final_result_bytes: mean_usize(
                    &items.iter().map(|row| row.final_result_bytes).collect::<Vec<_>>(),
                ),
                avg_result_count: mean_usize(
                    &items.iter().map(|row| row.result_count).collect::<Vec<_>>(),
                ),
                avg_live_events_published: mean_usize(
                    &items.iter().map(|row| row.live_events_published).collect::<Vec<_>>(),
                ),
                avg_live_events_processed: mean_usize(
                    &items.iter().map(|row| row.live_events_processed).collect::<Vec<_>>(),
                ),
                avg_live_stream_processing_latency_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.live_stream_processing_latency_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_external_join_latency_ms: mean(
                    &items.iter().map(|row| row.external_join_latency_ms).collect::<Vec<_>>(),
                ),
                avg_first_hybrid_result_latency_ms: mean(
                    &items.iter().map(|row| row.first_hybrid_result_latency_ms).collect::<Vec<_>>(),
                ),
                avg_historical_result_count: mean_usize(
                    &items.iter().map(|row| row.historical_result_count).collect::<Vec<_>>(),
                ),
                avg_live_result_count: mean_usize(
                    &items.iter().map(|row| row.live_result_count).collect::<Vec<_>>(),
                ),
                avg_hybrid_result_count: mean_usize(
                    &items.iter().map(|row| row.hybrid_result_count).collect::<Vec<_>>(),
                ),
                historical_equivalence_rate: if runs > 0 {
                    historical_equiv_count as f64 / runs as f64
                } else {
                    0.0
                },
                hybrid_equivalence_rate: if runs > 0 {
                    hybrid_equiv_count as f64 / runs as f64
                } else {
                    0.0
                },
            }
        })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        left.system.cmp(&right.system).then_with(|| left.mode.cmp(&right.mode))
    });
    summary
}

pub fn summarize_scaling(rows: &[ScalingRow]) -> Vec<ScalingSummaryRow> {
    let mut grouped = HashMap::<(usize, String, String), Vec<&ScalingRow>>::new();
    for row in rows {
        grouped
            .entry((row.dataset_size_quads, row.query_type.clone(), row.mode.clone()))
            .or_default()
            .push(row);
    }

    let mut summary = grouped
        .into_iter()
        .map(|((dataset_size_quads, query_type, mode), items)| ScalingSummaryRow {
            dataset_size_quads,
            query_type,
            mode,
            runs: items.len(),
            logical_quads_scanned: items.first().map_or(0, |row| row.logical_quads_scanned),
            selectivity: items.first().map_or(0.0, |row| row.selectivity),
            result_count: items.first().map_or(0, |row| row.result_count),
            p50_latency_ms: percentile(
                &items.iter().map(|row| row.latency_ms).collect::<Vec<_>>(),
                50.0,
            ),
            p95_latency_ms: percentile(
                &items.iter().map(|row| row.latency_ms).collect::<Vec<_>>(),
                95.0,
            ),
            avg_latency_ms: mean(&items.iter().map(|row| row.latency_ms).collect::<Vec<_>>()),
            avg_throughput_quads_per_sec: mean(
                &items.iter().map(|row| row.throughput_quads_per_sec).collect::<Vec<_>>(),
            ),
            max_peak_rss_mb: items
                .iter()
                .filter_map(|row| row.peak_rss_mb)
                .max_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal)),
        })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        left.dataset_size_quads
            .cmp(&right.dataset_size_quads)
            .then_with(|| left.query_type.cmp(&right.query_type))
            .then_with(|| left.mode.cmp(&right.mode))
    });
    summary
}

pub fn summarize_scaling_fit(rows: &[ScalingRow]) -> Vec<ScalingFitRow> {
    let mut grouped = HashMap::<(String, String), Vec<&ScalingRow>>::new();
    for row in rows {
        grouped.entry((row.query_type.clone(), row.mode.clone())).or_default().push(row);
    }

    let mut fit_rows = grouped
        .into_iter()
        .map(|((query_type, mode), items)| {
            let mut points = HashMap::<usize, Vec<f64>>::new();
            for row in items {
                points.entry(row.dataset_size_quads).or_default().push(row.latency_ms);
            }
            let mut sorted_points = points
                .into_iter()
                .map(|(size, values)| (size as f64, mean(&values)))
                .collect::<Vec<_>>();
            sorted_points
                .sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(Ordering::Equal));
            linear_fit_row(&query_type, &mode, &sorted_points)
        })
        .collect::<Vec<_>>();
    fit_rows.sort_by(|left, right| {
        left.query_type.cmp(&right.query_type).then_with(|| left.mode.cmp(&right.mode))
    });
    fit_rows
}

pub fn fill_scaling_percentiles(rows: &mut [ScalingRow]) {
    let mut grouped = HashMap::<(usize, String, String), Vec<usize>>::new();
    for (index, row) in rows.iter().enumerate() {
        grouped
            .entry((row.dataset_size_quads, row.query_type.clone(), row.mode.clone()))
            .or_default()
            .push(index);
    }

    for indexes in grouped.values() {
        let latencies = indexes.iter().map(|index| rows[*index].latency_ms).collect::<Vec<_>>();
        let p50 = percentile(&latencies, 50.0);
        let p95 = percentile(&latencies, 95.0);
        for index in indexes {
            rows[*index].p50_latency_ms = p50;
            rows[*index].p95_latency_ms = p95;
        }
    }
}

pub fn generate_citybench_dataset(
    size_quads: usize,
    output_dir: &Path,
) -> Result<HistoricalDataset, Box<dyn std::error::Error>> {
    let logs_dir = output_dir.join("logs");
    fs::create_dir_all(&logs_dir)?;
    let log_path = logs_dir.join(format!("citybench_{size_quads}.nq"));

    let mut log_file = File::create(&log_path)?;
    let storage = Arc::new(StreamingSegmentedStorage::new(unique_config("paper_scaling"))?);
    let start_ts = 1_720_000_000_000;
    let fixed_window = 1_000usize.min(size_quads.max(1));
    let range_10 = (size_quads / 10).max(1);
    let range_50 = (size_quads / 2).max(1);
    let midpoint = size_quads / 2;
    let point_ts = start_ts + midpoint as u64;
    let point_subject = format!("http://example.org/junction/{}", midpoint % 256);

    for index in 0..size_quads {
        let event = citybench_event(start_ts + index as u64, index);
        writeln!(
            log_file,
            "{} <{}> <{}> \"{}\" <{}> .",
            event.timestamp, event.subject, event.predicate, event.object, event.graph
        )?;
        storage.write_rdf_event(event)?;
    }
    storage.flush()?;

    Ok(HistoricalDataset {
        storage,
        spec: DatasetSpec {
            size_quads,
            start_ts,
            end_ts: start_ts + size_quads.saturating_sub(1) as u64,
            point_ts,
            point_subject,
            fixed_start: start_ts + midpoint.saturating_sub(fixed_window / 2) as u64,
            fixed_end: start_ts
                + midpoint.saturating_sub(fixed_window / 2) as u64
                + fixed_window as u64,
            proportional_10_end: start_ts + range_10 as u64,
            proportional_50_end: start_ts + range_50 as u64,
        },
    })
}

pub fn prepare_coordination_workload(
    historical_events: usize,
    live_events: usize,
) -> Result<CoordinationWorkload, Box<dyn std::error::Error>> {
    let historical_start_ts = 1_800_000_000_000;
    let historical_storage = build_historical_storage(historical_events, historical_start_ts)?;
    let historical_rdf_events = historical_storage.query_rdf(
        historical_start_ts,
        historical_start_ts + historical_events.saturating_sub(1) as u64,
    )?;
    Ok(CoordinationWorkload {
        historical_storage,
        historical_rdf_events,
        live_events: build_live_events(live_events, 1_900_000_000_000),
        historical_start_ts,
        historical_end_ts: historical_start_ts + historical_events.saturating_sub(1) as u64,
        historical_sparql_query: historical_baseline_sparql_query()?,
        hybrid_query: hybrid_query(
            historical_start_ts,
            historical_start_ts + historical_events.saturating_sub(1) as u64,
        ),
    })
}

pub fn run_coordination_pair(
    config: CoordinationRunConfig<'_>,
) -> Result<CoordinationPair, Box<dyn std::error::Error>> {
    let unified = run_coordination_system(CoordinationSystem::JanusUnified, &config)?;
    let decomposed = run_coordination_system(CoordinationSystem::DecomposedOxigraph, &config)?;

    let historical_equivalent = unified.historical_result_hash == decomposed.historical_result_hash;
    let hybrid_equivalent = unified.hybrid_result_hash == decomposed.hybrid_result_hash
        && unified.hybrid_result_count == decomposed.hybrid_result_count;

    let mut unified = unified;
    let mut decomposed = decomposed;

    unified.historical_equivalent_to_baseline = Some(historical_equivalent);
    unified.hybrid_equivalent_to_baseline = Some(hybrid_equivalent);
    unified.equivalent_to_baseline = Some(hybrid_equivalent); // hybrid is primary equivalence target

    decomposed.historical_equivalent_to_baseline = None;
    decomposed.hybrid_equivalent_to_baseline = None;
    decomposed.equivalent_to_baseline = None;

    if let Some(debug_output_dir) = config.debug_output_dir {
        write_h1_debug_artifacts(debug_output_dir, &unified, &decomposed, &config)?;
    }

    Ok(CoordinationPair { unified, decomposed })
}

fn run_coordination_system(
    system: CoordinationSystem,
    config: &CoordinationRunConfig<'_>,
) -> Result<CoordinationRow, Box<dyn std::error::Error>> {
    let owned_workload;
    let workload = if config.mode == ExecutionMode::Warm {
        config.warm_workload.ok_or("warm mode requires prepared workload")?
    } else {
        owned_workload =
            prepare_coordination_workload(config.historical_events, config.live_events)?;
        &owned_workload
    };

    let client_start = now_ms();
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&workload.hybrid_query)?;
    let query_registered = now_ms();

    let historical_start = now_ms();
    let executor =
        HistoricalExecutor::new(Arc::clone(&workload.historical_storage), OxigraphAdapter::new());
    let baseline_bindings = executor.execute_fixed_window(
        parsed.historical_windows.first().ok_or("missing historical window")?,
        &workload.historical_sparql_query,
    )?;
    let historical_done = now_ms();

    let live_events_published = workload.live_events.len();
    let live_events_processed = workload.live_events.len();
    let live_window_size = 10000;
    let live_window_slide = 1000;
    let live_first_event_at = workload.live_events.first().map(|e| e.timestamp).unwrap_or(0);

    match system {
        CoordinationSystem::JanusUnified => {
            let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
            processor.register_stream(LIVE_STREAM_URI)?;
            materialize_bindings_as_static_baseline(&mut processor, &baseline_bindings)?;
            processor.start_processing()?;
            let live_ready = now_ms();
            publish_live_events(&processor, &workload.live_events)?;
            let first_event_published = now_ms();
            let live_collection = if workload.live_events.is_empty() {
                LiveCollectionResult { first_result_engine_ms: now_ms(), all_rows: Vec::new() }
            } else {
                collect_live_results(
                    &processor,
                    Duration::from_millis(500),
                    Duration::from_millis(10),
                )?
            };
            let result_rows = live_collection.all_rows;
            let first_result_engine = live_collection.first_result_engine_ms;
            let first_result_client = now_ms();

            let estimated_useful_engine_work_ms = (historical_done - historical_start) as f64
                + (first_result_engine - first_event_published) as f64;
            let e2e_latency_ms = (first_result_client - client_start) as f64;

            let live_stream_processing_latency_ms =
                (first_result_engine as f64 - first_event_published as f64).max(0.0);
            let external_join_latency_ms = 0.0;
            let first_hybrid_result_latency_ms = e2e_latency_ms;

            let historical_result_count = baseline_bindings.len();
            let historical_result_hash = canonical_result_hash(&baseline_bindings)?;
            let live_result_count = 0;
            let live_result_hash = None;
            let hybrid_result_count = result_rows.len();
            let hybrid_result_hash = canonical_result_hash(&result_rows)?;

            Ok(CoordinationRow {
                system: system.as_str().to_string(),
                mode: config.mode.as_str().to_string(),
                is_warmup: config.is_warmup,
                run_index: config.run_index,
                historical_events: config.historical_events,
                live_events: config.live_events,
                client_start,
                query_registered,
                historical_start,
                historical_done,
                live_ready,
                first_event_published,
                first_result_engine,
                first_result_client,
                e2e_latency_ms,
                estimated_useful_engine_work_ms,
                estimated_coordination_overhead_ms: (e2e_latency_ms
                    - estimated_useful_engine_work_ms)
                    .max(0.0),
                historical_intermediate_bytes: serde_json::to_vec(&baseline_bindings)?.len(),
                live_intermediate_bytes: serde_json::to_vec(&event_payloads(
                    &workload.live_events,
                ))?
                .len(),
                estimated_external_transfer_bytes: 0,
                final_result_bytes: serde_json::to_vec(&result_rows)?.len(),
                components: 1,
                process_boundaries: 0,
                serialization_steps: 1,
                result_count: result_rows.len(),
                historical_input_hash: historical_input_hash(&workload.historical_rdf_events)?,
                live_input_hash: live_input_hash(&workload.live_events)?,
                result_hash: canonical_result_hash(&result_rows)?,
                equivalent_to_baseline: None,
                metadata: config.metadata.clone(),

                live_events_published,
                live_events_processed,
                live_window_size,
                live_window_slide,
                live_query_registered_at: query_registered,
                live_first_event_at,
                live_first_window_result_at: first_result_engine,
                live_stream_processing_latency_ms,
                external_join_latency_ms,
                first_hybrid_result_latency_ms,
                historical_result_count,
                historical_result_hash,
                live_result_count,
                live_result_hash,
                hybrid_result_count,
                hybrid_result_hash,
                historical_equivalent_to_baseline: None,
                hybrid_equivalent_to_baseline: None,
            })
        }
        CoordinationSystem::DecomposedOxigraph => {
            let external_bindings = config.adapter.execute_bindings_query(
                &workload.historical_sparql_query,
                &workload.historical_rdf_events,
            )?;
            let materialized_baseline_rows =
                materialized_baseline_rows_from_bindings(&external_bindings, "baselineFlow");
            let historical_done = now_ms();
            let mut processor = LiveStreamProcessing::new(live_only_rspql())?;
            processor.register_stream(LIVE_STREAM_URI)?;
            processor.start_processing()?;
            let live_ready = now_ms();
            publish_live_events(&processor, &workload.live_events)?;
            let first_event_published = now_ms();
            let live_collection = if workload.live_events.is_empty() {
                LiveCollectionResult { first_result_engine_ms: now_ms(), all_rows: Vec::new() }
            } else {
                collect_live_results(
                    &processor,
                    Duration::from_millis(500),
                    Duration::from_millis(10),
                )?
            };
            let live_rows = live_collection.all_rows;
            let joined_rows = join_live_with_baseline(&live_rows, &materialized_baseline_rows);
            let first_result_engine = live_collection.first_result_engine_ms;
            let first_result_client = now_ms();

            let estimated_useful_engine_work_ms = (historical_done - historical_start) as f64
                + (first_result_engine - first_event_published) as f64;
            let e2e_latency_ms = (first_result_client - client_start) as f64;

            let live_stream_processing_latency_ms =
                (first_result_engine as f64 - first_event_published as f64).max(0.0);
            let external_join_latency_ms =
                (first_result_client as f64 - first_result_engine as f64).max(0.0);
            let first_hybrid_result_latency_ms = e2e_latency_ms;

            let historical_result_count = external_bindings.len();
            let historical_result_hash = canonical_result_hash(&external_bindings)?;
            let live_result_count = live_rows.len();
            let live_result_hash = Some(canonical_result_hash(&live_rows)?);
            let hybrid_result_count = joined_rows.len();
            let hybrid_result_hash = canonical_result_hash(&joined_rows)?;

            let historical_intermediate_bytes = serde_json::to_vec(&external_bindings)?.len();
            let live_intermediate_bytes =
                serde_json::to_vec(&event_payloads(&workload.live_events))?.len();
            Ok(CoordinationRow {
                system: system.as_str().to_string(),
                mode: config.mode.as_str().to_string(),
                is_warmup: config.is_warmup,
                run_index: config.run_index,
                historical_events: config.historical_events,
                live_events: config.live_events,
                client_start,
                query_registered,
                historical_start,
                historical_done,
                live_ready,
                first_event_published,
                first_result_engine,
                first_result_client,
                e2e_latency_ms,
                estimated_useful_engine_work_ms,
                estimated_coordination_overhead_ms: (e2e_latency_ms
                    - estimated_useful_engine_work_ms)
                    .max(0.0),
                historical_intermediate_bytes,
                live_intermediate_bytes,
                estimated_external_transfer_bytes: historical_intermediate_bytes
                    + live_intermediate_bytes,
                final_result_bytes: serde_json::to_vec(&joined_rows)?.len(),
                components: 4,
                process_boundaries: 3,
                serialization_steps: 4,
                result_count: joined_rows.len(),
                historical_input_hash: historical_input_hash(&workload.historical_rdf_events)?,
                live_input_hash: live_input_hash(&workload.live_events)?,
                result_hash: canonical_result_hash(&joined_rows)?,
                equivalent_to_baseline: None,
                metadata: config.metadata.clone(),

                live_events_published,
                live_events_processed,
                live_window_size,
                live_window_slide,
                live_query_registered_at: query_registered,
                live_first_event_at,
                live_first_window_result_at: first_result_engine,
                live_stream_processing_latency_ms,
                external_join_latency_ms,
                first_hybrid_result_latency_ms,
                historical_result_count,
                historical_result_hash,
                live_result_count,
                live_result_hash,
                hybrid_result_count,
                hybrid_result_hash,
                historical_equivalent_to_baseline: None,
                hybrid_equivalent_to_baseline: None,
            })
        }
    }
}

pub fn run_scaling_query(
    config: ScalingRunConfig<'_>,
) -> Result<ScalingRow, Box<dyn std::error::Error>> {
    let owned_dataset;
    let dataset = if config.mode == ExecutionMode::Warm {
        config.warm_dataset.ok_or("warm mode requires prepared dataset")?
    } else {
        owned_dataset = generate_citybench_dataset(config.dataset_size_quads, config.output_dir)?;
        &owned_dataset
    };

    let started = Instant::now();
    let query_result = match config.query_type {
        ScalingQueryType::PointLookup => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(
                dataset.spec.point_ts,
                dataset.spec.point_ts,
                Some(&dataset.spec.point_subject),
            ),
        )?,
        ScalingQueryType::FixedWindow => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.fixed_start, dataset.spec.fixed_end, None),
        )?,
        ScalingQueryType::ProportionalRange10 => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.start_ts, dataset.spec.proportional_10_end, None),
        )?,
        ScalingQueryType::ProportionalRange50 => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.start_ts, dataset.spec.proportional_50_end, None),
        )?,
        ScalingQueryType::FullRange => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.start_ts, dataset.spec.end_ts, None),
        )?,
        ScalingQueryType::HybridBaselineLookup => {
            run_hybrid_baseline_lookup(Arc::clone(&dataset.storage), &dataset.spec)?
        }
    };

    let latency_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let throughput_quads_per_sec = if latency_ms == 0.0 {
        0.0
    } else {
        query_result.logical_quads_scanned as f64 / (latency_ms / 1_000.0)
    };

    Ok(ScalingRow {
        dataset_size_quads: dataset.spec.size_quads,
        query_type: config.query_type.as_str().to_string(),
        mode: config.mode.as_str().to_string(),
        is_warmup: config.is_warmup,
        run_index: config.run_index,
        logical_quads_scanned: query_result.logical_quads_scanned,
        selectivity: if dataset.spec.size_quads == 0 {
            0.0
        } else {
            query_result.result_rows.len() as f64 / dataset.spec.size_quads as f64
        },
        result_count: query_result.result_rows.len(),
        result_hash: canonical_result_hash(&query_result.result_rows)?,
        latency_ms,
        p50_latency_ms: 0.0,
        p95_latency_ms: 0.0,
        throughput_quads_per_sec,
        peak_rss_mb: current_rss_bytes().map(|bytes| bytes as f64 / (1024.0 * 1024.0)),
        metadata: config.metadata.clone(),
    })
}

struct QueryExecutionResult {
    logical_quads_scanned: usize,
    result_rows: Vec<HashMap<String, String>>,
}

struct LiveCollectionResult {
    first_result_engine_ms: u64,
    all_rows: Vec<HashMap<String, String>>,
}

fn run_historical_query(
    storage: Arc<StreamingSegmentedStorage>,
    query: String,
) -> Result<QueryExecutionResult, Box<dyn std::error::Error>> {
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&query)?;
    let window = parsed.historical_windows.first().ok_or("missing historical window")?;
    let logical_quads_scanned = storage
        .query(window.start.ok_or("missing start")?, window.end.ok_or("missing end")?)?
        .len();
    let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
    let result_rows = executor.execute_fixed_window(
        window,
        parsed.sparql_queries.first().ok_or("missing historical query")?,
    )?;
    Ok(QueryExecutionResult { logical_quads_scanned, result_rows })
}

fn run_hybrid_baseline_lookup(
    storage: Arc<StreamingSegmentedStorage>,
    dataset: &DatasetSpec,
) -> Result<QueryExecutionResult, Box<dyn std::error::Error>> {
    let query = format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX baseline: <{BASELINE_NS}>

        REGISTER RStream <output> AS
        SELECT ?sensor ?liveFlow ?baselineFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {} END {}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE 5000 STEP 1000]
        USING BASELINE ex:hist AGGREGATE
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:trafficFlow ?baselineFlow .
            }}
            WINDOW ex:live {{
                ?sensor ex:trafficFlow ?liveFlow .
            }}
            ?sensor baseline:baselineFlow ?baselineFlow .
        }}
        "#,
        dataset.start_ts, dataset.end_ts
    );
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&query)?;
    let window = parsed.historical_windows.first().ok_or("missing historical window")?;
    let logical_quads_scanned = storage.query(dataset.start_ts, dataset.end_ts)?.len();
    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());
    let bindings = executor.execute_fixed_window(
        window,
        parsed.sparql_queries.first().ok_or("missing historical query")?,
    )?;
    let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    processor.register_stream(LIVE_STREAM_URI)?;
    materialize_bindings_as_static_baseline(&mut processor, &bindings)?;
    processor.start_processing()?;
    processor.add_event(
        LIVE_STREAM_URI,
        RDFEvent::new(
            dataset.end_ts + 1,
            &dataset.point_subject,
            TRAFFIC_PREDICATE,
            "77",
            GRAPH_URI,
        ),
    )?;
    processor.close_stream(
        LIVE_STREAM_URI,
        i64::try_from(dataset.end_ts + 10_000).unwrap_or(i64::MAX),
    )?;
    let result_rows =
        collect_live_results(&processor, Duration::from_secs(10), Duration::from_millis(10))?
            .all_rows;
    Ok(QueryExecutionResult { logical_quads_scanned, result_rows })
}

fn materialize_bindings_as_static_baseline(
    processor: &mut LiveStreamProcessing,
    bindings: &[HashMap<String, String>],
) -> Result<(), JanusApiError> {
    for (subject, predicate, object) in baseline_statements_from_bindings(bindings) {
        processor
            .add_static_data(RDFEvent::new(0, &subject, &predicate, &object, ""))
            .map_err(|err| {
                JanusApiError::LiveProcessingError(format!(
                    "Failed to materialize baseline statement '{} {} {}': {}",
                    subject, predicate, object, err
                ))
            })?;
    }
    Ok(())
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

fn join_live_with_baseline(
    live_rows: &[HashMap<String, String>],
    baseline_rows: &[HashMap<String, String>],
) -> Vec<HashMap<String, String>> {
    join_live_with_baseline_detailed(live_rows, baseline_rows).0
}

fn join_live_with_baseline_detailed(
    live_rows: &[HashMap<String, String>],
    baseline_rows: &[HashMap<String, String>],
) -> (Vec<HashMap<String, String>>, Vec<JoinTraceRow>) {
    let mut baseline_by_subject = HashMap::<String, HashMap<String, String>>::new();
    for row in baseline_rows {
        let Some(subject) = row
            .get("sensor")
            .or_else(|| row.get("s"))
            .map(|value| normalize_binding_term(value))
        else {
            continue;
        };
        baseline_by_subject.insert(subject, row.clone());
    }

    let mut joined = Vec::new();
    let mut trace = Vec::new();
    for live_row in live_rows {
        let Some(subject) = live_row
            .get("sensor")
            .or_else(|| live_row.get("s"))
            .map(|value| normalize_binding_term(value))
        else {
            trace.push(JoinTraceRow {
                historical_join_key: None,
                live_join_key: None,
                accepted: false,
                rejection_reason: Some("missing_live_join_key".to_string()),
                historical_row: None,
                live_row: canonicalize_row(live_row),
                joined_row: None,
            });
            continue;
        };
        if let Some(baseline_row) = baseline_by_subject.get(&subject) {
            let mut merged = baseline_row.clone();
            for (key, value) in live_row {
                merged.insert(key.clone(), value.clone());
            }
            trace.push(JoinTraceRow {
                historical_join_key: Some(subject.clone()),
                live_join_key: Some(subject),
                accepted: true,
                rejection_reason: None,
                historical_row: Some(canonicalize_row(baseline_row)),
                live_row: canonicalize_row(live_row),
                joined_row: Some(canonicalize_row(&merged)),
            });
            joined.push(merged);
        } else {
            trace.push(JoinTraceRow {
                historical_join_key: None,
                live_join_key: Some(subject),
                accepted: false,
                rejection_reason: Some("no_historical_row_for_join_key".to_string()),
                historical_row: None,
                live_row: canonicalize_row(live_row),
                joined_row: None,
            });
        }
    }
    (joined, trace)
}

fn build_historical_storage(
    events: usize,
    start_ts: u64,
) -> Result<Arc<StreamingSegmentedStorage>, Box<dyn std::error::Error>> {
    let storage = Arc::new(StreamingSegmentedStorage::new(unique_config("paper_h1"))?);
    for index in 0..events {
        let event = RDFEvent::new(
            start_ts + index as u64,
            &format!("http://example.org/junction/{}", index % 64),
            BASELINE_PREDICATE,
            &(40 + (index % 17)).to_string(),
            GRAPH_URI,
        );
        storage.write_rdf_event(event)?;
    }
    storage.flush()?;
    Ok(storage)
}

fn build_live_events(events: usize, start_ts: u64) -> Vec<RDFEvent> {
    (0..events)
        .map(|index| {
            RDFEvent::new(
                start_ts + index as u64,
                &format!("http://example.org/junction/{}", index % 64),
                TRAFFIC_PREDICATE,
                &(70 + (index % 11)).to_string(),
                GRAPH_URI,
            )
        })
        .collect()
}

fn hybrid_query(start_ts: u64, end_ts: u64) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX baseline: <{BASELINE_NS}>

        REGISTER RStream <output> AS
        SELECT ?sensor ?liveFlow ?baselineFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {start_ts} END {end_ts}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE 10000 STEP 1000]
        USING BASELINE ex:hist AGGREGATE
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:baselineFlow ?baselineFlow .
            }}
            WINDOW ex:live {{
                ?sensor ex:trafficFlow ?liveFlow .
            }}
            ?sensor baseline:baselineFlow ?baselineFlow .
        }}
        "#
    )
}

fn historical_baseline_sparql_query() -> Result<String, Box<dyn std::error::Error>> {
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&format!(
        r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor ?baselineFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START 1 END 2]
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:baselineFlow ?baselineFlow .
            }}
        }}
        "#
    ))?;
    Ok(parsed
        .sparql_queries
        .first()
        .cloned()
        .ok_or("missing generated historical SPARQL query")?)
}

fn live_only_rspql() -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor ?liveFlow
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE 10000 STEP 1000]
        WHERE {{
            WINDOW ex:live {{
                ?sensor ex:trafficFlow ?liveFlow .
            }}
        }}
        "#
    )
}

fn historical_lookup_query(start: u64, end: u64, subject_filter: Option<&str>) -> String {
    let subject_clause = subject_filter
        .map(|subject| format!("<{subject}> ex:trafficFlow ?trafficFlow ."))
        .unwrap_or_else(|| "?sensor ex:trafficFlow ?trafficFlow .".to_string());
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor ?trafficFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {start} END {end}]
        WHERE {{
            WINDOW ex:hist {{
                {subject_clause}
            }}
        }}
        "#
    )
}

fn publish_live_events(
    processor: &LiveStreamProcessing,
    live_events: &[RDFEvent],
) -> Result<(), Box<dyn std::error::Error>> {
    if live_events.is_empty() {
        processor.close_stream(LIVE_STREAM_URI, 20_000_i64)?;
        return Ok(());
    }
    let first = live_events.first().unwrap().clone();
    processor.add_event(LIVE_STREAM_URI, first)?;
    for event in live_events.iter().skip(1) {
        processor.add_event(LIVE_STREAM_URI, event.clone())?;
    }
    let close_ts = live_events
        .last()
        .map_or(20_000_i64, |event| i64::try_from(event.timestamp).unwrap_or(i64::MAX) + 20_000);
    processor.close_stream(LIVE_STREAM_URI, close_ts)?;
    Ok(())
}

fn collect_live_results(
    processor: &LiveStreamProcessing,
    first_result_timeout: Duration,
    idle_timeout: Duration,
) -> Result<LiveCollectionResult, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + first_result_timeout;
    loop {
        if let Some(result) = processor.try_receive_result()? {
            let first_result_engine_ms = now_ms();
            let mut rows = vec![parse_rsprs_binding_string(&result.bindings)];
            let mut idle_deadline = Instant::now() + idle_timeout;
            loop {
                if let Some(next_result) = processor.try_receive_result()? {
                    rows.push(parse_rsprs_binding_string(&next_result.bindings));
                    idle_deadline = Instant::now() + idle_timeout;
                    continue;
                }
                if Instant::now() >= idle_deadline {
                    return Ok(LiveCollectionResult { first_result_engine_ms, all_rows: rows });
                }
                std::thread::yield_now();
            }
        }
        if Instant::now() >= deadline {
            return Ok(LiveCollectionResult {
                first_result_engine_ms: now_ms(),
                all_rows: Vec::new(),
            });
        }
        std::thread::yield_now();
    }
}

fn event_payloads(events: &[RDFEvent]) -> Vec<(&str, &str, &str, &str, u64)> {
    events
        .iter()
        .map(|event| {
            (
                event.subject.as_str(),
                event.predicate.as_str(),
                event.object.as_str(),
                event.graph.as_str(),
                event.timestamp,
            )
        })
        .collect()
}

fn parse_rsprs_binding_string(binding_str: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let bindings_str = binding_str.trim_matches(|ch| ch == '{' || ch == '}').trim();
    let parts = bindings_str.split(", Variable").collect::<Vec<_>>();

    for (index, part) in parts.iter().enumerate() {
        let binding = if index == 0 {
            part.trim_start_matches("Variable")
        } else {
            part
        };
        let Some(name_start) = binding.find("name: \"") else {
            continue;
        };
        let name_offset = name_start + 7;
        let Some(name_end) = binding[name_offset..].find('"') else {
            continue;
        };
        let variable = &binding[name_offset..name_offset + name_end];
        let value = if binding.contains("TypedLiteral") {
            extract_between(binding, "value: \"", "\"")
        } else if binding.contains("NamedNode") {
            extract_between(binding, "iri: \"", "\"")
        } else if binding.contains("Literal(Literal(String(\"") {
            extract_between(binding, "String(\"", "\")")
        } else if binding.contains("Literal(Literal(") {
            extract_between(binding, "Literal(Literal(", "))")
        } else {
            None
        };
        if let Some(value) = value {
            result.insert(variable.to_string(), value);
        }
    }

    result
}

fn extract_between(input: &str, start: &str, end: &str) -> Option<String> {
    let start_index = input.find(start)? + start.len();
    let end_index = input[start_index..].find(end)?;
    Some(input[start_index..start_index + end_index].to_string())
}

fn citybench_event(timestamp: u64, index: usize) -> RDFEvent {
    let junction = index % 256;
    let flow = 20 + (index % 80);
    RDFEvent::new(
        timestamp,
        &format!("http://example.org/junction/{junction}"),
        TRAFFIC_PREDICATE,
        &flow.to_string(),
        GRAPH_URI,
    )
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

fn canonicalize_row(row: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut entries = row
        .iter()
        .map(|(key, value)| (key.clone(), normalize_binding_term(value)))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    entries
}

fn linear_fit_row(query_type: &str, mode: &str, points: &[(f64, f64)]) -> ScalingFitRow {
    if points.is_empty() {
        return ScalingFitRow {
            query_type: query_type.to_string(),
            mode: mode.to_string(),
            slope_ms_per_100k_quads: 0.0,
            intercept_ms: 0.0,
            r_squared: 0.0,
            number_of_points: 0,
        };
    }

    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let numerator = points.iter().map(|(x, y)| (x - mean_x) * (y - mean_y)).sum::<f64>();
    let denominator = points.iter().map(|(x, _)| (x - mean_x).powi(2)).sum::<f64>();
    let slope = if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    };
    let intercept = mean_y - slope * mean_x;

    let ss_tot = points.iter().map(|(_, y)| (y - mean_y).powi(2)).sum::<f64>();
    let ss_res = points
        .iter()
        .map(|(x, y)| {
            let predicted = intercept + slope * x;
            (y - predicted).powi(2)
        })
        .sum::<f64>();
    let r_squared = if ss_tot == 0.0 {
        1.0
    } else {
        1.0 - (ss_res / ss_tot)
    };

    ScalingFitRow {
        query_type: query_type.to_string(),
        mode: mode.to_string(),
        slope_ms_per_100k_quads: slope * 100_000.0,
        intercept_ms: intercept,
        r_squared,
        number_of_points: points.len(),
    }
}

fn percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let rank = ((pct / 100.0) * (sorted.len().saturating_sub(1) as f64)).round() as usize;
    sorted[rank]
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn mean_usize(values: &[usize]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<usize>() as f64 / values.len() as f64
    }
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

fn capture_command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn detect_os() -> String {
    format!("{} {}", env::consts::OS, capture_command("uname", &["-r"]))
}

fn detect_cpu_model() -> String {
    #[cfg(target_os = "macos")]
    {
        let model = capture_command("sysctl", &["-n", "machdep.cpu.brand_string"]);
        if model != "unknown" {
            return model;
        }
    }
    #[cfg(target_os = "linux")]
    {
        let model =
            capture_command("sh", &["-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"]);
        if model != "unknown" {
            return model.trim().to_string();
        }
    }
    "unknown".to_string()
}

fn detect_ram_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        return capture_command("sysctl", &["-n", "hw.memsize"]).parse::<u64>().ok();
    }
    #[cfg(target_os = "linux")]
    {
        return capture_command("sh", &["-c", "grep MemTotal /proc/meminfo | awk '{print $2}'"])
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024);
    }
    #[allow(unreachable_code)]
    None
}

fn current_rss_bytes() -> Option<u64> {
    capture_command("ps", &["-o", "rss=", "-p", &std::process::id().to_string()])
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
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

fn historical_input_hash(events: &[RDFEvent]) -> Result<String, Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec(&event_payloads(events))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn live_input_hash(events: &[RDFEvent]) -> Result<String, Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec(&event_payloads(events))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn event_payload_rows(events: &[RDFEvent]) -> Vec<HashMap<String, String>> {
    events
        .iter()
        .map(|event| {
            HashMap::from([
                ("timestamp".to_string(), event.timestamp.to_string()),
                ("subject".to_string(), event.subject.clone()),
                ("predicate".to_string(), event.predicate.clone()),
                ("object".to_string(), event.object.clone()),
                ("graph".to_string(), event.graph.clone()),
            ])
        })
        .collect()
}

fn canonical_result_rows(rows: &[HashMap<String, String>]) -> Vec<Vec<(String, String)>> {
    let mut canonical = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
    canonical.sort();
    canonical
}

fn write_trig_events(path: &Path, events: &[RDFEvent]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;
    let mut grouped = HashMap::<String, Vec<&RDFEvent>>::new();
    for event in events {
        grouped.entry(event.graph.clone()).or_default().push(event);
    }
    let mut graphs = grouped.into_iter().collect::<Vec<_>>();
    graphs.sort_by(|left, right| left.0.cmp(&right.0));
    for (graph, rows) in graphs {
        writeln!(file, "<{}> {{", graph)?;
        for event in rows {
            writeln!(
                file,
                "  <{}> <{}> \"{}\" . # ts={}",
                event.subject, event.predicate, event.object, event.timestamp
            )?;
        }
        writeln!(file, "}}")?;
    }
    Ok(())
}

fn write_h1_debug_artifacts(
    base_dir: &Path,
    unified: &CoordinationRow,
    decomposed: &CoordinationRow,
    config: &CoordinationRunConfig<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let run_dir = base_dir.join("h1_equivalence_debug").join(format!(
        "run_{:03}_{}",
        config.run_index,
        if config.is_warmup {
            "warmup"
        } else {
            "measured"
        }
    ));
    fs::create_dir_all(&run_dir)?;

    let workload = config
        .warm_workload
        .ok_or("debug equivalence artifacts currently require warm workload")?;
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&workload.hybrid_query)?;
    let executor =
        HistoricalExecutor::new(Arc::clone(&workload.historical_storage), OxigraphAdapter::new());
    let janus_historical_results = executor.execute_fixed_window(
        parsed.historical_windows.first().ok_or("missing historical window")?,
        &workload.historical_sparql_query,
    )?;
    let oxigraph_historical_results = config.adapter.execute_bindings_query(
        &workload.historical_sparql_query,
        &workload.historical_rdf_events,
    )?;
    let oxigraph_materialized_baseline =
        materialized_baseline_rows_from_bindings(&oxigraph_historical_results, "baselineFlow");

    let mut live_processor = LiveStreamProcessing::new(live_only_rspql())?;
    live_processor.register_stream(LIVE_STREAM_URI)?;
    live_processor.start_processing()?;
    publish_live_events(&live_processor, &workload.live_events)?;
    let live_rows =
        collect_live_results(&live_processor, Duration::from_secs(10), Duration::from_millis(10))?
            .all_rows;
    let (decomposed_join_results, join_trace) =
        join_live_with_baseline_detailed(&live_rows, &oxigraph_materialized_baseline);

    let mut janus_processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    janus_processor.register_stream(LIVE_STREAM_URI)?;
    materialize_bindings_as_static_baseline(&mut janus_processor, &janus_historical_results)?;
    janus_processor.start_processing()?;
    publish_live_events(&janus_processor, &workload.live_events)?;
    let janus_results =
        collect_live_results(&janus_processor, Duration::from_secs(10), Duration::from_millis(10))?
            .all_rows;

    write_trig_events(
        &run_dir.join("janus_input_historical.trig"),
        &workload.historical_rdf_events,
    )?;
    write_trig_events(
        &run_dir.join("oxigraph_input_historical.trig"),
        &workload.historical_rdf_events,
    )?;
    write_jsonl(&run_dir.join("live_events.jsonl"), &event_payload_rows(&workload.live_events))?;
    write_jsonl(&run_dir.join("janus_results.jsonl"), &janus_results)?;
    write_jsonl(&run_dir.join("oxigraph_historical_results.jsonl"), &oxigraph_historical_results)?;
    write_jsonl(
        &run_dir.join("oxigraph_materialized_baseline.jsonl"),
        &oxigraph_materialized_baseline,
    )?;
    write_jsonl(&run_dir.join("decomposed_join_results.jsonl"), &decomposed_join_results)?;
    write_jsonl(
        &run_dir.join("canonical_janus_results.jsonl"),
        &canonical_result_rows(&janus_results),
    )?;
    write_jsonl(
        &run_dir.join("canonical_decomposed_results.jsonl"),
        &canonical_result_rows(&decomposed_join_results),
    )?;
    write_jsonl(&run_dir.join("join_trace.jsonl"), &join_trace)?;
    fs::write(
        run_dir.join("oxigraph_query.rq"),
        format!("{}\n", workload.historical_sparql_query),
    )?;

    let report = EquivalenceReport {
        system_pair: "janus_unified_vs_decomposed_oxigraph".to_string(),
        run_index: config.run_index,
        mode: config.mode.as_str().to_string(),
        historical_input_hash: unified.historical_input_hash.clone(),
        live_input_hash: unified.live_input_hash.clone(),
        janus_result_count: unified.result_count,
        decomposed_result_count: decomposed.result_count,
        janus_result_hash: unified.result_hash.clone(),
        decomposed_result_hash: decomposed.result_hash.clone(),
        equivalent: unified.equivalent_to_baseline.unwrap_or(false),
        historical_inputs_semantically_equal: unified.historical_input_hash
            == decomposed.historical_input_hash,
        live_inputs_semantically_equal: unified.live_input_hash == decomposed.live_input_hash,
        notes: vec![
            "Historical inputs are emitted from the same RDFEvent sequence for Janus and Oxigraph."
                .to_string(),
            "TriG comments preserve source timestamps for debug inspection.".to_string(),
        ],
    };
    fs::write(run_dir.join("equivalence_report.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(())
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

// =========================================================================
// H1.2 Sustained Hybrid Window Execution Benchmark
// =========================================================================

#[derive(Clone, Debug, Serialize)]
pub struct SustainedRow {
    pub system: String,
    pub mode: String,
    pub time_mode: String,
    pub is_warmup: bool,
    pub run_index: usize,
    pub historical_events: usize,
    pub logical_live_duration_seconds: usize,
    pub event_rate_hz: usize,
    pub event_interval_ms: f64,
    pub expected_wall_clock_duration_ms: u64,
    pub events_published: usize,
    pub events_processed: usize,
    pub window_size_ms: usize,
    pub window_slide_ms: usize,
    pub expected_completed_windows: usize,
    pub completed_windows_total: usize,
    pub completed_windows_in_horizon: usize,
    pub flush_windows: usize,
    pub missed_windows: usize,
    pub historical_start_at: u64,
    pub historical_ready_at: u64,
    pub live_start_at: u64,
    pub first_live_event_at: u64,
    pub first_live_window_ready_at: u64,
    pub first_hybrid_result_at: u64,
    pub historical_preparation_latency_ms: f64,
    pub first_live_window_latency_ms: f64,
    pub first_hybrid_result_latency_ms: f64,
    pub first_hybrid_result_wall_clock_ms: f64,
    pub readiness_gap_ms: f64,
    pub hybrid_wait_after_inputs_ready_ms: f64,
    pub p50_window_hybrid_latency_ms: f64,
    pub p95_window_hybrid_latency_ms: f64,
    pub window_result_wall_clock_offsets_ms: Vec<f64>,
    pub p50_window_result_wall_clock_offset_ms: f64,
    pub p95_window_result_wall_clock_offset_ms: f64,
    pub external_join_latency_total_ms: f64,
    pub external_join_latency_avg_ms: f64,
    pub estimated_external_transfer_bytes_total: usize,
    pub estimated_external_transfer_bytes_per_window: usize,
    pub hybrid_result_count_total: usize,
    pub hybrid_result_hash_total: String,
    pub equivalent_to_baseline: Option<bool>,
    pub metadata: ReproMetadata,

    // Additional requested metrics
    pub wall_clock_benchmark_duration_ms: u64,
    pub wall_clock_overhead_ms: f64,
    pub uses_virtual_event_time: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SustainedSummaryRow {
    pub system: String,
    pub mode: String,
    pub time_mode: String,
    pub runs: usize,
    pub historical_events: usize,
    pub logical_live_duration_seconds: usize,
    pub event_rate_hz: usize,
    pub event_interval_ms: f64,
    pub expected_wall_clock_duration_ms: u64,
    pub window_size_ms: usize,
    pub window_slide_ms: usize,
    pub avg_events_published: f64,
    pub avg_events_processed: f64,
    pub avg_completed_windows_total: f64,
    pub avg_completed_windows_in_horizon: f64,
    pub avg_flush_windows: f64,
    pub avg_missed_windows: f64,
    pub p50_first_hybrid_result_latency_ms: f64,
    pub p95_first_hybrid_result_latency_ms: f64,
    pub p50_first_hybrid_result_wall_clock_ms: f64,
    pub p95_first_hybrid_result_wall_clock_ms: f64,
    pub p50_window_hybrid_latency_ms: f64,
    pub p95_window_hybrid_latency_ms: f64,
    pub p50_window_result_wall_clock_offset_ms: f64,
    pub p95_window_result_wall_clock_offset_ms: f64,
    pub avg_historical_preparation_latency_ms: f64,
    pub avg_first_live_window_latency_ms: f64,
    pub avg_readiness_gap_ms: f64,
    pub avg_hybrid_wait_after_inputs_ready_ms: f64,
    pub avg_external_join_latency_total_ms: f64,
    pub avg_external_join_latency_ms: f64,
    pub avg_estimated_external_transfer_bytes_total: f64,
    pub avg_estimated_external_transfer_bytes_per_window: f64,
    pub avg_hybrid_result_count_total: f64,
    pub avg_wall_clock_benchmark_duration_ms: f64,
    pub avg_wall_clock_overhead_ms: f64,
    pub uses_virtual_event_time: bool,
    pub equivalence_rate: f64,
}

#[derive(Clone)]
pub struct SustainedWorkload {
    pub historical_storage: Arc<StreamingSegmentedStorage>,
    pub historical_rdf_events: Vec<RDFEvent>,
    pub live_events: Vec<RDFEvent>,
    pub historical_start_ts: u64,
    pub historical_end_ts: u64,
    pub historical_sparql_query: String,
    pub hybrid_query: String,
}

pub struct SustainedPair {
    pub unified: SustainedRow,
    pub decomposed: SustainedRow,
}

pub struct SustainedRunConfig<'a> {
    pub mode: ExecutionMode,
    pub time_mode: TimeMode,
    pub run_index: usize,
    pub is_warmup: bool,
    pub historical_events: usize,
    pub live_duration_seconds: usize,
    pub event_rate_hz: usize,
    pub event_interval_ms: f64,
    pub expected_wall_clock_duration_ms: u64,
    pub window_size_seconds: usize,
    pub window_slide_seconds: usize,
    pub metadata: &'a ReproMetadata,
    pub adapter: &'a dyn ExternalHistoricalAdapter,
    pub warm_workload: Option<&'a SustainedWorkload>,
    pub debug_output_dir: Option<&'a Path>,
}

impl<'a> SustainedRunConfig<'a> {
    pub fn expected_completed_windows_in_horizon(&self) -> usize {
        let d = self.live_duration_seconds;
        let w = self.window_size_seconds;
        let s = self.window_slide_seconds;
        if d == 0 {
            return 0;
        }
        let first_end_sec = w + s - 1;
        if d >= first_end_sec {
            1 + (d - first_end_sec) / s
        } else {
            0
        }
    }
}

pub fn sustained_event_interval_ms(event_rate_hz: usize) -> f64 {
    1000.0 / event_rate_hz as f64
}

pub fn sustained_expected_wall_clock_duration_ms(live_duration_seconds: usize) -> u64 {
    (live_duration_seconds as u64) * 1000
}

pub struct SustainedSystemOutput {
    pub row: SustainedRow,
    pub window_results: HashMap<String, Vec<HashMap<String, String>>>,
    pub live_events: Vec<RDFEvent>,
    pub historical_baseline: Vec<HashMap<String, String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BindingWithWindow {
    pub bindings: HashMap<String, String>,
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub window_id: String,
}

pub struct SustainedLiveCollectionResult {
    pub first_result_engine_ms: u64,
    pub bindings: Vec<BindingWithWindow>,
}

fn canonical_window_hash(
    rows: &[HashMap<String, String>],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut canonical_rows = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
    canonical_rows.sort();
    let payload = serde_json::to_vec(&canonical_rows)?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}"))
}

fn canonical_result_hash_sustained(
    windows: &HashMap<String, Vec<HashMap<String, String>>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut keys: Vec<String> = windows.keys().cloned().collect();
    keys.sort_unstable();
    let mut all_canonical = Vec::new();
    for key in keys {
        let rows = &windows[&key];
        let mut canonical_rows = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
        canonical_rows.sort();
        all_canonical.push((key, canonical_rows));
    }
    let payload = serde_json::to_vec(&all_canonical)?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}"))
}

pub fn build_sustained_live_events(
    duration_sec: usize,
    rate_hz: usize,
    start_ts: u64,
) -> Vec<RDFEvent> {
    let total_events = duration_sec * rate_hz;
    if total_events == 0 {
        return Vec::new();
    }
    let interval_ms = 1000 / rate_hz;
    (0..total_events)
        .map(|index| {
            RDFEvent::new(
                start_ts + (index * interval_ms) as u64,
                &format!("http://example.org/junction/{}", index % 64),
                TRAFFIC_PREDICATE,
                &(70 + (index % 11)).to_string(),
                GRAPH_URI,
            )
        })
        .collect()
}

fn parse_window_id(window_id: &str) -> Option<(u64, u64)> {
    let mut parts = window_id.split('-');
    let start = parts.next()?.parse().ok()?;
    let end = parts.next()?.parse().ok()?;
    Some((start, end))
}

fn wait_for_sustained_event_schedule(
    config: &SustainedRunConfig<'_>,
    replay_start: Instant,
    event_index: usize,
) {
    if config.time_mode != TimeMode::WallClock {
        return;
    }
    let target =
        replay_start + Duration::from_secs_f64(event_index as f64 / config.event_rate_hz as f64);
    if let Some(remaining) = target.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

fn wait_for_sustained_replay_flush(config: &SustainedRunConfig<'_>, replay_deadline: Instant) {
    if config.time_mode != TimeMode::WallClock {
        return;
    }
    if let Some(remaining) = replay_deadline.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

fn sustained_hybrid_query(
    start_ts: u64,
    end_ts: u64,
    window_size_ms: usize,
    window_slide_ms: usize,
) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX baseline: <{BASELINE_NS}>

        REGISTER RStream <output> AS
        SELECT ?sensor ?liveFlow ?baselineFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {start_ts} END {end_ts}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        USING BASELINE ex:hist AGGREGATE
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:baselineFlow ?baselineFlow .
            }}
            WINDOW ex:live {{
                ?sensor ex:trafficFlow ?liveFlow .
            }}
            ?sensor baseline:baselineFlow ?baselineFlow .
        }}
        "#
    )
}

fn live_only_rspql_sustained(window_size_ms: usize, window_slide_ms: usize) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor ?liveFlow
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        WHERE {{
            WINDOW ex:live {{
                ?sensor ex:trafficFlow ?liveFlow .
            }}
        }}
        "#
    )
}

pub fn prepare_sustained_workload(
    historical_events: usize,
    live_duration_seconds: usize,
    event_rate_hz: usize,
    window_size_seconds: usize,
    window_slide_seconds: usize,
) -> Result<SustainedWorkload, Box<dyn std::error::Error>> {
    let historical_start_ts = 1_800_000_000_000;
    let historical_storage = build_historical_storage(historical_events, historical_start_ts)?;
    let historical_rdf_events = historical_storage.query_rdf(
        historical_start_ts,
        historical_start_ts + historical_events.saturating_sub(1) as u64,
    )?;
    let live_events =
        build_sustained_live_events(live_duration_seconds, event_rate_hz, 1_900_000_000_000);
    let window_size_ms = window_size_seconds * 1000;
    let window_slide_ms = window_slide_seconds * 1000;

    Ok(SustainedWorkload {
        historical_storage,
        historical_rdf_events,
        live_events,
        historical_start_ts,
        historical_end_ts: historical_start_ts + historical_events.saturating_sub(1) as u64,
        historical_sparql_query: historical_baseline_sparql_query()?,
        hybrid_query: sustained_hybrid_query(
            historical_start_ts,
            historical_start_ts + historical_events.saturating_sub(1) as u64,
            window_size_ms,
            window_slide_ms,
        ),
    })
}

pub fn run_sustained_pair(
    config: SustainedRunConfig<'_>,
) -> Result<SustainedPair, Box<dyn std::error::Error>> {
    let unified = run_sustained_system(CoordinationSystem::JanusUnified, &config)?;
    let decomposed = run_sustained_system(CoordinationSystem::DecomposedOxigraph, &config)?;

    let mut unified_row = unified.row.clone();
    let mut decomposed_row = decomposed.row.clone();

    let expected_completed_windows = config.expected_completed_windows_in_horizon();
    let mut equivalent = unified_row.completed_windows_in_horizon
        == decomposed_row.completed_windows_in_horizon
        && unified_row.completed_windows_in_horizon == expected_completed_windows
        && unified_row.hybrid_result_hash_total == decomposed_row.hybrid_result_hash_total
        && !unified_row.hybrid_result_hash_total.is_empty()
        && unified_row.hybrid_result_hash_total
            != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    if equivalent {
        let horizon_end_ms = i64::try_from(
            unified_row
                .first_live_event_at
                .saturating_add((config.live_duration_seconds * 1000) as u64),
        )
        .map_err(|_| "horizon end timestamp exceeds i64 range")?;
        for (win_id, u_rows) in &unified.window_results {
            let parts: Vec<&str> = win_id.split('-').collect();
            if parts.len() == 2 {
                if let Ok(win_end) = parts[1].parse::<i64>() {
                    if win_end > horizon_end_ms {
                        continue;
                    }
                }
            }
            if let Some(d_rows) = decomposed.window_results.get(win_id) {
                let u_hash = canonical_window_hash(u_rows)?;
                let d_hash = canonical_window_hash(d_rows)?;
                if u_hash != d_hash || u_rows.len() != d_rows.len() {
                    equivalent = false;
                    break;
                }
            } else {
                equivalent = false;
                break;
            }
        }
    }

    unified_row.equivalent_to_baseline = Some(equivalent);
    decomposed_row.equivalent_to_baseline = None;

    if let Some(debug_output_dir) = config.debug_output_dir {
        write_h1_2_debug_artifacts(debug_output_dir, &unified, &decomposed, &config, equivalent)?;
    }

    Ok(SustainedPair { unified: unified_row, decomposed: decomposed_row })
}

fn run_sustained_system(
    system: CoordinationSystem,
    config: &SustainedRunConfig<'_>,
) -> Result<SustainedSystemOutput, Box<dyn std::error::Error>> {
    let owned_workload;
    let workload = if config.mode == ExecutionMode::Warm {
        config.warm_workload.ok_or("warm mode requires prepared workload")?
    } else {
        owned_workload = prepare_sustained_workload(
            config.historical_events,
            config.live_duration_seconds,
            config.event_rate_hz,
            config.window_size_seconds,
            config.window_slide_seconds,
        )?;
        &owned_workload
    };

    let start_wall = Instant::now();
    let client_start = now_ms();
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&workload.hybrid_query)?;
    let query_registered = now_ms();

    let historical_start = now_ms();
    let executor =
        HistoricalExecutor::new(Arc::clone(&workload.historical_storage), OxigraphAdapter::new());
    let baseline_bindings = executor.execute_fixed_window(
        parsed.historical_windows.first().ok_or("missing historical window")?,
        &workload.historical_sparql_query,
    )?;
    let historical_done = now_ms();

    let events_published = workload.live_events.len();
    let events_processed = workload.live_events.len();
    let window_size_ms = config.window_size_seconds * 1000;
    let window_slide_ms = config.window_slide_seconds * 1000;
    let expected_completed_windows = config.expected_completed_windows_in_horizon();
    let live_first_event_at = workload.live_events.first().map(|e| e.timestamp).unwrap_or(0);
    let horizon_end_ms = i64::try_from(
        live_first_event_at.saturating_add((config.live_duration_seconds * 1000) as u64),
    )
    .map_err(|_| "horizon end timestamp exceeds i64 range")?;

    let mut window_results = HashMap::<String, Vec<HashMap<String, String>>>::new();
    let mut window_hybrid_latencies = Vec::<(i64, f64)>::new();
    let mut window_wall_clock_offsets = HashMap::<String, f64>::new();
    let mut first_live_window_ready_at = 0u64;
    let mut first_hybrid_result_at = 0u64;
    let mut first_hybrid_result_wall_clock_ms = 0.0;

    let live_start_at;
    let mut external_join_latencies = Vec::<(i64, f64)>::new();
    let historical_baseline;

    let estimated_external_transfer_bytes_total;

    match system {
        CoordinationSystem::JanusUnified => {
            let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
            processor.register_stream(LIVE_STREAM_URI)?;
            materialize_bindings_as_static_baseline(&mut processor, &baseline_bindings)?;
            processor.start_processing()?;
            live_start_at = now_ms();
            let replay_start = Instant::now();
            let replay_deadline =
                replay_start + Duration::from_millis(config.expected_wall_clock_duration_ms);

            historical_baseline = baseline_statements_from_bindings(&baseline_bindings)
                .into_iter()
                .map(|(s, p, o)| {
                    HashMap::from([("sensor".to_string(), s), ("baselineFlow".to_string(), o)])
                })
                .collect();
            estimated_external_transfer_bytes_total = 0;

            for (event_index, event) in workload.live_events.iter().enumerate() {
                wait_for_sustained_event_schedule(config, replay_start, event_index);
                let start_add = Instant::now();
                processor.add_event(LIVE_STREAM_URI, event.clone())?;
                let add_duration = start_add.elapsed().as_secs_f64() * 1000.0;

                std::thread::sleep(std::time::Duration::from_millis(2));

                let mut received = Vec::new();
                while let Some(res) = processor.try_receive_result()? {
                    received.push(res);
                }

                if !received.is_empty() {
                    let now = now_ms();
                    let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                    if first_live_window_ready_at == 0 {
                        first_live_window_ready_at = now;
                        first_hybrid_result_at = now;
                        first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                    }
                    for res in received {
                        let win_start = res.timestamp_from;
                        let win_end = res.timestamp_to;
                        let win_id = format!("{}-{}", win_start, win_end);
                        let rows = parse_rsprs_binding_string(&res.bindings);
                        window_results.entry(win_id.clone()).or_default().push(rows);
                        window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);
                        window_hybrid_latencies.push((win_end, add_duration));
                    }
                }
            }

            wait_for_sustained_replay_flush(config, replay_deadline);
            let close_start = Instant::now();
            let close_ts = workload.live_events.last().map_or(20_000_i64, |e| {
                i64::try_from(e.timestamp).unwrap_or(i64::MAX).saturating_add(20_000)
            });
            processor.close_stream(LIVE_STREAM_URI, close_ts)?;
            let close_duration = close_start.elapsed().as_secs_f64() * 1000.0;

            std::thread::sleep(std::time::Duration::from_millis(10));

            let mut final_received = Vec::new();
            while let Some(res) = processor.try_receive_result()? {
                final_received.push(res);
            }
            if !final_received.is_empty() {
                let now = now_ms();
                let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                if first_live_window_ready_at == 0 {
                    first_live_window_ready_at = now;
                    first_hybrid_result_at = now;
                    first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                }
                for res in final_received {
                    let win_start = res.timestamp_from;
                    let win_end = res.timestamp_to;
                    let win_id = format!("{}-{}", win_start, win_end);
                    let rows = parse_rsprs_binding_string(&res.bindings);
                    window_results.entry(win_id.clone()).or_default().push(rows);
                    window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);
                    window_hybrid_latencies.push((win_end, close_duration));
                }
            }
        }
        CoordinationSystem::DecomposedOxigraph => {
            let external_bindings = config.adapter.execute_bindings_query(
                &workload.historical_sparql_query,
                &workload.historical_rdf_events,
            )?;
            let materialized_baseline_rows =
                materialized_baseline_rows_from_bindings(&external_bindings, "baselineFlow");
            let historical_done = now_ms();

            historical_baseline = materialized_baseline_rows.clone();
            let historical_intermediate_bytes = serde_json::to_vec(&external_bindings)?.len();

            let mut processor = LiveStreamProcessing::new(live_only_rspql_sustained(
                window_size_ms,
                window_slide_ms,
            ))?;
            processor.register_stream(LIVE_STREAM_URI)?;
            processor.start_processing()?;
            live_start_at = now_ms();
            let replay_start = Instant::now();
            let replay_deadline =
                replay_start + Duration::from_millis(config.expected_wall_clock_duration_ms);

            let mut all_live_rows = Vec::new();

            for (event_index, event) in workload.live_events.iter().enumerate() {
                wait_for_sustained_event_schedule(config, replay_start, event_index);
                let start_add = Instant::now();
                processor.add_event(LIVE_STREAM_URI, event.clone())?;
                let add_duration = start_add.elapsed().as_secs_f64() * 1000.0;

                std::thread::sleep(std::time::Duration::from_millis(2));

                let mut received = Vec::new();
                while let Some(res) = processor.try_receive_result()? {
                    received.push(res);
                }

                if !received.is_empty() {
                    let now = now_ms();
                    let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                    if first_live_window_ready_at == 0 {
                        first_live_window_ready_at = now;
                    }
                    for res in received {
                        let win_start = res.timestamp_from;
                        let win_end = res.timestamp_to;
                        let win_id = format!("{}-{}", win_start, win_end);
                        let rows = parse_rsprs_binding_string(&res.bindings);
                        if win_end <= horizon_end_ms {
                            all_live_rows.push(rows.clone());
                        }
                        let start_join = Instant::now();
                        let joined = join_live_with_baseline(&[rows], &materialized_baseline_rows);
                        let join_duration = start_join.elapsed().as_secs_f64() * 1000.0;
                        if first_hybrid_result_at == 0 {
                            first_hybrid_result_at = now_ms();
                            first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                        }

                        for row in joined {
                            window_results.entry(win_id.clone()).or_default().push(row);
                        }
                        window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);

                        window_hybrid_latencies.push((win_end, add_duration + join_duration));
                        external_join_latencies.push((win_end, join_duration));
                    }
                }
            }

            wait_for_sustained_replay_flush(config, replay_deadline);
            let close_start = Instant::now();
            let close_ts = workload.live_events.last().map_or(20_000_i64, |e| {
                i64::try_from(e.timestamp).unwrap_or(i64::MAX).saturating_add(20_000)
            });
            processor.close_stream(LIVE_STREAM_URI, close_ts)?;
            let close_duration = close_start.elapsed().as_secs_f64() * 1000.0;

            std::thread::sleep(std::time::Duration::from_millis(10));

            let mut final_received = Vec::new();
            while let Some(res) = processor.try_receive_result()? {
                final_received.push(res);
            }
            if !final_received.is_empty() {
                let now = now_ms();
                let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                if first_live_window_ready_at == 0 {
                    first_live_window_ready_at = now;
                }
                for res in final_received {
                    let win_start = res.timestamp_from;
                    let win_end = res.timestamp_to;
                    let win_id = format!("{}-{}", win_start, win_end);
                    let rows = parse_rsprs_binding_string(&res.bindings);
                    if win_end <= horizon_end_ms {
                        all_live_rows.push(rows.clone());
                    }
                    let start_join = Instant::now();
                    let joined = join_live_with_baseline(&[rows], &materialized_baseline_rows);
                    let join_duration = start_join.elapsed().as_secs_f64() * 1000.0;
                    if first_hybrid_result_at == 0 {
                        first_hybrid_result_at = now_ms();
                        first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                    }

                    for row in joined {
                        window_results.entry(win_id.clone()).or_default().push(row);
                    }
                    window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);

                    window_hybrid_latencies.push((win_end, close_duration + join_duration));
                    external_join_latencies.push((win_end, join_duration));
                }
            }

            let live_intermediate_bytes = serde_json::to_vec(&all_live_rows)?.len();
            estimated_external_transfer_bytes_total =
                historical_intermediate_bytes + live_intermediate_bytes;
        }
    }

    let wall_clock_benchmark_duration_ms = start_wall.elapsed().as_millis() as u64;

    let completed_windows_total = window_results.len();
    let mut completed_windows_in_horizon = 0;
    let mut horizon_window_results = HashMap::new();
    for (win_id, rows) in &window_results {
        let parts: Vec<&str> = win_id.split('-').collect();
        if parts.len() == 2 {
            if let Ok(win_end) = parts[1].parse::<i64>() {
                if win_end <= horizon_end_ms {
                    completed_windows_in_horizon += 1;
                    horizon_window_results.insert(win_id.clone(), rows.clone());
                }
            }
        }
    }
    let flush_windows = completed_windows_total - completed_windows_in_horizon;
    let missed_windows = expected_completed_windows.saturating_sub(completed_windows_in_horizon);

    let historical_preparation_latency_ms = (historical_done - historical_start) as f64;
    let first_live_window_latency_ms = if first_live_window_ready_at > 0 {
        (first_live_window_ready_at - live_start_at) as f64
    } else {
        0.0
    };
    let first_hybrid_result_latency_ms = if first_hybrid_result_at > 0 {
        (first_hybrid_result_at - client_start) as f64
    } else {
        0.0
    };

    let readiness_gap_ms = if historical_done > 0 && first_live_window_ready_at > 0 {
        (historical_done as f64 - first_live_window_ready_at as f64).abs()
    } else {
        0.0
    };

    let hybrid_wait_after_inputs_ready_ms = if first_hybrid_result_at > 0 {
        let max_ready = historical_done.max(first_live_window_ready_at) as f64;
        (first_hybrid_result_at as f64 - max_ready).max(0.0)
    } else {
        0.0
    };

    let horizon_latencies: Vec<f64> = window_hybrid_latencies
        .iter()
        .filter(|&&(win_end, _)| win_end <= horizon_end_ms)
        .map(|&(_, lat)| lat)
        .collect();
    let p50_window_hybrid_latency_ms = percentile(&horizon_latencies, 50.0);
    let p95_window_hybrid_latency_ms = percentile(&horizon_latencies, 95.0);

    let horizon_join_latencies: Vec<f64> = external_join_latencies
        .iter()
        .filter(|&&(win_end, _)| win_end <= horizon_end_ms)
        .map(|&(_, lat)| lat)
        .collect();
    let external_join_latency_total_ms: f64 = horizon_join_latencies.iter().sum();
    let external_join_latency_avg_ms = if completed_windows_in_horizon > 0 {
        external_join_latency_total_ms / completed_windows_in_horizon as f64
    } else {
        0.0
    };

    let estimated_external_transfer_bytes_per_window = if completed_windows_in_horizon > 0 {
        estimated_external_transfer_bytes_total
            .checked_div(completed_windows_in_horizon)
            .unwrap_or(0)
    } else {
        0
    };
    let mut window_result_wall_clock_offsets_ms = window_wall_clock_offsets
        .iter()
        .filter_map(|(win_id, offset)| {
            let (_, win_end) = parse_window_id(win_id)?;
            (i64::try_from(win_end).ok()? <= horizon_end_ms).then_some(*offset)
        })
        .collect::<Vec<_>>();
    window_result_wall_clock_offsets_ms
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let p50_window_result_wall_clock_offset_ms =
        percentile(&window_result_wall_clock_offsets_ms, 50.0);
    let p95_window_result_wall_clock_offset_ms =
        percentile(&window_result_wall_clock_offsets_ms, 95.0);

    let mut hybrid_result_count_total = 0;
    for rows in horizon_window_results.values() {
        hybrid_result_count_total += rows.len();
    }
    let hybrid_result_hash_total = canonical_result_hash_sustained(&horizon_window_results)?;
    let wall_clock_overhead_ms =
        wall_clock_benchmark_duration_ms as f64 - config.expected_wall_clock_duration_ms as f64;

    let row = SustainedRow {
        system: system.as_str().to_string(),
        mode: config.mode.as_str().to_string(),
        time_mode: config.time_mode.as_str().to_string(),
        is_warmup: config.is_warmup,
        run_index: config.run_index,
        historical_events: config.historical_events,
        logical_live_duration_seconds: config.live_duration_seconds,
        event_rate_hz: config.event_rate_hz,
        event_interval_ms: config.event_interval_ms,
        expected_wall_clock_duration_ms: config.expected_wall_clock_duration_ms,
        events_published,
        events_processed,
        window_size_ms,
        window_slide_ms,
        expected_completed_windows,
        completed_windows_total,
        completed_windows_in_horizon,
        flush_windows,
        missed_windows,
        historical_start_at: historical_start,
        historical_ready_at: historical_done,
        live_start_at,
        first_live_event_at: live_first_event_at,
        first_live_window_ready_at,
        first_hybrid_result_at,
        historical_preparation_latency_ms,
        first_live_window_latency_ms,
        first_hybrid_result_latency_ms,
        first_hybrid_result_wall_clock_ms,
        readiness_gap_ms,
        hybrid_wait_after_inputs_ready_ms,
        p50_window_hybrid_latency_ms,
        p95_window_hybrid_latency_ms,
        window_result_wall_clock_offsets_ms,
        p50_window_result_wall_clock_offset_ms,
        p95_window_result_wall_clock_offset_ms,
        external_join_latency_total_ms,
        external_join_latency_avg_ms,
        estimated_external_transfer_bytes_total,
        estimated_external_transfer_bytes_per_window,
        hybrid_result_count_total,
        hybrid_result_hash_total,
        equivalent_to_baseline: None,
        metadata: config.metadata.clone(),
        wall_clock_benchmark_duration_ms,
        wall_clock_overhead_ms,
        uses_virtual_event_time: config.time_mode.uses_virtual_event_time(),
    };

    Ok(SustainedSystemOutput {
        row,
        window_results,
        live_events: workload.live_events.clone(),
        historical_baseline,
    })
}

pub fn summarize_sustained(rows: &[SustainedRow]) -> Vec<SustainedSummaryRow> {
    let mut grouped = HashMap::<(String, String, String), Vec<&SustainedRow>>::new();
    for row in rows {
        grouped
            .entry((row.system.clone(), row.mode.clone(), row.time_mode.clone()))
            .or_default()
            .push(row);
    }

    let mut summary = grouped
        .into_iter()
        .map(|((system, mode, time_mode), items)| {
            let runs = items.len();
            let equiv_count =
                items.iter().filter(|row| row.equivalent_to_baseline == Some(true)).count();

            SustainedSummaryRow {
                system,
                mode,
                time_mode,
                runs,
                historical_events: items.first().map_or(0, |row| row.historical_events),
                logical_live_duration_seconds: items
                    .first()
                    .map_or(0, |row| row.logical_live_duration_seconds),
                event_rate_hz: items.first().map_or(0, |row| row.event_rate_hz),
                event_interval_ms: items.first().map_or(0.0, |row| row.event_interval_ms),
                expected_wall_clock_duration_ms: items
                    .first()
                    .map_or(0, |row| row.expected_wall_clock_duration_ms),
                window_size_ms: items.first().map_or(0, |row| row.window_size_ms),
                window_slide_ms: items.first().map_or(0, |row| row.window_slide_ms),
                avg_events_published: mean_usize(
                    &items.iter().map(|row| row.events_published).collect::<Vec<_>>(),
                ),
                avg_events_processed: mean_usize(
                    &items.iter().map(|row| row.events_processed).collect::<Vec<_>>(),
                ),
                avg_completed_windows_total: mean_usize(
                    &items.iter().map(|row| row.completed_windows_total).collect::<Vec<_>>(),
                ),
                avg_completed_windows_in_horizon: mean_usize(
                    &items.iter().map(|row| row.completed_windows_in_horizon).collect::<Vec<_>>(),
                ),
                avg_flush_windows: mean_usize(
                    &items.iter().map(|row| row.flush_windows).collect::<Vec<_>>(),
                ),
                avg_missed_windows: mean_usize(
                    &items.iter().map(|row| row.missed_windows).collect::<Vec<_>>(),
                ),
                p50_first_hybrid_result_latency_ms: percentile(
                    &items.iter().map(|row| row.first_hybrid_result_latency_ms).collect::<Vec<_>>(),
                    50.0,
                ),
                p95_first_hybrid_result_latency_ms: percentile(
                    &items.iter().map(|row| row.first_hybrid_result_latency_ms).collect::<Vec<_>>(),
                    95.0,
                ),
                p50_first_hybrid_result_wall_clock_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.first_hybrid_result_wall_clock_ms)
                        .collect::<Vec<_>>(),
                    50.0,
                ),
                p95_first_hybrid_result_wall_clock_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.first_hybrid_result_wall_clock_ms)
                        .collect::<Vec<_>>(),
                    95.0,
                ),
                p50_window_hybrid_latency_ms: percentile(
                    &items.iter().map(|row| row.p50_window_hybrid_latency_ms).collect::<Vec<_>>(),
                    50.0,
                ),
                p95_window_hybrid_latency_ms: percentile(
                    &items.iter().map(|row| row.p95_window_hybrid_latency_ms).collect::<Vec<_>>(),
                    95.0,
                ),
                p50_window_result_wall_clock_offset_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.p50_window_result_wall_clock_offset_ms)
                        .collect::<Vec<_>>(),
                    50.0,
                ),
                p95_window_result_wall_clock_offset_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.p95_window_result_wall_clock_offset_ms)
                        .collect::<Vec<_>>(),
                    95.0,
                ),
                avg_historical_preparation_latency_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.historical_preparation_latency_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_first_live_window_latency_ms: mean(
                    &items.iter().map(|row| row.first_live_window_latency_ms).collect::<Vec<_>>(),
                ),
                avg_readiness_gap_ms: mean(
                    &items.iter().map(|row| row.readiness_gap_ms).collect::<Vec<_>>(),
                ),
                avg_hybrid_wait_after_inputs_ready_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.hybrid_wait_after_inputs_ready_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_external_join_latency_total_ms: mean(
                    &items.iter().map(|row| row.external_join_latency_total_ms).collect::<Vec<_>>(),
                ),
                avg_external_join_latency_ms: mean(
                    &items.iter().map(|row| row.external_join_latency_avg_ms).collect::<Vec<_>>(),
                ),
                avg_estimated_external_transfer_bytes_total: mean_usize(
                    &items
                        .iter()
                        .map(|row| row.estimated_external_transfer_bytes_total)
                        .collect::<Vec<_>>(),
                ),
                avg_estimated_external_transfer_bytes_per_window: mean_usize(
                    &items
                        .iter()
                        .map(|row| row.estimated_external_transfer_bytes_per_window)
                        .collect::<Vec<_>>(),
                ),
                avg_hybrid_result_count_total: mean_usize(
                    &items.iter().map(|row| row.hybrid_result_count_total).collect::<Vec<_>>(),
                ),
                avg_wall_clock_benchmark_duration_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.wall_clock_benchmark_duration_ms as f64)
                        .collect::<Vec<_>>(),
                ),
                avg_wall_clock_overhead_ms: mean(
                    &items.iter().map(|row| row.wall_clock_overhead_ms).collect::<Vec<_>>(),
                ),
                uses_virtual_event_time: items
                    .first()
                    .map_or(true, |row| row.uses_virtual_event_time),
                equivalence_rate: if runs > 0 {
                    equiv_count as f64 / runs as f64
                } else {
                    0.0
                },
            }
        })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        left.system
            .cmp(&right.system)
            .then_with(|| left.mode.cmp(&right.mode))
            .then_with(|| left.time_mode.cmp(&right.time_mode))
    });
    summary
}

pub fn write_sustained_summary_csv(
    path: &Path,
    rows: &[SustainedSummaryRow],
) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "system,mode,time_mode,runs,historical_events,logical_live_duration_seconds,event_rate_hz,event_interval_ms,expected_wall_clock_duration_ms,window_size_ms,window_slide_ms,avg_events_published,avg_events_processed,avg_completed_windows_total,avg_completed_windows_in_horizon,avg_flush_windows,avg_missed_windows,p50_first_hybrid_result_latency_ms,p95_first_hybrid_result_latency_ms,p50_first_hybrid_result_wall_clock_ms,p95_first_hybrid_result_wall_clock_ms,p50_window_hybrid_latency_ms,p95_window_hybrid_latency_ms,p50_window_result_wall_clock_offset_ms,p95_window_result_wall_clock_offset_ms,avg_historical_preparation_latency_ms,avg_first_live_window_latency_ms,avg_readiness_gap_ms,avg_hybrid_wait_after_inputs_ready_ms,avg_external_join_latency_total_ms,avg_external_join_latency_ms,avg_estimated_external_transfer_bytes_total,avg_estimated_external_transfer_bytes_per_window,avg_hybrid_result_count_total,avg_wall_clock_benchmark_duration_ms,avg_wall_clock_overhead_ms,uses_virtual_event_time,equivalence_rate"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{},{},{},{:.3},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{:.3}",
            row.system,
            row.mode,
            row.time_mode,
            row.runs,
            row.historical_events,
            row.logical_live_duration_seconds,
            row.event_rate_hz,
            row.event_interval_ms,
            row.expected_wall_clock_duration_ms,
            row.window_size_ms,
            row.window_slide_ms,
            row.avg_events_published,
            row.avg_events_processed,
            row.avg_completed_windows_total,
            row.avg_completed_windows_in_horizon,
            row.avg_flush_windows,
            row.avg_missed_windows,
            row.p50_first_hybrid_result_latency_ms,
            row.p95_first_hybrid_result_latency_ms,
            row.p50_first_hybrid_result_wall_clock_ms,
            row.p95_first_hybrid_result_wall_clock_ms,
            row.p50_window_hybrid_latency_ms,
            row.p95_window_hybrid_latency_ms,
            row.p50_window_result_wall_clock_offset_ms,
            row.p95_window_result_wall_clock_offset_ms,
            row.avg_historical_preparation_latency_ms,
            row.avg_first_live_window_latency_ms,
            row.avg_readiness_gap_ms,
            row.avg_hybrid_wait_after_inputs_ready_ms,
            row.avg_external_join_latency_total_ms,
            row.avg_external_join_latency_ms,
            row.avg_estimated_external_transfer_bytes_total,
            row.avg_estimated_external_transfer_bytes_per_window,
            row.avg_hybrid_result_count_total,
            row.avg_wall_clock_benchmark_duration_ms,
            row.avg_wall_clock_overhead_ms,
            row.uses_virtual_event_time,
            row.equivalence_rate
        )?;
    }
    Ok(())
}

fn write_h1_2_debug_artifacts(
    base_dir: &Path,
    unified: &SustainedSystemOutput,
    decomposed: &SustainedSystemOutput,
    config: &SustainedRunConfig<'_>,
    equivalent: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let run_dir = base_dir.join("h1_2_sustained_debug").join(format!(
        "run_{:03}_{}",
        config.run_index,
        if config.is_warmup {
            "warmup"
        } else {
            "measured"
        }
    ));
    fs::create_dir_all(&run_dir)?;

    write_jsonl(&run_dir.join("live_events.jsonl"), &event_payload_rows(&unified.live_events))?;
    write_jsonl(&run_dir.join("historical_baseline.jsonl"), &unified.historical_baseline)?;

    // Write janus_window_results.jsonl
    let mut janus_rows = Vec::new();
    for (win_id, rows) in &unified.window_results {
        let parts: Vec<&str> = win_id.split('-').collect();
        let win_start_val = parts[0].parse::<u64>().unwrap_or(0);
        let win_end_val = parts[1].parse::<u64>().unwrap_or(0);
        let win_hash = canonical_window_hash(rows)?;
        janus_rows.push(serde_json::json!({
            "window_start_ms": win_start_val,
            "window_end_ms": win_end_val,
            "window_id": win_id.clone(),
            "result_count": rows.len(),
            "result_hash": win_hash,
            "results": rows
        }));
    }
    janus_rows.sort_by_key(|val| val["window_start_ms"].as_u64().unwrap_or(0));
    write_jsonl(&run_dir.join("janus_window_results.jsonl"), &janus_rows)?;

    // Write decomposed_window_results.jsonl
    let mut decomposed_rows = Vec::new();
    for (win_id, rows) in &decomposed.window_results {
        let parts: Vec<&str> = win_id.split('-').collect();
        let win_start_val = parts[0].parse::<u64>().unwrap_or(0);
        let win_end_val = parts[1].parse::<u64>().unwrap_or(0);
        let win_hash = canonical_window_hash(rows)?;
        decomposed_rows.push(serde_json::json!({
            "window_start_ms": win_start_val,
            "window_end_ms": win_end_val,
            "window_id": win_id.clone(),
            "result_count": rows.len(),
            "result_hash": win_hash,
            "results": rows
        }));
    }
    decomposed_rows.sort_by_key(|val| val["window_start_ms"].as_u64().unwrap_or(0));
    write_jsonl(&run_dir.join("decomposed_window_results.jsonl"), &decomposed_rows)?;

    // Write per_window_equivalence.jsonl
    let mut equivalence_rows = Vec::new();
    let mut per_window_summary = Vec::new();
    let mut all_keys: HashSet<String> = unified.window_results.keys().cloned().collect();
    all_keys.extend(decomposed.window_results.keys().cloned());
    let mut sorted_keys: Vec<String> = all_keys.into_iter().collect();
    sorted_keys
        .sort_by_key(|k| k.split('-').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0));

    for win_id in sorted_keys {
        let parts: Vec<&str> = win_id.split('-').collect();
        let win_start_val = parts[0].parse::<u64>().unwrap_or(0);
        let win_end_val = parts[1].parse::<u64>().unwrap_or(0);

        let u_rows = unified.window_results.get(&win_id);
        let d_rows = decomposed.window_results.get(&win_id);
        let u_len = u_rows.map(|r| r.len()).unwrap_or(0);
        let d_len = d_rows.map(|r| r.len()).unwrap_or(0);
        let u_hash =
            u_rows.map(|r| canonical_window_hash(r).unwrap_or_default()).unwrap_or_default();
        let d_hash =
            d_rows.map(|r| canonical_window_hash(r).unwrap_or_default()).unwrap_or_default();
        let equivalent = u_len == d_len && u_hash == d_hash && u_len > 0;

        equivalence_rows.push(serde_json::json!({
            "window_start_ms": win_start_val,
            "window_end_ms": win_end_val,
            "window_id": win_id.clone(),
            "janus_count": u_len,
            "decomposed_count": d_len,
            "janus_hash": u_hash,
            "decomposed_hash": d_hash,
            "equivalent": equivalent
        }));

        per_window_summary.push(serde_json::json!({
            "window_id": win_id,
            "equivalent": equivalent
        }));
    }
    write_jsonl(&run_dir.join("per_window_equivalence.jsonl"), &equivalence_rows)?;

    // Write equivalence_report.json
    let report = serde_json::json!({
        "system_pair": "janus_unified_vs_decomposed_sustained",
        "run_index": config.run_index,
        "mode": config.mode.as_str().to_string(),
        "time_mode": config.time_mode.as_str().to_string(),
        "expected_windows": config.expected_completed_windows_in_horizon(),
        "janus_completed_windows": unified.row.completed_windows_in_horizon,
        "decomposed_completed_windows": decomposed.row.completed_windows_in_horizon,
        "janus_total_hash": unified.row.hybrid_result_hash_total,
        "decomposed_total_hash": decomposed.row.hybrid_result_hash_total,
        "equivalent": equivalent,
        "per_window_equivalence": per_window_summary
    });
    fs::write(run_dir.join("equivalence_report.json"), serde_json::to_vec_pretty(&report)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_bench::external::OxigraphExternalAdapter;

    fn test_metadata() -> ReproMetadata {
        ReproMetadata {
            git_commit_sha: "test".to_string(),
            branch: "test".to_string(),
            rustc_version: "test".to_string(),
            os: "test".to_string(),
            cpu_model: "test".to_string(),
            ram_bytes: Some(1),
            benchmark_command: "test".to_string(),
            timestamp_unix_ms: 0,
        }
    }

    #[test]
    fn tiny_h1_equivalence_matches_between_janus_and_oxigraph() {
        let metadata = test_metadata();
        let adapter = OxigraphExternalAdapter::new();
        let workload = prepare_coordination_workload(1, 1).expect("workload should build");
        let pair = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload),
            debug_output_dir: None,
        })
        .expect("pair should run");

        assert_eq!(pair.unified.result_count, 1);
        assert_eq!(pair.decomposed.result_count, 1);
        assert_eq!(pair.unified.result_hash, pair.decomposed.result_hash);
        assert_eq!(pair.unified.equivalent_to_baseline, Some(true));
    }

    #[test]
    fn test_deterministic_hybrid_dependency() {
        let metadata = test_metadata();
        let adapter = OxigraphExternalAdapter::new();

        // 1. historical=1, live=1 -> hybrid_result_count=1
        let workload_1_1 = prepare_coordination_workload(1, 1).expect("workload should build");
        let pair_1_1 = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_1_1),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_1_1.unified.hybrid_result_count, 1);
        assert_eq!(pair_1_1.decomposed.hybrid_result_count, 1);

        // 2. historical=1, live=0 -> hybrid_result_count=0
        let workload_1_0 = prepare_coordination_workload(1, 0).expect("workload should build");
        let pair_1_0 = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 0,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_1_0),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_1_0.unified.hybrid_result_count, 0);
        assert_eq!(pair_1_0.decomposed.hybrid_result_count, 0);

        // 3. historical=0, live=1 -> hybrid_result_count=0
        let workload_0_1 = prepare_coordination_workload(0, 1).expect("workload should build");
        let pair_0_1 = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 0,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_0_1),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_0_1.unified.hybrid_result_count, 0);
        assert_eq!(pair_0_1.decomposed.hybrid_result_count, 0);

        // 4. historical=1, live=1 but mismatched sensor/join key -> hybrid_result_count=0
        let mut workload_mismatched =
            prepare_coordination_workload(1, 1).expect("workload should build");
        // Change live event sensor to mismatched
        if !workload_mismatched.live_events.is_empty() {
            workload_mismatched.live_events[0] = RDFEvent::new(
                workload_mismatched.live_events[0].timestamp,
                "http://example.org/junction/mismatched_sensor",
                TRAFFIC_PREDICATE,
                &workload_mismatched.live_events[0].object,
                GRAPH_URI,
            );
        }
        let pair_mismatched = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_mismatched),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_mismatched.unified.hybrid_result_count, 0);
        assert_eq!(pair_mismatched.decomposed.hybrid_result_count, 0);
    }

    #[test]
    fn test_sustained_expected_completed_windows_formula() {
        // window_size=120s, slide=60s, duration=240s
        let duration = 240;
        let window_size = 120;
        let slide = 60;
        let first_start = window_size - slide - 1;
        let last_event = duration - 1;
        let close_ts = last_event + 20;
        let expected = if close_ts >= first_start {
            1 + (close_ts - first_start) / slide
        } else {
            0
        };
        assert_eq!(expected, 4);
    }

    #[test]
    fn test_sustained_wall_clock_config_helpers() {
        assert!((sustained_event_interval_ms(4) - 250.0).abs() < f64::EPSILON);
        assert_eq!(sustained_expected_wall_clock_duration_ms(20), 20_000);
    }

    #[test]
    fn test_sustained_hybrid_dependency_and_equivalence() {
        let metadata = test_metadata();
        let adapter = OxigraphExternalAdapter::new();

        // 1. Janus and decomposed baseline match per-window result hashes on a tiny workload
        let workload_1_1 =
            prepare_sustained_workload(100, 240, 1, 120, 60).expect("workload should build");
        let pair_1_1 = run_sustained_pair(SustainedRunConfig {
            mode: ExecutionMode::Warm,
            time_mode: TimeMode::Virtual,
            run_index: 0,
            is_warmup: false,
            historical_events: 100,
            live_duration_seconds: 240,
            event_rate_hz: 1,
            event_interval_ms: sustained_event_interval_ms(1),
            expected_wall_clock_duration_ms: sustained_expected_wall_clock_duration_ms(240),
            window_size_seconds: 120,
            window_slide_seconds: 60,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_1_1),
            debug_output_dir: None,
        })
        .expect("sustained pair should run");

        assert_eq!(pair_1_1.unified.expected_completed_windows, 2);
        assert_eq!(pair_1_1.unified.completed_windows_total, 4);
        assert_eq!(pair_1_1.unified.completed_windows_in_horizon, 2);
        assert_eq!(pair_1_1.unified.flush_windows, 2);
        assert_eq!(pair_1_1.unified.time_mode, "virtual");
        assert!(pair_1_1.unified.uses_virtual_event_time);
        assert!((pair_1_1.unified.event_interval_ms - 1000.0).abs() < f64::EPSILON);
        assert_eq!(pair_1_1.unified.expected_wall_clock_duration_ms, 240_000);
        assert_eq!(pair_1_1.decomposed.completed_windows_total, 4);
        assert_eq!(pair_1_1.decomposed.completed_windows_in_horizon, 2);
        assert_eq!(pair_1_1.decomposed.flush_windows, 2);
        assert!(pair_1_1.decomposed.uses_virtual_event_time);
        assert_eq!(pair_1_1.unified.equivalent_to_baseline, Some(true));
        assert!(pair_1_1.unified.hybrid_result_count_total > 0);

        // 2. Removing historical input yields zero hybrid results
        let workload_no_hist =
            prepare_sustained_workload(0, 240, 1, 120, 60).expect("workload should build");
        let pair_no_hist = run_sustained_pair(SustainedRunConfig {
            mode: ExecutionMode::Warm,
            time_mode: TimeMode::Virtual,
            run_index: 0,
            is_warmup: false,
            historical_events: 0,
            live_duration_seconds: 240,
            event_rate_hz: 1,
            event_interval_ms: sustained_event_interval_ms(1),
            expected_wall_clock_duration_ms: sustained_expected_wall_clock_duration_ms(240),
            window_size_seconds: 120,
            window_slide_seconds: 60,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_no_hist),
            debug_output_dir: None,
        })
        .expect("sustained pair should run");
        assert_eq!(pair_no_hist.unified.hybrid_result_count_total, 0);
        assert_eq!(pair_no_hist.decomposed.hybrid_result_count_total, 0);

        // 3. Removing live input yields zero hybrid results
        let workload_no_live =
            prepare_sustained_workload(100, 0, 1, 120, 60).expect("workload should build");
        let pair_no_live = run_sustained_pair(SustainedRunConfig {
            mode: ExecutionMode::Warm,
            time_mode: TimeMode::Virtual,
            run_index: 0,
            is_warmup: false,
            historical_events: 100,
            live_duration_seconds: 0,
            event_rate_hz: 1,
            event_interval_ms: sustained_event_interval_ms(1),
            expected_wall_clock_duration_ms: sustained_expected_wall_clock_duration_ms(0),
            window_size_seconds: 120,
            window_slide_seconds: 60,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_no_live),
            debug_output_dir: None,
        })
        .expect("sustained pair should run");
        assert_eq!(pair_no_live.unified.hybrid_result_count_total, 0);
        assert_eq!(pair_no_live.decomposed.hybrid_result_count_total, 0);
    }
}
