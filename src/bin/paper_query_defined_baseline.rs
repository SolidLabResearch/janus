use clap::Parser;
use janus::paper_bench::cli_output::{print_benchmark_stdout, BenchmarkArtifact};
use janus::paper_bench::query_defined_baseline::{
    run_query_defined_baseline_benchmark, LiveReplayMode, QueryDefinedBaselineBenchmarkConfig,
    QueryDefinedBaselineProfile,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value_t = 0)]
    warmup_runs: usize,
    #[arg(long, default_value_t = 1)]
    runs: usize,
    #[arg(long, value_delimiter = ',', default_values_t = [1000usize], value_parser = parse_historical_event_count)]
    historical_events: Vec<usize>,
    #[arg(long, value_delimiter = ',', default_values_t = [1usize])]
    baseline_entities: Vec<usize>,
    #[arg(long, value_enum, default_value_t = LiveReplayMode::Accelerated)]
    live_replay_mode: LiveReplayMode,
    #[arg(long, default_value_t = 4.0)]
    live_rate_hz: f64,
    #[arg(long)]
    live_duration_seconds: Option<u64>,
    #[arg(long)]
    live_window_size_seconds: Option<u64>,
    #[arg(long)]
    live_window_slide_seconds: Option<u64>,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = QueryDefinedBaselineProfile::Smoke)]
    profile: QueryDefinedBaselineProfile,
    #[arg(long)]
    debug_lowered_query: bool,
    #[arg(long)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let outcome = run_query_defined_baseline_benchmark(QueryDefinedBaselineBenchmarkConfig {
        profile: args.profile,
        runs: args.runs,
        warmup_runs: args.warmup_runs,
        historical_events: args.historical_events,
        baseline_entities: args.baseline_entities,
        live_replay_mode: args.live_replay_mode,
        live_rate_hz: args.live_rate_hz,
        live_duration_seconds: args.live_duration_seconds,
        live_window_size_seconds: args.live_window_size_seconds,
        live_window_slide_seconds: args.live_window_slide_seconds,
        output_dir: args.output_dir,
        debug_lowered_query: args.debug_lowered_query,
        verbose: args.verbose,
    })?;

    print_benchmark_stdout(
        "query_defined_baseline",
        Some(outcome.report.correctness_passed),
        Some(outcome.report.warmup_runs),
        Some(outcome.report.runs),
        &outcome.report.output_dir,
        &[
            BenchmarkArtifact { label: "raw_json", path: &outcome.report_path },
            BenchmarkArtifact { label: "summary_csv", path: &outcome.summary_csv_path },
            BenchmarkArtifact { label: "summary_md", path: &outcome.summary_md_path },
        ],
    );

    if args.verbose {
        for (index, comparison) in outcome.report.comparisons.iter().enumerate() {
            let phase = if index < outcome.report.warmup_runs {
                "warmup"
            } else {
                "measured"
            };
            println!(
                "phase={} run={} mode={} baseline_first_result_ms={:.3} live_only_first_result_ms={:.3} baseline_results={} live_only_results={} baseline_emitted_windows={} live_only_emitted_windows={} observed_baseline_rows={} observed_live_only_rows={} expected_emitted_windows={} expected_full_windows={} warmup_window_count={} startup_overhead_ms={:.3} first_result_overhead_ms={:.3}",
                phase,
                comparison.run_index,
                comparison.baseline.live_replay_mode,
                comparison.baseline.first_result_latency_ms,
                comparison.live_only.first_result_latency_ms,
                comparison.baseline.result_count,
                comparison.live_only.result_count,
                comparison.baseline.observed_emitted_windows,
                comparison.live_only.observed_emitted_windows,
                comparison.observed_baseline_rows,
                comparison.observed_live_only_rows,
                comparison.baseline.expected_emitted_windows,
                comparison.baseline.expected_full_windows,
                comparison
                    .baseline
                    .warmup_window_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                comparison.live_startup_overhead_ms,
                comparison.first_result_overhead_ms
            );
            if !comparison.baseline.completed_window_latencies_ms.is_empty() {
                println!(
                    "baseline_window_latencies_ms={:?}",
                    comparison.baseline.completed_window_latencies_ms
                );
            }
            if !comparison.live_only.completed_window_latencies_ms.is_empty() {
                println!(
                    "live_only_window_latencies_ms={:?}",
                    comparison.live_only.completed_window_latencies_ms
                );
            }
        }
    }

    Ok(())
}

fn parse_historical_event_count(raw: &str) -> Result<usize, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("historical event count cannot be empty".to_string());
    }

    let (digits, multiplier) = match trimmed.chars().last() {
        Some('K') => (&trimmed[..trimmed.len() - 1], 1_000usize),
        Some('M') => (&trimmed[..trimmed.len() - 1], 1_000_000usize),
        _ => (trimmed, 1usize),
    };

    if digits.is_empty() {
        return Err(format!("invalid historical event count '{raw}'"));
    }

    let base = digits
        .parse::<usize>()
        .map_err(|_| format!("invalid historical event count '{raw}'"))?;
    base.checked_mul(multiplier)
        .ok_or_else(|| format!("historical event count '{raw}' is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_accelerated_live_replay() {
        let args =
            Args::try_parse_from(["paper_query_defined_baseline"]).expect("args should parse");
        assert_eq!(args.live_replay_mode, LiveReplayMode::Accelerated);
        assert!((args.live_rate_hz - 4.0).abs() < f64::EPSILON);
        assert!(args.live_duration_seconds.is_none());
        assert!(args.live_window_size_seconds.is_none());
        assert!(args.live_window_slide_seconds.is_none());
    }

    #[test]
    fn parses_realtime_live_replay_flags() {
        let args = Args::try_parse_from([
            "paper_query_defined_baseline",
            "--live-replay-mode",
            "realtime",
            "--live-rate-hz",
            "4",
            "--live-duration-seconds",
            "240",
            "--live-window-size-seconds",
            "120",
            "--live-window-slide-seconds",
            "60",
        ])
        .expect("args should parse");
        assert_eq!(args.live_replay_mode, LiveReplayMode::Realtime);
        assert!((args.live_rate_hz - 4.0).abs() < f64::EPSILON);
        assert_eq!(args.live_duration_seconds, Some(240));
        assert_eq!(args.live_window_size_seconds, Some(120));
        assert_eq!(args.live_window_slide_seconds, Some(60));
    }
}
