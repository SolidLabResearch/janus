use crate::{
    core::RDFEvent,
    execution::{HistoricalExecutor, ResultConverter},
    paper_bench::harness::{collect_repro_metadata, ensure_output_dir},
    parsing::janusql_parser::{
        BaselineDefinition, BaselineGraphTemplate, GraphTermTemplate, JanusQLParser,
        ParsedJanusQuery, SourceKind, TripleTemplate, WindowDefinition, WindowType,
    },
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
    stream::live_stream_processing::LiveStreamProcessing,
};
use clap::ValueEnum;
use oxigraph::model::{BlankNode, GraphName, NamedNode, NamedOrBlankNode, Quad, Term};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const PREFIX: &str = "http://example.org/";
const STREAM_URI: &str = "http://example.org/stream";
const BASELINE_GRAPH: &str = "http://example.org/dayBaseline";
const BASELINE_QUERY_NAME: &str = "http://example.org/dayBaseline";
const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum QueryDefinedBaselineProfile {
    Smoke,
}

impl QueryDefinedBaselineProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LiveReplayMode {
    Accelerated,
    Realtime,
}

impl LiveReplayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accelerated => "accelerated",
            Self::Realtime => "realtime",
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueryDefinedBaselineBenchmarkConfig {
    pub profile: QueryDefinedBaselineProfile,
    pub runs: usize,
    pub warmup_runs: usize,
    pub historical_events: Vec<usize>,
    pub baseline_entities: Vec<usize>,
    pub live_replay_mode: LiveReplayMode,
    pub live_rate_hz: f64,
    pub live_duration_seconds: Option<u64>,
    pub live_window_size_seconds: Option<u64>,
    pub live_window_slide_seconds: Option<u64>,
    pub output_dir: Option<PathBuf>,
    pub debug_lowered_query: bool,
    pub verbose: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineObservedRow {
    pub sensor: String,
    pub minute_avg_value: f64,
    pub day_avg_value: Option<f64>,
    pub difference: Option<f64>,
    pub received_after_first_event_ms: f64,
    pub timestamp_from: i64,
    pub timestamp_to: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineCorrectnessDiagnostics {
    pub variant: String,
    pub expected_result_count: usize,
    pub observed_result_count: usize,
    pub expected_emitted_windows: Option<usize>,
    pub observed_emitted_windows: usize,
    pub expected_variables: Vec<String>,
    pub observed_variables: Vec<String>,
    pub first_observed_rows: Vec<QueryDefinedBaselineObservedRow>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineVariantMetrics {
    pub variant: String,
    pub run_index: usize,
    pub historical_events: usize,
    pub baseline_entities: usize,
    pub live_replay_mode: String,
    pub live_rate_hz: f64,
    pub live_duration_seconds: Option<u64>,
    pub live_window_size_seconds: Option<u64>,
    pub live_window_slide_seconds: Option<u64>,
    pub live_event_count: usize,
    pub expected_emitted_windows: usize,
    pub expected_full_windows: usize,
    pub warmup_window_count: Option<usize>,
    pub observed_emitted_windows: usize,
    pub window_semantics_note: Option<String>,
    pub historical_generation_ms: Option<f64>,
    pub storage_write_ms: Option<f64>,
    pub baseline_eval_ms: Option<f64>,
    pub materialization_ms: Option<f64>,
    pub static_injection_ms: Option<f64>,
    pub historical_query_ms: Option<f64>,
    pub baseline_materialization_ms: Option<f64>,
    pub static_graph_injection_ms: Option<f64>,
    pub live_startup_ms: f64,
    pub first_result_latency_ms: f64,
    pub peak_rss_mb: Option<f64>,
    pub mean_rss_mb: Option<f64>,
    pub peak_cpu_percent: Option<f64>,
    pub mean_cpu_percent: Option<f64>,
    pub sample_count: usize,
    pub result_count: usize,
    pub correctness_ok: bool,
    pub correctness_diagnostics: Option<QueryDefinedBaselineCorrectnessDiagnostics>,
    pub materialized_quad_count: Option<usize>,
    pub baseline_binding_count: Option<usize>,
    pub window_result_latencies_ms: Vec<f64>,
    pub completed_window_latencies_ms: Vec<f64>,
    pub completed_window_result_counts: Vec<usize>,
    pub observed_rows: Vec<QueryDefinedBaselineObservedRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineComparisonRow {
    pub historical_events: usize,
    pub baseline_entities: usize,
    pub is_warmup: bool,
    pub run_index: usize,
    pub observed_baseline_rows: usize,
    pub observed_live_only_rows: usize,
    pub baseline: QueryDefinedBaselineVariantMetrics,
    pub live_only: QueryDefinedBaselineVariantMetrics,
    pub live_startup_overhead_ms: f64,
    pub first_result_overhead_ms: f64,
    pub peak_rss_mb: Option<f64>,
    pub mean_rss_mb: Option<f64>,
    pub peak_cpu_percent: Option<f64>,
    pub mean_cpu_percent: Option<f64>,
    pub sample_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineBenchmarkReport {
    pub metadata: crate::paper_bench::harness::ReproMetadata,
    pub profile: String,
    pub live_replay_mode: String,
    pub live_rate_hz: f64,
    pub live_duration_seconds: Option<u64>,
    pub live_window_size_seconds: Option<u64>,
    pub live_window_slide_seconds: Option<u64>,
    pub runs: usize,
    pub warmup_runs: usize,
    pub historical_events: Vec<usize>,
    pub baseline_entities: Vec<usize>,
    pub correctness_passed: bool,
    pub output_dir: PathBuf,
    pub comparisons: Vec<QueryDefinedBaselineComparisonRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineMatrixSummaryRow {
    pub profile: String,
    pub historical_events: usize,
    pub baseline_entities: usize,
    pub runs: usize,
    pub warmup_runs: usize,
    pub correctness_rate: f64,
    pub expected_emitted_windows: Option<f64>,
    pub expected_full_windows: Option<f64>,
    pub warmup_window_count: Option<f64>,
    pub observed_emitted_windows_mean: f64,
    pub observed_emitted_windows_std: f64,
    pub observed_baseline_rows_mean: f64,
    pub observed_baseline_rows_std: f64,
    pub observed_live_only_rows_mean: f64,
    pub observed_live_only_rows_std: f64,
    pub historical_generation_ms_mean: Option<f64>,
    pub historical_generation_ms_std: Option<f64>,
    pub storage_write_ms_mean: Option<f64>,
    pub storage_write_ms_std: Option<f64>,
    pub peak_rss_mb_mean: Option<f64>,
    pub peak_rss_mb_std: Option<f64>,
    pub mean_rss_mb_mean: Option<f64>,
    pub mean_rss_mb_std: Option<f64>,
    pub peak_cpu_percent_mean: Option<f64>,
    pub peak_cpu_percent_std: Option<f64>,
    pub mean_cpu_percent_mean: Option<f64>,
    pub mean_cpu_percent_std: Option<f64>,
    pub baseline_eval_ms_mean: Option<f64>,
    pub baseline_eval_ms_std: Option<f64>,
    pub materialization_ms_mean: Option<f64>,
    pub materialization_ms_std: Option<f64>,
    pub static_injection_ms_mean: Option<f64>,
    pub static_injection_ms_std: Option<f64>,
    pub baseline_first_result_ms_mean: f64,
    pub baseline_first_result_ms_std: f64,
    pub live_only_first_result_ms_mean: f64,
    pub live_only_first_result_ms_std: f64,
    pub startup_overhead_ms_mean: f64,
    pub startup_overhead_ms_std: f64,
    pub first_result_overhead_ms_mean: f64,
    pub first_result_overhead_ms_std: f64,
    pub baseline_binding_count_mean: Option<f64>,
    pub baseline_binding_count_std: Option<f64>,
    pub materialized_quad_count_mean: Option<f64>,
    pub materialized_quad_count_std: Option<f64>,
    pub baseline_result_count_mean: f64,
    pub baseline_result_count_std: f64,
    pub live_only_result_count_mean: f64,
    pub live_only_result_count_std: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryDefinedBaselineBenchmarkCsvRows {
    pub matrix_summaries: Vec<QueryDefinedBaselineMatrixSummaryRow>,
}

struct PreparedStorage {
    storage: Arc<StreamingSegmentedStorage>,
    historical_min_timestamp: u64,
    historical_max_timestamp: u64,
    historical_generation_ms: f64,
    storage_write_ms: f64,
    live_events: Vec<RDFEvent>,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedLiveReplayConfig {
    mode: LiveReplayMode,
    rate_hz: f64,
    live_duration_seconds: Option<u64>,
    live_window_size_seconds: Option<u64>,
    live_window_slide_seconds: Option<u64>,
    live_event_count: usize,
    event_interval_ms: f64,
    expected_emitted_windows: usize,
    expected_full_windows: usize,
    warmup_window_count: Option<usize>,
}

struct HistoricalWriteStats {
    min_timestamp: u64,
    max_timestamp: u64,
    generation_ms: f64,
    storage_write_ms: f64,
}

#[derive(Clone, Debug)]
struct ResourceSample {
    rss_mb: f64,
    cpu_percent: f64,
}

#[derive(Clone, Debug, Default)]
struct ResourceSummary {
    peak_rss_mb: Option<f64>,
    mean_rss_mb: Option<f64>,
    peak_cpu_percent: Option<f64>,
    mean_cpu_percent: Option<f64>,
    sample_count: usize,
}

struct ResourceSampler {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<ResourceSample>>>,
    handle: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
struct VariantRunData {
    metrics: QueryDefinedBaselineVariantMetrics,
}

#[derive(Debug)]
struct TimedBinding {
    result: rsp_rs::BindingWithTimestamp,
    received_after_first_event_ms: f64,
}

#[derive(Debug)]
struct ObservedWindowSummary {
    result_count: usize,
    first_result_latency_ms: f64,
}

pub struct QueryDefinedBaselineBenchmarkOutcome {
    pub report_path: PathBuf,
    pub summary_csv_path: PathBuf,
    pub summary_md_path: PathBuf,
    pub report: QueryDefinedBaselineBenchmarkReport,
    pub csv_rows: QueryDefinedBaselineBenchmarkCsvRows,
}

impl ResourceSampler {
    fn start(interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_samples = Arc::clone(&samples);
        let handle = thread::spawn(move || {
            let pid: Pid = (std::process::id() as usize).into();
            let mut system = System::new_all();
            let refresh = |system: &mut System| {
                system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    ProcessRefreshKind::new().with_memory().with_cpu(),
                );
            };

            refresh(&mut system);
            let mut next_tick = Instant::now();

            loop {
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }

                next_tick += interval;
                if let Some(remaining) = next_tick.checked_duration_since(Instant::now()) {
                    thread::sleep(remaining);
                }

                if thread_stop.load(Ordering::Relaxed) {
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
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        let samples = self.samples.lock().expect("resource samples mutex poisoned");
        summarize_resource_samples(samples.as_slice())
    }
}

pub fn run_query_defined_baseline_benchmark(
    config: QueryDefinedBaselineBenchmarkConfig,
) -> Result<QueryDefinedBaselineBenchmarkOutcome, Box<dyn std::error::Error>> {
    let output_dir = config.output_dir.clone().unwrap_or_else(default_output_dir);
    ensure_output_dir(&output_dir)?;
    let metadata = collect_repro_metadata();
    let parser = JanusQLParser::new()?;
    let live_replay = resolve_live_replay_config(&config)?;
    let mut comparisons = Vec::new();

    for &historical_events in &config.historical_events {
        for &baseline_entities in &config.baseline_entities {
            let prepared = prepare_storage(
                config.profile,
                historical_events,
                baseline_entities,
                config.verbose,
            )?;

            for run_index in 0..config.warmup_runs {
                let comparison = run_single_comparison(
                    &parser,
                    &prepared,
                    &live_replay,
                    config.profile,
                    historical_events,
                    baseline_entities,
                    run_index,
                    true,
                    config.debug_lowered_query,
                    config.verbose,
                )?;
                comparisons.push(comparison);
            }

            for run_index in 0..config.runs {
                let comparison = run_single_comparison(
                    &parser,
                    &prepared,
                    &live_replay,
                    config.profile,
                    historical_events,
                    baseline_entities,
                    run_index,
                    false,
                    config.debug_lowered_query,
                    config.verbose,
                )?;
                comparisons.push(comparison);
            }
        }
    }

    let report = QueryDefinedBaselineBenchmarkReport {
        metadata,
        profile: config.profile.as_str().to_string(),
        live_replay_mode: live_replay.mode.as_str().to_string(),
        live_rate_hz: live_replay.rate_hz,
        live_duration_seconds: live_replay.live_duration_seconds,
        live_window_size_seconds: live_replay.live_window_size_seconds,
        live_window_slide_seconds: live_replay.live_window_slide_seconds,
        runs: config.runs,
        warmup_runs: config.warmup_runs,
        historical_events: config.historical_events.clone(),
        baseline_entities: config.baseline_entities.clone(),
        correctness_passed: comparisons.iter().all(|comparison| {
            comparison.baseline.correctness_ok && comparison.live_only.correctness_ok
        }),
        output_dir: output_dir.clone(),
        comparisons: comparisons.clone(),
    };

    let report_path = output_dir.join("query_defined_baseline.raw.json");
    let summary_csv_path = output_dir.join("query_defined_baseline.summary.csv");
    let summary_md_path = output_dir.join("query_defined_baseline_results.md");

    write_report_json(&report_path, &report)?;
    let csv_rows = summarize_comparisons(&report);
    write_summary_csv(&summary_csv_path, &csv_rows)?;
    write_summary_markdown(&summary_md_path, &csv_rows)?;

    Ok(QueryDefinedBaselineBenchmarkOutcome {
        report_path,
        summary_csv_path,
        summary_md_path,
        report,
        csv_rows,
    })
}

fn resolve_live_replay_config(
    config: &QueryDefinedBaselineBenchmarkConfig,
) -> Result<ResolvedLiveReplayConfig, Box<dyn std::error::Error>> {
    if !config.live_rate_hz.is_finite() || config.live_rate_hz <= 0.0 {
        return Err("--live-rate-hz must be greater than 0".into());
    }

    let resolved = match config.live_replay_mode {
        LiveReplayMode::Accelerated => ResolvedLiveReplayConfig {
            mode: LiveReplayMode::Accelerated,
            rate_hz: config.live_rate_hz,
            live_duration_seconds: None,
            live_window_size_seconds: None,
            live_window_slide_seconds: None,
            live_event_count: 0,
            event_interval_ms: 0.0,
            expected_emitted_windows: 0,
            expected_full_windows: 0,
            warmup_window_count: None,
        },
        LiveReplayMode::Realtime => {
            let live_duration_seconds = config.live_duration_seconds.unwrap_or(240);
            let live_window_size_seconds = config.live_window_size_seconds.unwrap_or(120);
            let live_window_slide_seconds = config.live_window_slide_seconds.unwrap_or(60);

            if live_window_size_seconds == 0 {
                return Err("--live-window-size-seconds must be greater than 0".into());
            }
            if live_window_slide_seconds == 0 {
                return Err("--live-window-slide-seconds must be greater than 0".into());
            }

            let live_event_count =
                ((live_duration_seconds as f64) * config.live_rate_hz).round() as usize;
            let event_interval_ms = 1000.0 / config.live_rate_hz;
            let expected_emitted_windows =
                expected_emitted_windows(live_duration_seconds, live_window_slide_seconds);
            let expected_full_windows = expected_full_windows(
                live_duration_seconds,
                live_window_size_seconds,
                live_window_slide_seconds,
            );

            ResolvedLiveReplayConfig {
                mode: LiveReplayMode::Realtime,
                rate_hz: config.live_rate_hz,
                live_duration_seconds: Some(live_duration_seconds),
                live_window_size_seconds: Some(live_window_size_seconds),
                live_window_slide_seconds: Some(live_window_slide_seconds),
                live_event_count,
                event_interval_ms,
                expected_emitted_windows,
                expected_full_windows,
                warmup_window_count: Some(
                    expected_emitted_windows.saturating_sub(expected_full_windows),
                ),
            }
        }
    };

    Ok(resolved)
}

fn expected_emitted_windows(live_duration_seconds: u64, window_slide_seconds: u64) -> usize {
    if window_slide_seconds == 0 {
        return 0;
    }

    (live_duration_seconds / window_slide_seconds) as usize
}

fn expected_full_windows(
    live_duration_seconds: u64,
    window_size_seconds: u64,
    window_slide_seconds: u64,
) -> usize {
    if live_duration_seconds < window_size_seconds {
        return 0;
    }

    1 + ((live_duration_seconds - window_size_seconds) / window_slide_seconds) as usize
}

fn default_output_dir() -> PathBuf {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    PathBuf::from(format!("logs/benchmark/query_defined_baseline/{ts}"))
}

fn prepare_storage(
    profile: QueryDefinedBaselineProfile,
    historical_events_count: usize,
    baseline_entities_count: usize,
    verbose: bool,
) -> Result<PreparedStorage, Box<dyn std::error::Error>> {
    if historical_events_count == 0 {
        return Err("historical_events must be at least 1".into());
    }
    if baseline_entities_count == 0 {
        return Err("baseline_entities must be at least 1".into());
    }
    if historical_events_count < baseline_entities_count {
        return Err("historical_events must be greater than or equal to baseline_entities".into());
    }

    let storage = StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: format!(
            "/tmp/janus_query_defined_baseline_{}_{}_{}",
            profile.as_str(),
            historical_events_count,
            baseline_entities_count
        ),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    })?;

    let historical_stats = write_historical_events(
        &storage,
        historical_events_count,
        baseline_entities_count,
        verbose,
    )?;
    let historical_generation_ms = historical_stats.generation_ms;
    let storage_write_ms = historical_stats.storage_write_ms;

    let live_events = generate_accelerated_live_events(baseline_entities_count);

    Ok(PreparedStorage {
        storage: Arc::new(storage),
        historical_min_timestamp: historical_stats.min_timestamp,
        historical_max_timestamp: historical_stats.max_timestamp,
        historical_generation_ms,
        storage_write_ms,
        live_events,
    })
}

fn write_historical_events(
    storage: &StreamingSegmentedStorage,
    historical_events_count: usize,
    baseline_entities_count: usize,
    verbose: bool,
) -> Result<HistoricalWriteStats, Box<dyn std::error::Error>> {
    let mut min_timestamp = 0u64;
    let mut max_timestamp = 0u64;
    let mut initialized = false;
    let mut timestamp = 10u64;
    let mut generation_ms = 0.0;
    let mut write_ms = 0.0;

    for event_idx in 0..historical_events_count {
        if verbose && event_idx > 0 && event_idx % 1_000_000 == 0 {
            eprintln!(
                "[query_defined_baseline] historical_events_written={event_idx}/{historical_events_count}"
            );
        }

        let event_started = Instant::now();
        let sensor_idx = event_idx % baseline_entities_count;
        let value = 10 + sensor_idx as i64;
        let event = RDFEvent::new(
            timestamp,
            &sensor_iri(sensor_idx),
            &format!("{PREFIX}hasValue"),
            &value.to_string(),
            &format!("{PREFIX}stream"),
        );
        generation_ms += event_started.elapsed().as_secs_f64() * 1_000.0;

        let write_started = Instant::now();
        storage.write_rdf_event(event)?;
        write_ms += write_started.elapsed().as_secs_f64() * 1_000.0;

        if !initialized {
            min_timestamp = timestamp;
            max_timestamp = timestamp;
            initialized = true;
        } else {
            min_timestamp = min_timestamp.min(timestamp);
            max_timestamp = max_timestamp.max(timestamp);
        }
        timestamp += 10;
    }

    if !initialized {
        return Err("missing historical benchmark events".into());
    }

    Ok(HistoricalWriteStats {
        min_timestamp,
        max_timestamp,
        generation_ms,
        storage_write_ms: write_ms,
    })
}

fn generate_accelerated_live_events(baseline_entities_count: usize) -> Vec<RDFEvent> {
    let mut events = Vec::with_capacity(baseline_entities_count);
    for sensor_idx in 0..baseline_entities_count {
        let timestamp = 1000 + (sensor_idx as u64 * 10);
        let value = 20 + sensor_idx as i64 * 10;
        events.push(RDFEvent::new(
            timestamp,
            &sensor_iri(sensor_idx),
            &format!("{PREFIX}hasValue"),
            &value.to_string(),
            &format!("{PREFIX}stream"),
        ));
    }
    events
}

fn generate_realtime_live_events(
    baseline_entities_count: usize,
    live_event_count: usize,
    event_interval_ms: f64,
) -> Vec<RDFEvent> {
    let mut events = Vec::with_capacity(live_event_count);
    let start_timestamp = 1_900_000_000_000u64;
    for event_idx in 0..live_event_count {
        let sensor_idx = event_idx % baseline_entities_count;
        let timestamp = start_timestamp + ((event_idx as f64) * event_interval_ms).round() as u64;
        let value = 20 + sensor_idx as i64 * 10;
        events.push(RDFEvent::new(
            timestamp,
            &sensor_iri(sensor_idx),
            &format!("{PREFIX}hasValue"),
            &value.to_string(),
            &format!("{PREFIX}stream"),
        ));
    }
    events
}

fn build_live_events_for_replay(
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    baseline_entities_count: usize,
) -> Vec<RDFEvent> {
    match live_replay.mode {
        LiveReplayMode::Accelerated => prepared.live_events.clone(),
        LiveReplayMode::Realtime => generate_realtime_live_events(
            baseline_entities_count,
            live_replay.live_event_count,
            live_replay.event_interval_ms,
        ),
    }
}

fn realtime_close_timestamp(
    live_events: &[RDFEvent],
    live_replay: &ResolvedLiveReplayConfig,
) -> Result<i64, Box<dyn std::error::Error>> {
    let last_timestamp = live_events.last().ok_or("missing live benchmark events")?.timestamp;
    let window_size_ms = live_replay
        .live_window_size_seconds
        .ok_or("missing realtime window size")?
        .saturating_mul(1000);
    let event_interval_ms = live_replay.event_interval_ms.ceil() as u64;
    let close_timestamp = last_timestamp
        .saturating_add(window_size_ms)
        .saturating_add(event_interval_ms)
        .saturating_add(1);
    Ok(i64::try_from(close_timestamp)?)
}

fn query_defined_baseline_query(
    live_replay: LiveReplayMode,
    window_size_ms: u64,
    window_slide_ms: u64,
) -> String {
    let window_clause = match live_replay {
        LiveReplayMode::Accelerated => "FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]".to_string(),
        LiveReplayMode::Realtime => format!(
            "FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE {window_size_ms} STEP {window_slide_ms}]"
        ),
    };
    format!(
        r#"
PREFIX ex: <{prefix}>

{window_clause}
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {{
  ?sensor ex:hasValue ?value .
}}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
       ((AVG(?value) - ?dayAvgValue) AS ?difference)
WHERE {{
  WINDOW ex:liveMinute {{
    ?sensor ex:hasValue ?value .
  }}
  GRAPH ex:dayBaseline {{
    ?sensor ex:dayAvgValue ?dayAvgValue .
  }}
}}
GROUP BY ?sensor ?dayAvgValue
HAVING(AVG(?value) > ?dayAvgValue)
"#,
        prefix = PREFIX,
        window_clause = window_clause
    )
}

fn smoke_historical_window(
    min_timestamp: u64,
    max_timestamp: u64,
) -> Result<WindowDefinition, Box<dyn std::error::Error>> {
    Ok(WindowDefinition {
        window_name: format!("{PREFIX}historyDay"),
        source_kind: SourceKind::Log,
        stream_name: format!("{PREFIX}stream"),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(min_timestamp),
        end: Some(max_timestamp),
        window_type: WindowType::HistoricalFixed,
    })
}

fn live_only_query(
    live_replay: LiveReplayMode,
    window_size_ms: u64,
    window_slide_ms: u64,
) -> String {
    let window_clause = match live_replay {
        LiveReplayMode::Accelerated => "FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60 STEP 5]".to_string(),
        LiveReplayMode::Realtime => format!(
            "FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE {window_size_ms} STEP {window_slide_ms}]"
        ),
    };
    format!(
        r#"
PREFIX : <{prefix}>

{window_clause}

REGISTER RStream :output AS
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
WHERE {{
  WINDOW :liveMinute {{
    ?sensor :hasValue ?value .
  }}
}}
GROUP BY ?sensor
"#,
        prefix = PREFIX,
        window_clause = window_clause
    )
}

fn run_single_comparison(
    parser: &JanusQLParser,
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    profile: QueryDefinedBaselineProfile,
    historical_events: usize,
    baseline_entities: usize,
    run_index: usize,
    is_warmup: bool,
    debug_lowered_query: bool,
    verbose: bool,
) -> Result<QueryDefinedBaselineComparisonRow, Box<dyn std::error::Error>> {
    let _ = profile;
    let sampler = ResourceSampler::start(RESOURCE_SAMPLE_INTERVAL);
    let comparison_result =
        (|| -> Result<QueryDefinedBaselineComparisonRow, Box<dyn std::error::Error>> {
            let baseline = run_baseline_variant(
                parser,
                prepared,
                live_replay,
                historical_events,
                baseline_entities,
                run_index,
                is_warmup,
                debug_lowered_query,
                verbose,
            )?;
            let live_only = run_live_only_variant(
                prepared,
                live_replay,
                historical_events,
                baseline_entities,
                run_index,
                is_warmup,
                debug_lowered_query,
                verbose,
            )?;

            Ok(QueryDefinedBaselineComparisonRow {
                historical_events,
                baseline_entities,
                is_warmup,
                run_index,
                observed_baseline_rows: baseline.metrics.result_count,
                observed_live_only_rows: live_only.metrics.result_count,
                live_startup_overhead_ms: (baseline.metrics.live_startup_ms
                    - live_only.metrics.live_startup_ms)
                    .max(0.0),
                first_result_overhead_ms: (baseline.metrics.first_result_latency_ms
                    - live_only.metrics.first_result_latency_ms)
                    .max(0.0),
                baseline: baseline.metrics,
                live_only: live_only.metrics,
                peak_rss_mb: None,
                mean_rss_mb: None,
                peak_cpu_percent: None,
                mean_cpu_percent: None,
                sample_count: 0,
            })
        })();
    let resource_usage = sampler.finish();
    let mut comparison = comparison_result?;
    comparison.peak_rss_mb = resource_usage.peak_rss_mb;
    comparison.mean_rss_mb = resource_usage.mean_rss_mb;
    comparison.peak_cpu_percent = resource_usage.peak_cpu_percent;
    comparison.mean_cpu_percent = resource_usage.mean_cpu_percent;
    comparison.sample_count = resource_usage.sample_count;
    Ok(comparison)
}

fn run_baseline_variant(
    parser: &JanusQLParser,
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    historical_events: usize,
    baseline_entities: usize,
    run_index: usize,
    is_warmup: bool,
    debug_lowered_query: bool,
    verbose: bool,
) -> Result<VariantRunData, Box<dyn std::error::Error>> {
    let window_size_ms = live_replay.live_window_size_seconds.unwrap_or(60).saturating_mul(1000);
    let window_slide_ms = live_replay.live_window_slide_seconds.unwrap_or(5).saturating_mul(1000);
    let query = query_defined_baseline_query(live_replay.mode, window_size_ms, window_slide_ms);
    log_stage(verbose, "baseline", "parse_query");
    let parsed = parser.parse(&query)?;
    log_stage(verbose, "baseline", "execute_historical_query");
    let historical_executor =
        HistoricalExecutor::new(Arc::clone(&prepared.storage), OxigraphAdapter::new());
    let historical_window = smoke_historical_window(
        prepared.historical_min_timestamp,
        prepared.historical_max_timestamp,
    )?;

    let historical_started = Instant::now();
    let bindings = historical_executor.execute_fixed_window(
        &historical_window,
        parsed
            .generated_baseline_queries
            .first()
            .ok_or("missing generated baseline query")?
            .sparql_query
            .as_str(),
    )?;
    let historical_query_ms = historical_started.elapsed().as_secs_f64() * 1_000.0;

    log_stage(verbose, "baseline", "materialize_quads");
    let materialize_started = Instant::now();
    let quads = materialize_query_defined_baseline_quads(&parsed, &bindings)?;
    let baseline_materialization_ms = materialize_started.elapsed().as_secs_f64() * 1_000.0;

    if debug_lowered_query {
        log_lowered_live_query("query_defined_baseline", &parsed.rspql_query);
    }
    log_stage(verbose, "baseline", "create_live_processor");
    let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    processor.register_stream(STREAM_URI)?;
    log_stage(verbose, "baseline", "inject_static_quads");
    let injection_started = Instant::now();
    for quad in &quads {
        processor.add_static_quad(quad.clone());
    }
    let static_graph_injection_ms = injection_started.elapsed().as_secs_f64() * 1_000.0;

    log_stage(verbose, "baseline", "start_live_processing");
    let startup_started = Instant::now();
    processor.start_processing()?;
    let live_startup_ms = startup_started.elapsed().as_secs_f64() * 1_000.0;

    let live_events = build_live_events_for_replay(prepared, live_replay, baseline_entities);
    let expected_live_averages = expected_live_averages(&live_events);
    let expected_day_averages = expected_day_averages(&bindings);
    let live_event_start = Instant::now();
    let mut collected = Vec::new();
    match live_replay.mode {
        LiveReplayMode::Accelerated => {
            log_stage(verbose, "baseline", "send_live_events");
            let first_event = live_events.first().ok_or("missing live benchmark events")?.clone();
            processor.add_event(STREAM_URI, first_event)?;
            for event in live_events.iter().skip(1) {
                processor.add_event(STREAM_URI, event.clone())?;
            }
            log_stage(verbose, "baseline", "close_stream");
            processor.close_stream(STREAM_URI, 10_000)?;
            std::thread::sleep(Duration::from_millis(25));
            log_stage(verbose, "baseline", "collect_results");
            collected = collect_live_results(&processor, live_event_start, baseline_entities)?;
        }
        LiveReplayMode::Realtime => {
            log_stage(verbose, "baseline", "send_live_events");
            for (event_index, event) in live_events.iter().enumerate() {
                wait_for_live_event_schedule(live_event_start, event_index, live_replay.rate_hz);
                processor.add_event(STREAM_URI, event.clone())?;
                drain_available_live_results(&processor, live_event_start, &mut collected)?;
            }
            log_stage(verbose, "baseline", "close_stream");
            let close_timestamp = realtime_close_timestamp(&live_events, live_replay)?;
            processor.close_stream(STREAM_URI, close_timestamp)?;
            log_stage(verbose, "baseline", "collect_results");
            collect_realtime_live_results(&processor, live_event_start, &mut collected)?;
        }
    }
    let observed_rows = parse_live_rows(&collected)?;
    log_stage(verbose, "baseline", "done");
    let mut window_semantics_note = if live_replay.mode == LiveReplayMode::Realtime {
        Some(format!(
            "Realtime replay reports emitted windows separately from full windows; the first emission is the initial warm-up window and full windows follow at the logical duration horizon."
        ))
    } else {
        None
    };
    let window_summaries = summarize_observed_windows(&observed_rows);
    let first_result_latency_ms = observed_rows
        .first()
        .map(|row| row.received_after_first_event_ms)
        .unwrap_or(0.0);
    let window_result_latencies_ms = observed_rows
        .iter()
        .map(|row| row.received_after_first_event_ms)
        .collect::<Vec<_>>();
    let completed_window_latencies_ms = window_summaries
        .iter()
        .map(|window| window.first_result_latency_ms)
        .collect::<Vec<_>>();
    let completed_window_result_counts =
        window_summaries.iter().map(|window| window.result_count).collect::<Vec<_>>();
    let observed_emitted_windows = window_summaries.len();
    let live_event_count = live_events.len();
    if live_replay.mode == LiveReplayMode::Realtime
        && observed_emitted_windows != live_replay.expected_emitted_windows
    {
        let boundary_note = format!(
            "Observed {} emitted windows vs expected {}; the difference is typically caused by inclusive/exclusive window boundary behavior on a window that lands exactly on the replay horizon.",
            observed_emitted_windows, live_replay.expected_emitted_windows
        );
        window_semantics_note = Some(match window_semantics_note {
            Some(existing) => format!("{existing} {boundary_note}"),
            None => boundary_note,
        });
    }
    let correctness_result = validate_baseline_rows(
        &observed_rows,
        &expected_live_averages,
        &expected_day_averages,
        baseline_entities,
        live_replay,
        observed_emitted_windows,
        observed_rows.len(),
        live_event_count,
    );
    let correctness_ok = correctness_result.is_ok();
    let correctness_diagnostics = correctness_result.err().map(|reason| {
        build_correctness_diagnostics(
            "baseline",
            baseline_entities,
            &observed_rows,
            Some(live_replay.expected_emitted_windows),
            observed_emitted_windows,
            vec![
                "sensor".to_string(),
                "minuteAvgValue".to_string(),
                "dayAvgValue".to_string(),
                "difference".to_string(),
            ],
            reason,
        )
    });
    if verbose {
        if let Some(diagnostics) = &correctness_diagnostics {
            eprintln!("[baseline] correctness failed: {}", diagnostics.reason);
            eprintln!(
                "[baseline] expected_result_count={} observed_result_count={} expected_emitted_windows={:?} observed_emitted_windows={} expected_variables={:?} observed_variables={:?}",
                diagnostics.expected_result_count,
                diagnostics.observed_result_count,
                diagnostics.expected_emitted_windows,
                diagnostics.observed_emitted_windows,
                diagnostics.expected_variables,
                diagnostics.observed_variables
            );
            eprintln!("[baseline] first_observed_rows={:?}", diagnostics.first_observed_rows);
        }
    }
    let metrics = QueryDefinedBaselineVariantMetrics {
        variant: "baseline".to_string(),
        run_index,
        historical_events,
        baseline_entities,
        live_replay_mode: live_replay.mode.as_str().to_string(),
        live_rate_hz: live_replay.rate_hz,
        live_duration_seconds: live_replay.live_duration_seconds,
        live_window_size_seconds: live_replay.live_window_size_seconds,
        live_window_slide_seconds: live_replay.live_window_slide_seconds,
        live_event_count,
        expected_emitted_windows: live_replay.expected_emitted_windows,
        expected_full_windows: live_replay.expected_full_windows,
        warmup_window_count: live_replay.warmup_window_count,
        observed_emitted_windows,
        window_semantics_note,
        historical_generation_ms: Some(prepared.historical_generation_ms),
        storage_write_ms: Some(prepared.storage_write_ms),
        baseline_eval_ms: Some(historical_query_ms),
        materialization_ms: Some(baseline_materialization_ms),
        static_injection_ms: Some(static_graph_injection_ms),
        historical_query_ms: Some(historical_query_ms),
        baseline_materialization_ms: Some(baseline_materialization_ms),
        static_graph_injection_ms: Some(static_graph_injection_ms),
        live_startup_ms,
        first_result_latency_ms,
        peak_rss_mb: None,
        mean_rss_mb: None,
        peak_cpu_percent: None,
        mean_cpu_percent: None,
        sample_count: 0,
        result_count: observed_rows.len(),
        correctness_ok,
        correctness_diagnostics,
        materialized_quad_count: Some(quads.len()),
        baseline_binding_count: Some(bindings.len()),
        window_result_latencies_ms,
        completed_window_latencies_ms,
        completed_window_result_counts,
        observed_rows,
    };

    let _ = is_warmup;
    Ok(VariantRunData { metrics })
}

fn run_live_only_variant(
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    historical_events: usize,
    baseline_entities: usize,
    run_index: usize,
    is_warmup: bool,
    debug_lowered_query: bool,
    verbose: bool,
) -> Result<VariantRunData, Box<dyn std::error::Error>> {
    let _ = (run_index, is_warmup);
    let window_size_ms = live_replay.live_window_size_seconds.unwrap_or(60).saturating_mul(1000);
    let window_slide_ms = live_replay.live_window_slide_seconds.unwrap_or(5).saturating_mul(1000);
    let live_query = live_only_query(live_replay.mode, window_size_ms, window_slide_ms);
    log_stage(verbose, "live_only", "parse_query");
    if debug_lowered_query {
        log_lowered_live_query("live_only", &live_query);
    }
    log_stage(verbose, "live_only", "create_live_processor");
    let mut processor = LiveStreamProcessing::new(live_query)?;
    processor.register_stream(STREAM_URI)?;
    log_stage(verbose, "live_only", "start_live_processing");
    let startup_started = Instant::now();
    processor.start_processing()?;
    let live_startup_ms = startup_started.elapsed().as_secs_f64() * 1_000.0;

    let live_events = build_live_events_for_replay(prepared, live_replay, baseline_entities);
    let expected_live_averages = expected_live_averages(&live_events);
    let live_event_start = Instant::now();
    let mut collected = Vec::new();
    match live_replay.mode {
        LiveReplayMode::Accelerated => {
            log_stage(verbose, "live_only", "send_live_events");
            let first_event = live_events.first().ok_or("missing live benchmark events")?.clone();
            processor.add_event(STREAM_URI, first_event)?;
            for event in live_events.iter().skip(1) {
                processor.add_event(STREAM_URI, event.clone())?;
            }
            log_stage(verbose, "live_only", "close_stream");
            processor.close_stream(STREAM_URI, 10_000)?;
            std::thread::sleep(Duration::from_millis(25));
            log_stage(verbose, "live_only", "collect_results");
            collected = collect_live_results(&processor, live_event_start, baseline_entities)?;
        }
        LiveReplayMode::Realtime => {
            log_stage(verbose, "live_only", "send_live_events");
            for (event_index, event) in live_events.iter().enumerate() {
                wait_for_live_event_schedule(live_event_start, event_index, live_replay.rate_hz);
                processor.add_event(STREAM_URI, event.clone())?;
                drain_available_live_results(&processor, live_event_start, &mut collected)?;
            }
            log_stage(verbose, "live_only", "close_stream");
            let close_timestamp = realtime_close_timestamp(&live_events, live_replay)?;
            processor.close_stream(STREAM_URI, close_timestamp)?;
            log_stage(verbose, "live_only", "collect_results");
            collect_realtime_live_results(&processor, live_event_start, &mut collected)?;
        }
    }
    let observed_rows = parse_live_rows(&collected)?;
    log_stage(verbose, "live_only", "done");
    let mut window_semantics_note = if live_replay.mode == LiveReplayMode::Realtime {
        Some(format!(
            "Realtime replay reports emitted windows separately from full windows; the first emission is the initial warm-up window and full windows follow at the logical duration horizon."
        ))
    } else {
        None
    };
    let window_summaries = summarize_observed_windows(&observed_rows);
    let first_result_latency_ms = observed_rows
        .first()
        .map(|row| row.received_after_first_event_ms)
        .unwrap_or(0.0);
    let window_result_latencies_ms = observed_rows
        .iter()
        .map(|row| row.received_after_first_event_ms)
        .collect::<Vec<_>>();
    let completed_window_latencies_ms = window_summaries
        .iter()
        .map(|window| window.first_result_latency_ms)
        .collect::<Vec<_>>();
    let completed_window_result_counts =
        window_summaries.iter().map(|window| window.result_count).collect::<Vec<_>>();
    let observed_emitted_windows = window_summaries.len();
    let live_event_count = live_events.len();
    if live_replay.mode == LiveReplayMode::Realtime
        && observed_emitted_windows != live_replay.expected_emitted_windows
    {
        let boundary_note = format!(
            "Observed {} emitted windows vs expected {}; the difference is typically caused by inclusive/exclusive window boundary behavior on a window that lands exactly on the replay horizon.",
            observed_emitted_windows, live_replay.expected_emitted_windows
        );
        window_semantics_note = Some(match window_semantics_note {
            Some(existing) => format!("{existing} {boundary_note}"),
            None => boundary_note,
        });
    }
    let correctness_result = validate_live_only_rows(
        &observed_rows,
        &expected_live_averages,
        baseline_entities,
        live_replay,
        observed_emitted_windows,
        observed_rows.len(),
        live_event_count,
    );
    let correctness_ok = correctness_result.is_ok();
    let correctness_diagnostics = correctness_result.err().map(|reason| {
        build_correctness_diagnostics(
            "live_only",
            baseline_entities,
            &observed_rows,
            if live_replay.mode == LiveReplayMode::Realtime {
                Some(live_replay.expected_emitted_windows)
            } else {
                None
            },
            observed_emitted_windows,
            vec!["sensor".to_string(), "minuteAvgValue".to_string()],
            reason,
        )
    });
    if verbose {
        if let Some(diagnostics) = &correctness_diagnostics {
            eprintln!("[live_only] correctness failed: {}", diagnostics.reason);
            eprintln!(
                "[live_only] expected_result_count={} observed_result_count={} expected_emitted_windows={:?} observed_emitted_windows={} expected_variables={:?} observed_variables={:?}",
                diagnostics.expected_result_count,
                diagnostics.observed_result_count,
                diagnostics.expected_emitted_windows,
                diagnostics.observed_emitted_windows,
                diagnostics.expected_variables,
                diagnostics.observed_variables
            );
            eprintln!("[live_only] first_observed_rows={:?}", diagnostics.first_observed_rows);
        }
    }
    let metrics = QueryDefinedBaselineVariantMetrics {
        variant: "live_only".to_string(),
        run_index,
        historical_events,
        baseline_entities,
        live_replay_mode: live_replay.mode.as_str().to_string(),
        live_rate_hz: live_replay.rate_hz,
        live_duration_seconds: live_replay.live_duration_seconds,
        live_window_size_seconds: live_replay.live_window_size_seconds,
        live_window_slide_seconds: live_replay.live_window_slide_seconds,
        live_event_count,
        expected_emitted_windows: live_replay.expected_emitted_windows,
        expected_full_windows: live_replay.expected_full_windows,
        warmup_window_count: live_replay.warmup_window_count,
        observed_emitted_windows,
        window_semantics_note,
        historical_generation_ms: Some(prepared.historical_generation_ms),
        storage_write_ms: Some(prepared.storage_write_ms),
        baseline_eval_ms: None,
        materialization_ms: None,
        static_injection_ms: None,
        historical_query_ms: None,
        baseline_materialization_ms: None,
        static_graph_injection_ms: None,
        live_startup_ms,
        first_result_latency_ms,
        peak_rss_mb: None,
        mean_rss_mb: None,
        peak_cpu_percent: None,
        mean_cpu_percent: None,
        sample_count: 0,
        result_count: observed_rows.len(),
        correctness_ok,
        correctness_diagnostics,
        materialized_quad_count: None,
        baseline_binding_count: None,
        window_result_latencies_ms,
        completed_window_latencies_ms,
        completed_window_result_counts,
        observed_rows,
    };

    Ok(VariantRunData { metrics })
}

fn sensor_iri(sensor_idx: usize) -> String {
    format!("{PREFIX}sensor{sensor_idx}")
}

fn log_stage(verbose: bool, variant: &str, stage: &str) {
    if verbose {
        eprintln!("[{variant}] stage={stage}");
    }
}

fn log_lowered_live_query(label: &str, query: &str) {
    eprintln!("[{}] lowered live query:", label);
    for (index, line) in query.lines().enumerate() {
        eprintln!("{:>4} | {}", index + 1, line);
    }
}

fn wait_for_live_event_schedule(replay_start: Instant, event_index: usize, rate_hz: f64) {
    let target = replay_start + Duration::from_secs_f64(event_index as f64 / rate_hz);
    if let Some(remaining) = target.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

fn drain_available_live_results(
    processor: &LiveStreamProcessing,
    first_event_started: Instant,
    results: &mut Vec<TimedBinding>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut made_progress = false;
    while let Some(result) = processor.try_receive_result()? {
        let received_after_first_event_ms = first_event_started.elapsed().as_secs_f64() * 1_000.0;
        results.push(TimedBinding { result, received_after_first_event_ms });
        made_progress = true;
    }

    Ok(made_progress)
}

fn collect_realtime_live_results(
    processor: &LiveStreamProcessing,
    first_event_started: Instant,
    results: &mut Vec<TimedBinding>,
) -> Result<(), Box<dyn std::error::Error>> {
    let idle_timeout = Duration::from_millis(250);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_progress = Instant::now();

    loop {
        if drain_available_live_results(processor, first_event_started, results)? {
            last_progress = Instant::now();
        } else if last_progress.elapsed() >= idle_timeout {
            break;
        }

        if Instant::now() >= deadline {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    if results.is_empty() {
        return Err("timed out waiting for realtime live results".into());
    }

    Ok(())
}

fn summarize_observed_windows(
    rows: &[QueryDefinedBaselineObservedRow],
) -> Vec<ObservedWindowSummary> {
    let mut by_window: std::collections::BTreeMap<
        (i64, i64),
        Vec<&QueryDefinedBaselineObservedRow>,
    > = std::collections::BTreeMap::new();

    for row in rows {
        by_window.entry((row.timestamp_from, row.timestamp_to)).or_default().push(row);
    }

    by_window
        .into_iter()
        .map(|((_timestamp_from, _timestamp_to), rows)| ObservedWindowSummary {
            result_count: rows.len(),
            first_result_latency_ms: rows
                .iter()
                .map(|row| row.received_after_first_event_ms)
                .fold(f64::INFINITY, f64::min),
        })
        .collect()
}

fn parse_live_rows(
    results: &[TimedBinding],
) -> Result<Vec<QueryDefinedBaselineObservedRow>, Box<dyn std::error::Error>> {
    let converter = ResultConverter::new("query_defined_baseline".to_string());
    let mut rows = Vec::new();

    for result in results {
        let converted = converter.from_live_binding(result.result.clone());
        let binding = converted.bindings.first().ok_or("live result did not contain bindings")?;

        let sensor = binding.get("sensor").cloned().ok_or("live result missing sensor binding")?;
        let minute_avg_value = parse_numeric(
            binding
                .get("minuteAvgValue")
                .ok_or("live result missing minuteAvgValue binding")?,
        )?;
        let day_avg_value =
            binding.get("dayAvgValue").map(|value| parse_numeric(value)).transpose()?;
        let difference = binding.get("difference").map(|value| parse_numeric(value)).transpose()?;

        let observed = QueryDefinedBaselineObservedRow {
            sensor,
            minute_avg_value,
            day_avg_value,
            difference,
            received_after_first_event_ms: result.received_after_first_event_ms,
            timestamp_from: result.result.timestamp_from,
            timestamp_to: result.result.timestamp_to,
        };
        rows.push(observed);
    }

    rows.sort_by(|left, right| {
        left.timestamp_from
            .cmp(&right.timestamp_from)
            .then_with(|| left.timestamp_to.cmp(&right.timestamp_to))
            .then_with(|| left.sensor.cmp(&right.sensor))
    });

    Ok(rows)
}

fn collect_live_results(
    processor: &LiveStreamProcessing,
    first_event_started: Instant,
    expected_results: usize,
) -> Result<Vec<TimedBinding>, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut timed_results = Vec::new();
    let mut last_progress = Instant::now();

    loop {
        let mut made_progress = false;
        while let Some(result) = processor.try_receive_result()? {
            let received_after_first_event_ms =
                first_event_started.elapsed().as_secs_f64() * 1_000.0;
            timed_results.push(TimedBinding { result, received_after_first_event_ms });
            if timed_results.len() >= expected_results {
                return Ok(timed_results);
            }
            made_progress = true;
            last_progress = Instant::now();
        }

        if made_progress && last_progress.elapsed() >= Duration::from_millis(50) {
            break;
        }

        if Instant::now() > deadline {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    if timed_results.is_empty() {
        return Err("timed out waiting for live results".into());
    }

    Ok(timed_results)
}

fn expected_live_averages(live_events: &[RDFEvent]) -> HashMap<String, f64> {
    let mut by_sensor: HashMap<String, (f64, usize)> = HashMap::new();
    for event in live_events {
        let sensor = normalize_binding_term(&event.subject.to_string());
        let value = event.object.to_string().trim().parse::<f64>().unwrap_or(0.0);
        let entry = by_sensor.entry(sensor).or_insert((0.0, 0));
        entry.0 += value;
        entry.1 += 1;
    }

    by_sensor
        .into_iter()
        .map(|(sensor, (sum, count))| (sensor, if count == 0 { 0.0 } else { sum / count as f64 }))
        .collect()
}

fn expected_day_averages(bindings: &[HashMap<String, String>]) -> HashMap<String, f64> {
    let mut expected = HashMap::new();
    for binding in bindings {
        if let (Some(sensor), Some(day_avg)) = (binding.get("sensor"), binding.get("dayAvgValue")) {
            if let Ok(value) = parse_numeric(day_avg) {
                expected.insert(normalize_binding_term(sensor), value);
            }
        }
    }

    expected
}

fn observed_query_variables(rows: &[QueryDefinedBaselineObservedRow]) -> Vec<String> {
    let mut vars = vec!["sensor".to_string(), "minuteAvgValue".to_string()];
    if rows.iter().any(|row| row.day_avg_value.is_some()) {
        vars.push("dayAvgValue".to_string());
    }
    if rows.iter().any(|row| row.difference.is_some()) {
        vars.push("difference".to_string());
    }
    vars
}

fn build_correctness_diagnostics(
    variant: &str,
    expected_result_count: usize,
    observed_rows: &[QueryDefinedBaselineObservedRow],
    expected_emitted_windows: Option<usize>,
    observed_emitted_windows: usize,
    expected_variables: Vec<String>,
    reason: String,
) -> QueryDefinedBaselineCorrectnessDiagnostics {
    QueryDefinedBaselineCorrectnessDiagnostics {
        variant: variant.to_string(),
        expected_result_count,
        observed_result_count: observed_rows.len(),
        expected_emitted_windows,
        observed_emitted_windows,
        expected_variables,
        observed_variables: observed_query_variables(observed_rows),
        first_observed_rows: observed_rows.iter().take(3).cloned().collect(),
        reason,
    }
}

fn validate_baseline_rows(
    rows: &[QueryDefinedBaselineObservedRow],
    expected_live_averages: &HashMap<String, f64>,
    expected_day_averages: &HashMap<String, f64>,
    expected_entities: usize,
    live_replay: &ResolvedLiveReplayConfig,
    observed_emitted_windows: usize,
    observed_row_count: usize,
    live_event_count: usize,
) -> Result<(), String> {
    if expected_day_averages.len() != expected_entities {
        return Err(format!(
            "expected {} baseline bindings but found {}",
            expected_entities,
            expected_day_averages.len()
        ));
    }
    if observed_row_count != expected_entities {
        return Err(format!(
            "expected {} baseline rows but observed {}",
            expected_entities, observed_row_count
        ));
    }
    if live_replay.mode == LiveReplayMode::Realtime {
        if live_event_count != live_replay.live_event_count {
            return Err(format!(
                "expected {} live events but observed {}",
                live_replay.live_event_count, live_event_count
            ));
        }
        if observed_emitted_windows != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} emitted windows but observed {}",
                live_replay.expected_emitted_windows, observed_emitted_windows
            ));
        }
        if summarize_observed_windows(rows).len() != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} summarized windows but observed {}",
                live_replay.expected_emitted_windows,
                summarize_observed_windows(rows).len()
            ));
        }
    }

    for row in rows {
        let expected_live = expected_live_averages
            .get(&row.sensor)
            .ok_or_else(|| format!("missing expected live aggregate for sensor {}", row.sensor))?;
        if (row.minute_avg_value - expected_live).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} minuteAvgValue {:.6} did not match expected {:.6}",
                row.sensor, row.minute_avg_value, expected_live
            ));
        }
        let expected_day = expected_day_averages
            .get(&row.sensor)
            .ok_or_else(|| format!("missing expected day aggregate for sensor {}", row.sensor))?;
        let day_avg = row
            .day_avg_value
            .ok_or_else(|| format!("sensor {} is missing dayAvgValue", row.sensor))?;
        if (day_avg - expected_day).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} dayAvgValue {:.6} did not match expected {:.6}",
                row.sensor, day_avg, expected_day
            ));
        }
        let difference = row
            .difference
            .ok_or_else(|| format!("sensor {} is missing difference", row.sensor))?;
        if (row.minute_avg_value - day_avg - difference).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} difference {:.6} was not minuteAvgValue - dayAvgValue",
                row.sensor, difference
            ));
        }
    }

    Ok(())
}

fn validate_live_only_rows(
    rows: &[QueryDefinedBaselineObservedRow],
    expected_live_averages: &HashMap<String, f64>,
    expected_entities: usize,
    live_replay: &ResolvedLiveReplayConfig,
    observed_emitted_windows: usize,
    observed_row_count: usize,
    live_event_count: usize,
) -> Result<(), String> {
    if observed_row_count != expected_entities {
        return Err(format!(
            "expected {} live-only rows but observed {}",
            expected_entities, observed_row_count
        ));
    }
    if live_replay.mode == LiveReplayMode::Realtime {
        if live_event_count != live_replay.live_event_count {
            return Err(format!(
                "expected {} live events but observed {}",
                live_replay.live_event_count, live_event_count
            ));
        }
        if observed_emitted_windows != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} emitted windows but observed {}",
                live_replay.expected_emitted_windows, observed_emitted_windows
            ));
        }
        if summarize_observed_windows(rows).len() != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} summarized windows but observed {}",
                live_replay.expected_emitted_windows,
                summarize_observed_windows(rows).len()
            ));
        }
    }

    for row in rows {
        let expected_live = expected_live_averages
            .get(&row.sensor)
            .ok_or_else(|| format!("missing expected live aggregate for sensor {}", row.sensor))?;
        if (row.minute_avg_value - expected_live).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} minuteAvgValue {:.6} did not match expected {:.6}",
                row.sensor, row.minute_avg_value, expected_live
            ));
        }
        if row.day_avg_value.is_some() {
            return Err(format!("sensor {} unexpectedly produced dayAvgValue", row.sensor));
        }
        if row.difference.is_some() {
            return Err(format!("sensor {} unexpectedly produced difference", row.sensor));
        }
    }

    Ok(())
}

fn parse_numeric(raw: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    let cleaned = trimmed
        .strip_prefix('"')
        .and_then(|value| value.split('"').next())
        .unwrap_or(trimmed)
        .split("^^")
        .next()
        .unwrap_or(trimmed);
    Ok(cleaned.parse::<f64>()?)
}

fn materialize_query_defined_baseline_quads(
    parsed: &ParsedJanusQuery,
    bindings: &[HashMap<String, String>],
) -> Result<Vec<Quad>, Box<dyn std::error::Error>> {
    let definition = parsed
        .ast
        .baseline_definitions
        .iter()
        .find(|definition| definition.name == BASELINE_QUERY_NAME)
        .ok_or("missing baseline definition")?;
    let template = parsed
        .baseline_graph_templates
        .iter()
        .find(|template| template.baseline_name == BASELINE_QUERY_NAME)
        .ok_or("missing baseline graph template")?;

    let graph_name = GraphName::NamedNode(NamedNode::new(BASELINE_GRAPH)?);
    let mut quads = Vec::new();
    for binding in bindings {
        for triple in &template.triples {
            let subject = resolve_subject_term(triple, binding)?;
            let predicate = resolve_predicate_term(triple)?;
            let object = resolve_object_term(triple, binding)?;
            quads.push(Quad::new(subject, predicate, object, graph_name.clone()));
        }
    }

    validate_template_against_definition(definition, template)?;
    Ok(quads)
}

fn validate_template_against_definition(
    definition: &BaselineDefinition,
    template: &BaselineGraphTemplate,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_variables = definition
        .output_variables
        .iter()
        .map(|variable| variable.trim_start_matches('?'))
        .collect::<std::collections::HashSet<_>>();

    for triple in &template.triples {
        for term in [&triple.subject, &triple.object] {
            if let GraphTermTemplate::Variable(variable_name) = term {
                if !output_variables.contains(variable_name.as_str()) {
                    return Err(format!(
                        "template references variable '?{}' that is not produced by the baseline SELECT output",
                        variable_name
                    )
                    .into());
                }
            }
        }

        if matches!(triple.predicate, GraphTermTemplate::Variable(_)) {
            return Err("baseline GRAPH template predicates must be concrete IRIs".into());
        }
    }

    Ok(())
}

fn resolve_subject_term(
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<NamedOrBlankNode, Box<dyn std::error::Error>> {
    match &triple.subject {
        GraphTermTemplate::Variable(name) => parse_named_or_blank_node(
            binding
                .get(name)
                .ok_or_else(|| format!("missing GRAPH template variable '?{}'", name))?,
        ),
        GraphTermTemplate::Iri(iri) => parse_named_or_blank_node(iri),
        GraphTermTemplate::Literal(raw) => Err(format!(
            "GRAPH template has a literal subject '{}', but subjects must be IRIs or blank nodes",
            raw
        )
        .into()),
    }
}

fn resolve_predicate_term(
    triple: &TripleTemplate,
) -> Result<NamedNode, Box<dyn std::error::Error>> {
    match &triple.predicate {
        GraphTermTemplate::Iri(iri) => Ok(NamedNode::new(iri.clone())?),
        GraphTermTemplate::Variable(name) => {
            Err(format!("GRAPH template uses variable predicate '?{}'", name).into())
        }
        GraphTermTemplate::Literal(raw) => Err(format!(
            "GRAPH template has a literal predicate '{}', but predicates must be IRIs",
            raw
        )
        .into()),
    }
}

fn resolve_object_term(
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<Term, Box<dyn std::error::Error>> {
    match &triple.object {
        GraphTermTemplate::Variable(name) => {
            let raw_value = binding
                .get(name)
                .ok_or_else(|| format!("missing GRAPH template variable '?{}'", name))?;
            Ok(parse_term(raw_value)?)
        }
        GraphTermTemplate::Iri(iri) => Ok(parse_term(iri)?),
        GraphTermTemplate::Literal(raw) => Ok(Term::Literal(parse_literal_term(raw)?)),
    }
}

fn parse_named_or_blank_node(raw: &str) -> Result<NamedOrBlankNode, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    if let Some(name) = trimmed.strip_prefix("_:") {
        Ok(NamedOrBlankNode::BlankNode(BlankNode::new(name)?))
    } else {
        let iri = trimmed.trim_start_matches('<').trim_end_matches('>');
        Ok(NamedOrBlankNode::NamedNode(NamedNode::new(iri)?))
    }
}

fn parse_term(raw: &str) -> Result<Term, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    if trimmed.starts_with("_:")
        || trimmed.starts_with('<')
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
    {
        if let Some(name) = trimmed.strip_prefix("_:") {
            return Ok(Term::BlankNode(BlankNode::new(name)?));
        }
        let iri = trimmed.trim_start_matches('<').trim_end_matches('>');
        return Ok(Term::NamedNode(NamedNode::new(iri)?));
    }

    Ok(Term::Literal(parse_literal_term(trimmed)?))
}

fn parse_literal_term(raw: &str) -> Result<oxigraph::model::Literal, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    if let Ok(value) = trimmed.parse::<i64>() {
        return Ok(oxigraph::model::Literal::new_typed_literal(
            value.to_string(),
            NamedNode::new("http://www.w3.org/2001/XMLSchema#integer")?,
        ));
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return Ok(oxigraph::model::Literal::new_typed_literal(
            value.to_string(),
            NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal")?,
        ));
    }

    if !trimmed.starts_with('"') {
        return Ok(oxigraph::model::Literal::new_simple_literal(trimmed));
    }

    let (lexical, suffix) = split_literal_lexical_and_suffix(trimmed)?;
    let lexical = unescape_literal_lexical(lexical);

    if let Some(language) = suffix.strip_prefix('@') {
        return Ok(oxigraph::model::Literal::new_language_tagged_literal(lexical, language)?);
    }

    if let Some(datatype_iri) = suffix.strip_prefix("^^") {
        let datatype = if datatype_iri.starts_with('<') && datatype_iri.ends_with('>') {
            &datatype_iri[1..datatype_iri.len() - 1]
        } else {
            datatype_iri
        };
        return Ok(oxigraph::model::Literal::new_typed_literal(lexical, NamedNode::new(datatype)?));
    }

    Ok(oxigraph::model::Literal::new_simple_literal(lexical))
}

fn split_literal_lexical_and_suffix(raw: &str) -> Result<(&str, &str), Box<dyn std::error::Error>> {
    let mut escaped = false;

    for (index, ch) in raw.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Ok((&raw[1..index], raw[index + 1..].trim())),
            _ => {}
        }
    }

    Err("missing closing quote in literal".into())
}

fn unescape_literal_lexical(lexical: &str) -> String {
    let mut result = String::with_capacity(lexical.len());
    let mut chars = lexical.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    other => {
                        result.push('\\');
                        result.push(other);
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn write_report_json(
    path: &Path,
    report: &QueryDefinedBaselineBenchmarkReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, report)?;
    Ok(())
}

fn summarize_comparisons(
    report: &QueryDefinedBaselineBenchmarkReport,
) -> QueryDefinedBaselineBenchmarkCsvRows {
    let mut by_config: HashMap<(usize, usize), Vec<&QueryDefinedBaselineComparisonRow>> =
        HashMap::new();
    for comparison in &report.comparisons {
        if comparison.is_warmup {
            continue;
        }
        by_config
            .entry((comparison.historical_events, comparison.baseline_entities))
            .or_default()
            .push(comparison);
    }

    let mut matrix_summaries = by_config
        .into_iter()
        .map(|((historical_events, baseline_entities), rows)| {
            summarize_matrix_config(
                &report.profile,
                historical_events,
                baseline_entities,
                report.warmup_runs,
                rows,
            )
        })
        .collect::<Vec<_>>();

    matrix_summaries.sort_by(|left, right| {
        left.historical_events
            .cmp(&right.historical_events)
            .then_with(|| left.baseline_entities.cmp(&right.baseline_entities))
    });

    QueryDefinedBaselineBenchmarkCsvRows { matrix_summaries }
}

fn summarize_matrix_config(
    profile: &str,
    historical_events: usize,
    baseline_entities: usize,
    warmup_runs: usize,
    rows: Vec<&QueryDefinedBaselineComparisonRow>,
) -> QueryDefinedBaselineMatrixSummaryRow {
    let correctness_rate = if rows.is_empty() {
        0.0
    } else {
        rows.iter()
            .filter(|run| run.baseline.correctness_ok && run.live_only.correctness_ok)
            .count() as f64
            / rows.len() as f64
    };

    let baseline_rows = rows.iter().map(|comparison| &comparison.baseline).collect::<Vec<_>>();
    let live_rows = rows.iter().map(|comparison| &comparison.live_only).collect::<Vec<_>>();
    let first_baseline = baseline_rows.first().copied();
    let observed_baseline_rows = rows
        .iter()
        .map(|comparison| comparison.observed_baseline_rows as f64)
        .collect::<Vec<_>>();
    let observed_live_only_rows = rows
        .iter()
        .map(|comparison| comparison.observed_live_only_rows as f64)
        .collect::<Vec<_>>();
    let observed_emitted_windows = rows
        .iter()
        .map(|comparison| comparison.baseline.observed_emitted_windows as f64)
        .collect::<Vec<_>>();

    let baseline_eval_ms = collect_optional_metric(&baseline_rows, |run| run.baseline_eval_ms);
    let materialization_ms = collect_optional_metric(&baseline_rows, |run| run.materialization_ms);
    let static_injection_ms =
        collect_optional_metric(&baseline_rows, |run| run.static_injection_ms);
    let historical_generation_ms =
        collect_optional_metric(&baseline_rows, |run| run.historical_generation_ms);
    let storage_write_ms = collect_optional_metric(&baseline_rows, |run| run.storage_write_ms);
    let baseline_binding_count = collect_optional_metric(&baseline_rows, |run| {
        run.baseline_binding_count.map(|value| value as f64)
    });
    let materialized_quad_count = collect_optional_metric(&baseline_rows, |run| {
        run.materialized_quad_count.map(|value| value as f64)
    });

    let baseline_first_result =
        baseline_rows.iter().map(|run| run.first_result_latency_ms).collect::<Vec<_>>();
    let live_only_first_result =
        live_rows.iter().map(|run| run.first_result_latency_ms).collect::<Vec<_>>();
    let startup_overheads = rows
        .iter()
        .map(|comparison| comparison.live_startup_overhead_ms)
        .collect::<Vec<_>>();
    let first_result_overheads = rows
        .iter()
        .map(|comparison| comparison.first_result_overhead_ms)
        .collect::<Vec<_>>();
    let peak_rss_mb = rows.iter().map(|comparison| comparison.peak_rss_mb).collect::<Vec<_>>();
    let mean_rss_mb = rows.iter().map(|comparison| comparison.mean_rss_mb).collect::<Vec<_>>();
    let peak_cpu_percent =
        rows.iter().map(|comparison| comparison.peak_cpu_percent).collect::<Vec<_>>();
    let mean_cpu_percent =
        rows.iter().map(|comparison| comparison.mean_cpu_percent).collect::<Vec<_>>();
    let baseline_result_counts =
        baseline_rows.iter().map(|run| run.result_count as f64).collect::<Vec<_>>();
    let live_only_result_counts =
        live_rows.iter().map(|run| run.result_count as f64).collect::<Vec<_>>();

    let baseline_eval_stats = baseline_eval_ms.as_ref().map(|values| stats(values));
    let materialization_stats = materialization_ms.as_ref().map(|values| stats(values));
    let static_injection_stats = static_injection_ms.as_ref().map(|values| stats(values));
    let baseline_binding_stats = baseline_binding_count.as_ref().map(|values| stats(values));
    let materialized_quad_stats = materialized_quad_count.as_ref().map(|values| stats(values));
    let peak_rss_stats = collect_optional_metric_from_rows(&peak_rss_mb);
    let mean_rss_stats = collect_optional_metric_from_rows(&mean_rss_mb);
    let peak_cpu_stats = collect_optional_metric_from_rows(&peak_cpu_percent);
    let mean_cpu_stats = collect_optional_metric_from_rows(&mean_cpu_percent);
    let expected_emitted_windows = first_baseline.map(|run| run.expected_emitted_windows as f64);
    let expected_full_windows = first_baseline.map(|run| run.expected_full_windows as f64);
    let warmup_window_count =
        first_baseline.and_then(|run| run.warmup_window_count.map(|value| value as f64));
    let observed_emitted_windows_stats = stats(&observed_emitted_windows);
    let observed_baseline_rows_stats = stats(&observed_baseline_rows);
    let observed_live_only_rows_stats = stats(&observed_live_only_rows);

    QueryDefinedBaselineMatrixSummaryRow {
        profile: profile.to_string(),
        historical_events,
        baseline_entities,
        runs: rows.len(),
        warmup_runs,
        correctness_rate,
        expected_emitted_windows,
        expected_full_windows,
        warmup_window_count,
        observed_emitted_windows_mean: observed_emitted_windows_stats.mean,
        observed_emitted_windows_std: observed_emitted_windows_stats.std,
        observed_baseline_rows_mean: observed_baseline_rows_stats.mean,
        observed_baseline_rows_std: observed_baseline_rows_stats.std,
        observed_live_only_rows_mean: observed_live_only_rows_stats.mean,
        observed_live_only_rows_std: observed_live_only_rows_stats.std,
        historical_generation_ms_mean: historical_generation_ms
            .as_ref()
            .map(|values| stats(values).mean),
        historical_generation_ms_std: historical_generation_ms
            .as_ref()
            .map(|values| stats(values).std),
        storage_write_ms_mean: storage_write_ms.as_ref().map(|values| stats(values).mean),
        storage_write_ms_std: storage_write_ms.as_ref().map(|values| stats(values).std),
        peak_rss_mb_mean: peak_rss_stats.as_ref().map(|value| value.mean),
        peak_rss_mb_std: peak_rss_stats.as_ref().map(|value| value.std),
        mean_rss_mb_mean: mean_rss_stats.as_ref().map(|value| value.mean),
        mean_rss_mb_std: mean_rss_stats.as_ref().map(|value| value.std),
        peak_cpu_percent_mean: peak_cpu_stats.as_ref().map(|value| value.mean),
        peak_cpu_percent_std: peak_cpu_stats.as_ref().map(|value| value.std),
        mean_cpu_percent_mean: mean_cpu_stats.as_ref().map(|value| value.mean),
        mean_cpu_percent_std: mean_cpu_stats.as_ref().map(|value| value.std),
        baseline_eval_ms_mean: baseline_eval_stats.as_ref().map(|value| value.mean),
        baseline_eval_ms_std: baseline_eval_stats.as_ref().map(|value| value.std),
        materialization_ms_mean: materialization_stats.as_ref().map(|value| value.mean),
        materialization_ms_std: materialization_stats.as_ref().map(|value| value.std),
        static_injection_ms_mean: static_injection_stats.as_ref().map(|value| value.mean),
        static_injection_ms_std: static_injection_stats.as_ref().map(|value| value.std),
        baseline_first_result_ms_mean: stats(&baseline_first_result).mean,
        baseline_first_result_ms_std: stats(&baseline_first_result).std,
        live_only_first_result_ms_mean: stats(&live_only_first_result).mean,
        live_only_first_result_ms_std: stats(&live_only_first_result).std,
        startup_overhead_ms_mean: stats(&startup_overheads).mean,
        startup_overhead_ms_std: stats(&startup_overheads).std,
        first_result_overhead_ms_mean: stats(&first_result_overheads).mean,
        first_result_overhead_ms_std: stats(&first_result_overheads).std,
        baseline_binding_count_mean: baseline_binding_stats.as_ref().map(|value| value.mean),
        baseline_binding_count_std: baseline_binding_stats.as_ref().map(|value| value.std),
        materialized_quad_count_mean: materialized_quad_stats.as_ref().map(|value| value.mean),
        materialized_quad_count_std: materialized_quad_stats.as_ref().map(|value| value.std),
        baseline_result_count_mean: stats(&baseline_result_counts).mean,
        baseline_result_count_std: stats(&baseline_result_counts).std,
        live_only_result_count_mean: stats(&live_only_result_counts).mean,
        live_only_result_count_std: stats(&live_only_result_counts).std,
    }
}

#[derive(Clone, Copy, Debug)]
struct MetricStats {
    mean: f64,
    std: f64,
}

fn stats(values: &[f64]) -> MetricStats {
    if values.is_empty() {
        return MetricStats { mean: 0.0, std: 0.0 };
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let std = if values.len() > 1 {
        let variance = values
            .iter()
            .map(|value| {
                let delta = value - mean;
                delta * delta
            })
            .sum::<f64>()
            / (values.len() as f64 - 1.0);
        variance.sqrt()
    } else {
        0.0
    };

    MetricStats { mean, std }
}

fn collect_optional_metric<F>(
    runs: &[&QueryDefinedBaselineVariantMetrics],
    f: F,
) -> Option<Vec<f64>>
where
    F: Fn(&QueryDefinedBaselineVariantMetrics) -> Option<f64>,
{
    let values = runs.iter().filter_map(|run| f(run)).collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn collect_optional_metric_from_rows(values: &[Option<f64>]) -> Option<MetricStats> {
    let filtered = values.iter().copied().flatten().collect::<Vec<_>>();
    if filtered.is_empty() {
        None
    } else {
        Some(stats(&filtered))
    }
}

fn summarize_resource_samples(samples: &[ResourceSample]) -> ResourceSummary {
    if samples.is_empty() {
        return ResourceSummary::default();
    }

    let rss_values = samples.iter().map(|sample| sample.rss_mb).collect::<Vec<_>>();
    let cpu_values = samples.iter().map(|sample| sample.cpu_percent).collect::<Vec<_>>();
    let rss_stats = stats(&rss_values);
    let cpu_stats = stats(&cpu_values);

    ResourceSummary {
        peak_rss_mb: rss_values.iter().copied().reduce(f64::max),
        mean_rss_mb: Some(rss_stats.mean),
        peak_cpu_percent: cpu_values.iter().copied().reduce(f64::max),
        mean_cpu_percent: Some(cpu_stats.mean),
        sample_count: samples.len(),
    }
}

fn write_summary_csv(
    path: &Path,
    rows: &QueryDefinedBaselineBenchmarkCsvRows,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "historical_events,baseline_entities,measured_runs,warmup_runs,correctness_rate,expected_emitted_windows,expected_full_windows,warmup_window_count,observed_emitted_windows_mean,observed_emitted_windows_std,observed_baseline_rows_mean,observed_baseline_rows_std,observed_live_only_rows_mean,observed_live_only_rows_std,baseline_binding_count_mean,materialized_quad_count_mean,baseline_result_count_mean,live_only_result_count_mean,baseline_eval_ms_mean,baseline_eval_ms_std,materialization_ms_mean,materialization_ms_std,static_injection_ms_mean,static_injection_ms_std,baseline_first_result_ms_mean,baseline_first_result_ms_std,live_only_first_result_ms_mean,live_only_first_result_ms_std,startup_overhead_ms_mean,startup_overhead_ms_std,first_result_overhead_ms_mean,first_result_overhead_ms_std,historical_generation_ms_mean,historical_generation_ms_std,storage_write_ms_mean,storage_write_ms_std,peak_rss_mb_mean,peak_rss_mb_std,mean_rss_mb_mean,mean_rss_mb_std,peak_cpu_percent_mean,peak_cpu_percent_std,mean_cpu_percent_mean,mean_cpu_percent_std"
    )?;

    for row in &rows.matrix_summaries {
        let columns = vec![
            row.historical_events.to_string(),
            row.baseline_entities.to_string(),
            row.runs.to_string(),
            row.warmup_runs.to_string(),
            format!("{:.3}", row.correctness_rate),
            format_opt(row.expected_emitted_windows),
            format_opt(row.expected_full_windows),
            format_opt(row.warmup_window_count),
            format!("{:.3}", row.observed_emitted_windows_mean),
            format!("{:.3}", row.observed_emitted_windows_std),
            format!("{:.3}", row.observed_baseline_rows_mean),
            format!("{:.3}", row.observed_baseline_rows_std),
            format!("{:.3}", row.observed_live_only_rows_mean),
            format!("{:.3}", row.observed_live_only_rows_std),
            format_opt(row.baseline_binding_count_mean),
            format_opt(row.materialized_quad_count_mean),
            format!("{:.3}", row.baseline_result_count_mean),
            format!("{:.3}", row.live_only_result_count_mean),
            format_opt(row.baseline_eval_ms_mean),
            format_opt(row.baseline_eval_ms_std),
            format_opt(row.materialization_ms_mean),
            format_opt(row.materialization_ms_std),
            format_opt(row.static_injection_ms_mean),
            format_opt(row.static_injection_ms_std),
            format!("{:.3}", row.baseline_first_result_ms_mean),
            format!("{:.3}", row.baseline_first_result_ms_std),
            format!("{:.3}", row.live_only_first_result_ms_mean),
            format!("{:.3}", row.live_only_first_result_ms_std),
            format!("{:.3}", row.startup_overhead_ms_mean),
            format!("{:.3}", row.startup_overhead_ms_std),
            format!("{:.3}", row.first_result_overhead_ms_mean),
            format!("{:.3}", row.first_result_overhead_ms_std),
            format_opt(row.historical_generation_ms_mean),
            format_opt(row.historical_generation_ms_std),
            format_opt(row.storage_write_ms_mean),
            format_opt(row.storage_write_ms_std),
            format_opt(row.peak_rss_mb_mean),
            format_opt(row.peak_rss_mb_std),
            format_opt(row.mean_rss_mb_mean),
            format_opt(row.mean_rss_mb_std),
            format_opt(row.peak_cpu_percent_mean),
            format_opt(row.peak_cpu_percent_std),
            format_opt(row.mean_cpu_percent_mean),
            format_opt(row.mean_cpu_percent_std),
        ];
        writeln!(file, "{}", columns.join(","))?;
    }

    Ok(())
}

fn format_opt(value: Option<f64>) -> String {
    value.map(|v| format!("{v:.3}")).unwrap_or_default()
}

fn write_summary_markdown(
    path: &Path,
    rows: &QueryDefinedBaselineBenchmarkCsvRows,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;
    writeln!(file, "| historical_events | baseline_entities | expected_emitted_windows | expected_full_windows | warmup_window_count | observed_emitted_windows | observed_baseline_rows | observed_live_only_rows | injected_quads | baseline_eval_ms | materialization_ms | static_injection_ms | first_result_overhead_ms | peak_rss_mb | mean_cpu_percent | correctness_rate |")?;
    writeln!(
        file,
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |"
    )?;

    for row in &rows.matrix_summaries {
        let injected_quads =
            row.materialized_quad_count_mean.unwrap_or(row.baseline_entities as f64);
        writeln!(
            file,
            "| {} | {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} | {} | {} | {:.3} |",
            row.historical_events,
            row.baseline_entities,
            format_opt(row.expected_emitted_windows),
            format_opt(row.expected_full_windows),
            format_opt(row.warmup_window_count),
            row.observed_emitted_windows_mean,
            row.observed_baseline_rows_mean,
            row.observed_live_only_rows_mean,
            format_decimal(injected_quads),
            format_mean_std(row.baseline_eval_ms_mean, row.baseline_eval_ms_std),
            format_mean_std(row.materialization_ms_mean, row.materialization_ms_std),
            format_mean_std(row.static_injection_ms_mean, row.static_injection_ms_std),
            format_mean_std(
                Some(row.first_result_overhead_ms_mean),
                Some(row.first_result_overhead_ms_std),
            ),
            format_mean_std(row.peak_rss_mb_mean, row.peak_rss_mb_std),
            format_mean_std(row.mean_cpu_percent_mean, row.mean_cpu_percent_std),
            row.correctness_rate,
        )?;
    }

    Ok(())
}

fn format_mean_std(mean: Option<f64>, std: Option<f64>) -> String {
    match (mean, std) {
        (Some(mean), Some(std)) => format!("{mean:.3} ± {std:.3}"),
        (Some(mean), None) => format!("{mean:.3} ± 0.000"),
        (None, _) => String::new(),
    }
}

fn format_decimal(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{value:.3}")
    }
}

fn normalize_binding_term(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accelerated_replay() -> ResolvedLiveReplayConfig {
        ResolvedLiveReplayConfig {
            mode: LiveReplayMode::Accelerated,
            rate_hz: 4.0,
            live_duration_seconds: None,
            live_window_size_seconds: None,
            live_window_slide_seconds: None,
            live_event_count: 10,
            event_interval_ms: 0.0,
            expected_emitted_windows: 0,
            expected_full_windows: 0,
            warmup_window_count: None,
        }
    }

    fn synthetic_row(
        sensor: &str,
        minute: f64,
        day: Option<f64>,
        diff: Option<f64>,
    ) -> QueryDefinedBaselineObservedRow {
        QueryDefinedBaselineObservedRow {
            sensor: sensor.to_string(),
            minute_avg_value: minute,
            day_avg_value: day,
            difference: diff,
            received_after_first_event_ms: 1.0,
            timestamp_from: 10,
            timestamp_to: 70,
        }
    }

    #[test]
    fn live_only_accelerated_mode_accepts_multiple_emitted_windows() {
        let rows = vec![
            synthetic_row("http://example.org/sensor0", 20.0, None, None),
            synthetic_row("http://example.org/sensor1", 30.0, None, None),
        ];
        let expected_live = HashMap::from([
            ("http://example.org/sensor0".to_string(), 20.0),
            ("http://example.org/sensor1".to_string(), 30.0),
        ]);

        let result =
            validate_live_only_rows(&rows, &expected_live, 2, &accelerated_replay(), 4, 2, 2);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn baseline_accelerated_mode_accepts_multiple_emitted_windows() {
        let rows = vec![
            synthetic_row("http://example.org/sensor0", 20.0, Some(10.0), Some(10.0)),
            synthetic_row("http://example.org/sensor1", 30.0, Some(11.0), Some(19.0)),
        ];
        let expected_live = HashMap::from([
            ("http://example.org/sensor0".to_string(), 20.0),
            ("http://example.org/sensor1".to_string(), 30.0),
        ]);
        let expected_day = HashMap::from([
            ("http://example.org/sensor0".to_string(), 10.0),
            ("http://example.org/sensor1".to_string(), 11.0),
        ]);

        let result = validate_baseline_rows(
            &rows,
            &expected_live,
            &expected_day,
            2,
            &accelerated_replay(),
            4,
            2,
            2,
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn failed_validation_builds_structured_diagnostics() {
        let rows = vec![synthetic_row("http://example.org/sensor0", 20.0, Some(10.0), None)];
        let expected_live = HashMap::from([("http://example.org/sensor0".to_string(), 20.0)]);

        let reason =
            validate_live_only_rows(&rows, &expected_live, 1, &accelerated_replay(), 1, 1, 1)
                .expect_err("validation should fail");
        let diagnostics = build_correctness_diagnostics(
            "live_only",
            1,
            &rows,
            None,
            1,
            vec!["sensor".to_string(), "minuteAvgValue".to_string()],
            reason.clone(),
        );

        assert_eq!(diagnostics.variant, "live_only");
        assert_eq!(diagnostics.expected_result_count, 1);
        assert_eq!(diagnostics.observed_result_count, 1);
        assert_eq!(diagnostics.reason, reason);
        assert_eq!(diagnostics.first_observed_rows.len(), 1);
        assert!(diagnostics.observed_variables.contains(&"dayAvgValue".to_string()));
    }
}
