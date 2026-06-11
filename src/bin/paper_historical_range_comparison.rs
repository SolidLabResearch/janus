use clap::Parser;
use janus::paper_bench::historical_range::{
    run_historical_range_comparison, HistoricalRangeComparisonConfig, HistoricalRangeMode,
    HistoricalRangeQueryCase, HistoricalRangeQueryCaseArg, DEFAULT_DATASET_SIZES,
    DEFAULT_FIXED_RANGE_SECONDS, FIXED_60S_PLOT_PATH, FULL_HISTORY_PLOT_PATH, RESULT_MARKDOWN_PATH,
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
    #[arg(long, default_value = "target/paper_benchmarks/paper_h2_range_comparison")]
    output_dir: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let outcome = run_historical_range_comparison(&HistoricalRangeComparisonConfig {
        runs: args.runs,
        warmup_runs: args.warmup_runs,
        dataset_sizes: args.dataset_sizes,
        query_cases: args.query_cases.into_iter().map(HistoricalRangeQueryCase::from_arg).collect(),
        fixed_range_seconds: args.fixed_range_seconds,
        mode: args.mode,
        debug_equivalence: args.debug_equivalence,
        output_dir: args.output_dir.clone(),
    })?;

    println!(
        "raw_jsonl={}",
        args.output_dir.join("paper_historical_range_comparison.raw.jsonl").display()
    );
    println!(
        "summary_csv={}",
        args.output_dir.join("paper_historical_range_comparison.summary.csv").display()
    );
    println!("markdown={RESULT_MARKDOWN_PATH}");
    println!("fixed_plot={FIXED_60S_PLOT_PATH}");
    println!("full_history_plot={FULL_HISTORY_PLOT_PATH}");
    println!("rows={}", outcome.raw_rows.len());
    println!("summary_rows={}", outcome.summary_rows.len());
    Ok(())
}
