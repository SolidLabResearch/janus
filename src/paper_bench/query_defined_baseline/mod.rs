pub mod rdf;
pub mod reporting;
pub mod runner;
pub mod storage;
pub mod system;
pub mod types;
pub mod validation;

use crate::paper_bench::harness::{collect_repro_metadata, ensure_output_dir};
use crate::parsing::janusql_parser::JanusQLParser;
use std::path::PathBuf;

pub use types::{
    LiveReplayMode, QueryDefinedBaselineBenchmarkConfig, QueryDefinedBaselineBenchmarkCsvRows,
    QueryDefinedBaselineBenchmarkOutcome, QueryDefinedBaselineBenchmarkReport,
    QueryDefinedBaselineComparisonRow, QueryDefinedBaselineCorrectnessDiagnostics,
    QueryDefinedBaselineMatrixSummaryRow, QueryDefinedBaselineObservedRow,
    QueryDefinedBaselineProfile, QueryDefinedBaselineVariantMetrics,
};

pub const PREFIX: &str = "http://example.org/";
pub const STREAM_URI: &str = "http://example.org/stream";
pub const BASELINE_GRAPH: &str = "http://example.org/dayBaseline";
pub const BASELINE_QUERY_NAME: &str = "http://example.org/dayBaseline";
pub const RESOURCE_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

pub fn run_query_defined_baseline_benchmark(
    config: QueryDefinedBaselineBenchmarkConfig,
) -> Result<QueryDefinedBaselineBenchmarkOutcome, Box<dyn std::error::Error>> {
    let output_dir = config.output_dir.clone().unwrap_or_else(reporting::default_output_dir);
    ensure_output_dir(&output_dir)?;
    let metadata = collect_repro_metadata();
    let parser = JanusQLParser::new()?;
    let live_replay = reporting::resolve_live_replay_config(&config)?;
    let mut comparisons = Vec::new();

    for &historical_events in &config.historical_events {
        for &baseline_entities in &config.baseline_entities {
            let prepared = storage::prepare_storage(
                config.profile,
                historical_events,
                baseline_entities,
                config.verbose,
            )?;

            for run_index in 0..config.warmup_runs {
                let comparison = runner::run_single_comparison(
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
                let comparison = runner::run_single_comparison(
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

    reporting::write_report_json(&report_path, &report)?;
    let csv_rows = reporting::summarize_comparisons(&report);
    reporting::write_summary_csv(&summary_csv_path, &csv_rows)?;
    reporting::write_summary_markdown(&summary_md_path, &csv_rows)?;

    Ok(QueryDefinedBaselineBenchmarkOutcome {
        report_path,
        summary_csv_path,
        summary_md_path,
        report,
        csv_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use types::{LiveReplayMode, QueryDefinedBaselineObservedRow, ResolvedLiveReplayConfig};
    use validation::{
        build_correctness_diagnostics, validate_baseline_rows, validate_live_only_rows,
    };

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
