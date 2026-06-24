use clap::ValueEnum;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::{
    core::RDFEvent,
    paper_bench::harness::ReproMetadata,
    storage::segmented_storage::StreamingSegmentedStorage,
};

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
    pub metadata: ReproMetadata,
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

pub struct PreparedStorage {
    pub storage: Arc<StreamingSegmentedStorage>,
    pub historical_min_timestamp: u64,
    pub historical_max_timestamp: u64,
    pub historical_generation_ms: f64,
    pub storage_write_ms: f64,
    pub live_events: Vec<RDFEvent>,
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedLiveReplayConfig {
    pub mode: LiveReplayMode,
    pub rate_hz: f64,
    pub live_duration_seconds: Option<u64>,
    pub live_window_size_seconds: Option<u64>,
    pub live_window_slide_seconds: Option<u64>,
    pub live_event_count: usize,
    pub event_interval_ms: f64,
    pub expected_emitted_windows: usize,
    pub expected_full_windows: usize,
    pub warmup_window_count: Option<usize>,
}

pub struct HistoricalWriteStats {
    pub min_timestamp: u64,
    pub max_timestamp: u64,
    pub generation_ms: f64,
    pub storage_write_ms: f64,
}

#[derive(Clone, Debug)]
pub struct ResourceSample {
    pub rss_mb: f64,
    pub cpu_percent: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ResourceSummary {
    pub peak_rss_mb: Option<f64>,
    pub mean_rss_mb: Option<f64>,
    pub peak_cpu_percent: Option<f64>,
    pub mean_cpu_percent: Option<f64>,
    pub sample_count: usize,
}

pub struct ResourceSampler {
    pub stop: Arc<std::sync::atomic::AtomicBool>,
    pub samples: Arc<Mutex<Vec<ResourceSample>>>,
    pub handle: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct VariantRunData {
    pub metrics: QueryDefinedBaselineVariantMetrics,
}

#[derive(Debug)]
pub struct TimedBinding {
    pub result: rsp_rs::BindingWithTimestamp,
    pub received_after_first_event_ms: f64,
}

#[derive(Debug)]
pub struct ObservedWindowSummary {
    pub result_count: usize,
    pub first_result_latency_ms: f64,
}

pub struct QueryDefinedBaselineBenchmarkOutcome {
    pub report_path: PathBuf,
    pub summary_csv_path: PathBuf,
    pub summary_md_path: PathBuf,
    pub report: QueryDefinedBaselineBenchmarkReport,
    pub csv_rows: QueryDefinedBaselineBenchmarkCsvRows,
}
