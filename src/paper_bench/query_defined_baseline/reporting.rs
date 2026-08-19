use std::collections::HashMap;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{
    LiveReplayMode, QueryDefinedBaselineBenchmarkConfig, QueryDefinedBaselineBenchmarkCsvRows,
    QueryDefinedBaselineBenchmarkReport, QueryDefinedBaselineComparisonRow,
    QueryDefinedBaselineMatrixSummaryRow, QueryDefinedBaselineVariantMetrics,
    ResolvedLiveReplayConfig, ResourceSample, ResourceSummary,
};

#[derive(Clone, Copy, Debug)]
pub struct MetricStats {
    pub mean: f64,
    pub std: f64,
}

pub fn default_output_dir() -> PathBuf {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    PathBuf::from(format!("logs/benchmark/query_defined_baseline/{ts}"))
}

pub fn resolve_live_replay_config(
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

pub fn expected_emitted_windows(live_duration_seconds: u64, window_slide_seconds: u64) -> usize {
    if window_slide_seconds == 0 {
        return 0;
    }

    (live_duration_seconds / window_slide_seconds) as usize
}

pub fn expected_full_windows(
    live_duration_seconds: u64,
    window_size_seconds: u64,
    window_slide_seconds: u64,
) -> usize {
    if live_duration_seconds < window_size_seconds {
        return 0;
    }

    1 + ((live_duration_seconds - window_size_seconds) / window_slide_seconds) as usize
}

pub fn write_report_json(
    path: &Path,
    report: &QueryDefinedBaselineBenchmarkReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, report)?;
    Ok(())
}

pub fn summarize_comparisons(
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

pub fn summarize_matrix_config(
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

pub fn stats(values: &[f64]) -> MetricStats {
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

pub fn collect_optional_metric<F>(
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

pub fn collect_optional_metric_from_rows(values: &[Option<f64>]) -> Option<MetricStats> {
    let filtered = values.iter().copied().flatten().collect::<Vec<_>>();
    if filtered.is_empty() {
        None
    } else {
        Some(stats(&filtered))
    }
}

pub fn summarize_resource_samples(samples: &[ResourceSample]) -> ResourceSummary {
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

pub fn write_summary_csv(
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

pub fn format_opt(value: Option<f64>) -> String {
    value.map(|v| format!("{v:.3}")).unwrap_or_default()
}

pub fn write_summary_markdown(
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

pub fn format_mean_std(mean: Option<f64>, std: Option<f64>) -> String {
    match (mean, std) {
        (Some(mean), Some(std)) => format!("{mean:.3} ± {std:.3}"),
        (Some(mean), None) => format!("{mean:.3} ± 0.000"),
        (None, _) => String::new(),
    }
}

pub fn format_decimal(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{value:.3}")
    }
}
