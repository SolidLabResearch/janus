use clap::Parser;
use janus::paper_bench::cli_output::{
    default_benchmark_output_dir, print_benchmark_stdout, print_verbose_rows, BenchmarkArtifact,
};
use janus::paper_bench::historical_range::{
    run_historical_range_comparison, HistoricalRangeComparisonConfig, HistoricalRangeMode,
    HistoricalRangeQueryCase, HistoricalRangeQueryCaseArg, DEFAULT_DATASET_SIZES,
    DEFAULT_FIXED_RANGE_SECONDS, FIXED_60S_PLOT_FILE, FULL_HISTORY_PLOT_FILE, RESULT_MARKDOWN_FILE,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value_t = 3)]
    runs: usize,
    #[arg(long, default_value_t = 1)]
    warmup_runs: usize,
    #[arg(long, value_delimiter = ',', default_values_t = DEFAULT_DATASET_SIZES.to_vec())]
    dataset_sizes: Vec<usize>,
    #[arg(
        long,
        value_delimiter = ',',
        value_enum,
        default_values_t = vec![
            HistoricalRangeQueryCaseArg::Fixed60s,
            HistoricalRangeQueryCaseArg::FullHistory
        ]
    )]
    query_cases: Vec<HistoricalRangeQueryCaseArg>,
    #[arg(long, default_value_t = DEFAULT_FIXED_RANGE_SECONDS)]
    fixed_range_seconds: u64,
    #[arg(long, value_enum, default_value_t = HistoricalRangeMode::Warm)]
    mode: HistoricalRangeMode,
    #[arg(long, default_value_t = false)]
    debug_equivalence: bool,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let output_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| default_benchmark_output_dir("paper_historical_range_comparison"));
    let outcome = run_historical_range_comparison(&HistoricalRangeComparisonConfig {
        runs: args.runs,
        warmup_runs: args.warmup_runs,
        dataset_sizes: args.dataset_sizes,
        query_cases: args.query_cases.into_iter().map(HistoricalRangeQueryCase::from_arg).collect(),
        fixed_range_seconds: args.fixed_range_seconds,
        mode: args.mode,
        debug_equivalence: args.debug_equivalence,
        output_dir: output_dir.clone(),
    })?;

    let raw_jsonl_path = output_dir.join("paper_historical_range_comparison.raw.jsonl");
    let summary_csv_path = output_dir.join("paper_historical_range_comparison.summary.csv");
    let markdown_path = output_dir.join(RESULT_MARKDOWN_FILE);
    let fixed_plot_path = output_dir.join(FIXED_60S_PLOT_FILE);
    let full_plot_path = output_dir.join(FULL_HISTORY_PLOT_FILE);

    print_benchmark_stdout(
        "paper_historical_range_comparison",
        Some(outcome.run_outcomes.iter().all(|run| run.equivalent)),
        Some(args.warmup_runs),
        Some(args.runs),
        &output_dir,
        &[
            BenchmarkArtifact { label: "raw_jsonl", path: &raw_jsonl_path },
            BenchmarkArtifact { label: "summary_csv", path: &summary_csv_path },
            BenchmarkArtifact { label: "markdown", path: &markdown_path },
            BenchmarkArtifact { label: "fixed_plot", path: &fixed_plot_path },
            BenchmarkArtifact { label: "full_history_plot", path: &full_plot_path },
        ],
    );

    if args.verbose {
        print_verbose_rows(&outcome.run_outcomes)?;
    }

    Ok(())
}
