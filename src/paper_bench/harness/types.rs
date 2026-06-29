use crate::{
    core::RDFEvent, paper_bench::external::ExternalHistoricalAdapter,
    storage::segmented_storage::StreamingSegmentedStorage,
};
use clap::ValueEnum;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

pub const BASELINE_NS: &str = "https://janus.rs/baseline#";
pub const GRAPH_URI: &str = "http://example.org/citybench";
pub const LIVE_STREAM_URI: &str = "http://example.org/live";
pub const CONGESTION_PREDICATE: &str = "http://example.org/congestionLevel";
pub const TRAFFIC_PREDICATE: &str = CONGESTION_PREDICATE;
pub const BASELINE_PREDICATE: &str = "http://example.org/baselineFlow";

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
pub struct JoinTraceRow {
    pub historical_join_key: Option<String>,
    pub live_join_key: Option<String>,
    pub accepted: bool,
    pub rejection_reason: Option<String>,
    pub historical_row: Option<Vec<(String, String)>>,
    pub live_row: Vec<(String, String)>,
    pub joined_row: Option<Vec<(String, String)>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EquivalenceReport {
    pub system_pair: String,
    pub run_index: usize,
    pub mode: String,
    pub historical_input_hash: String,
    pub live_input_hash: String,
    pub janus_result_count: usize,
    pub decomposed_result_count: usize,
    pub janus_result_hash: String,
    pub decomposed_result_hash: String,
    pub equivalent: bool,
    pub historical_inputs_semantically_equal: bool,
    pub live_inputs_semantically_equal: bool,
    pub notes: Vec<String>,
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

pub struct BaselineAccumulator {
    pub last_value: String,
    pub numeric_sum: f64,
    pub numeric_count: usize,
    pub all_numeric: bool,
}

impl BaselineAccumulator {
    pub fn new() -> Self {
        Self { last_value: String::new(), numeric_sum: 0.0, numeric_count: 0, all_numeric: true }
    }
}
