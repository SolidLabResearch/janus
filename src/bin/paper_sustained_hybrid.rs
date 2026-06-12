use clap::Parser;
use janus::paper_bench::cli_output::{
    default_benchmark_output_dir, print_benchmark_stdout, print_verbose_rows, BenchmarkArtifact,
};
use janus::paper_bench::{
    external::OxigraphExternalAdapter,
    harness::{
        collect_repro_metadata, ensure_output_dir, prepare_sustained_workload, run_sustained_pair,
        summarize_sustained, sustained_event_interval_ms,
        sustained_expected_wall_clock_duration_ms, write_jsonl, write_sustained_summary_csv,
        ExecutionMode, SustainedRow, SustainedRunConfig, TimeMode,
    },
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value_t = 0)]
    warmup_runs: usize,
    #[arg(long, default_value_t = 10)]
    runs: usize,
    #[arg(long, default_value_t = false)]
    include_warmups: bool,
    #[arg(long, default_value_t = false)]
    debug_equivalence: bool,
    #[arg(long, default_value_t = 10_000)]
    historical_events: usize,
    #[arg(long, default_value_t = 240)]
    live_duration_seconds: usize,
    #[arg(long, default_value_t = 1)]
    event_rate_hz: usize,
    #[arg(long, default_value_t = 120)]
    window_size_seconds: usize,
    #[arg(long, default_value_t = 60)]
    window_slide_seconds: usize,
    #[arg(long, value_enum, default_value_t = ExecutionMode::Warm)]
    mode: ExecutionMode,
    #[arg(long, value_enum, default_value_t = TimeMode::Virtual)]
    time_mode: TimeMode,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.event_rate_hz == 0 {
        return Err("--event-rate-hz must be at least 1".into());
    }
    let event_interval_ms = sustained_event_interval_ms(args.event_rate_hz);
    let expected_wall_clock_duration_ms =
        sustained_expected_wall_clock_duration_ms(args.live_duration_seconds);
    let output_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| default_benchmark_output_dir("paper_sustained_hybrid"));
    ensure_output_dir(&output_dir)?;

    let metadata = collect_repro_metadata();
    let adapter = OxigraphExternalAdapter::new();
    let warm_workload = if args.mode == ExecutionMode::Warm {
        Some(prepare_sustained_workload(
            args.historical_events,
            args.live_duration_seconds,
            args.event_rate_hz,
            args.window_size_seconds,
            args.window_slide_seconds,
        )?)
    } else {
        None
    };

    let mut all_rows = Vec::<SustainedRow>::with_capacity((args.warmup_runs + args.runs) * 2);

    for run_index in 0..args.warmup_runs {
        let pair = run_sustained_pair(SustainedRunConfig {
            mode: args.mode,
            time_mode: args.time_mode,
            run_index,
            is_warmup: true,
            historical_events: args.historical_events,
            live_duration_seconds: args.live_duration_seconds,
            event_rate_hz: args.event_rate_hz,
            event_interval_ms,
            expected_wall_clock_duration_ms,
            window_size_seconds: args.window_size_seconds,
            window_slide_seconds: args.window_slide_seconds,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: warm_workload.as_ref(),
            debug_output_dir: args.debug_equivalence.then_some(output_dir.as_path()),
        })?;
        all_rows.push(pair.unified);
        all_rows.push(pair.decomposed);
    }

    for run_index in 0..args.runs {
        let pair = run_sustained_pair(SustainedRunConfig {
            mode: args.mode,
            time_mode: args.time_mode,
            run_index,
            is_warmup: false,
            historical_events: args.historical_events,
            live_duration_seconds: args.live_duration_seconds,
            event_rate_hz: args.event_rate_hz,
            event_interval_ms,
            expected_wall_clock_duration_ms,
            window_size_seconds: args.window_size_seconds,
            window_slide_seconds: args.window_slide_seconds,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: warm_workload.as_ref(),
            debug_output_dir: args.debug_equivalence.then_some(output_dir.as_path()),
        })?;
        all_rows.push(pair.unified);
        all_rows.push(pair.decomposed);
    }

    let output_rows = if args.include_warmups {
        all_rows.clone()
    } else {
        all_rows.iter().filter(|row| !row.is_warmup).cloned().collect::<Vec<_>>()
    };

    let jsonl_path = output_dir.join("paper_sustained_hybrid.raw.jsonl");
    let csv_path = output_dir.join("paper_sustained_hybrid.summary.csv");
    write_jsonl(&jsonl_path, &output_rows)?;
    write_sustained_summary_csv(&csv_path, &summarize_sustained(&output_rows))?;

    print_benchmark_stdout(
        "paper_sustained_hybrid",
        None,
        Some(args.warmup_runs),
        Some(args.runs),
        &output_dir,
        &[
            BenchmarkArtifact { label: "raw_jsonl", path: &jsonl_path },
            BenchmarkArtifact { label: "summary_csv", path: &csv_path },
        ],
    );

    if args.verbose {
        print_verbose_rows(&output_rows)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_mode_defaults_to_virtual() {
        let args = Args::try_parse_from(["paper_sustained_hybrid"]).expect("args should parse");
        assert_eq!(args.time_mode, TimeMode::Virtual);
    }

    #[test]
    fn time_mode_parses_wall_clock() {
        let args = Args::try_parse_from(["paper_sustained_hybrid", "--time-mode", "wall-clock"])
            .expect("args should parse");
        assert_eq!(args.time_mode, TimeMode::WallClock);
    }
}
