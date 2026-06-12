use clap::Parser;
use janus::paper_bench::cli_output::{
    default_benchmark_output_dir, print_benchmark_stdout, print_verbose_rows, BenchmarkArtifact,
};
use janus::paper_bench::harness::{
    collect_repro_metadata, ensure_output_dir, fill_scaling_percentiles,
    generate_citybench_dataset, run_scaling_query, summarize_scaling, summarize_scaling_fit,
    write_jsonl, write_scaling_fit_csv, write_scaling_summary_csv, ExecutionMode,
    HistoricalDataset, ScalingQueryType, ScalingRow, ScalingRunConfig,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value_t = 0)]
    warmup_runs: usize,
    #[arg(long, default_value_t = 5)]
    runs: usize,
    #[arg(long, default_value_t = false)]
    include_warmups: bool,
    #[arg(
        long,
        value_delimiter = ',',
        default_values_t = vec![100_000usize, 500_000, 1_000_000, 5_000_000]
    )]
    dataset_sizes: Vec<usize>,
    #[arg(
        long,
        value_delimiter = ',',
        value_enum,
        default_values_t = vec![
            ScalingQueryType::PointLookup,
            ScalingQueryType::FixedWindow,
            ScalingQueryType::ProportionalRange10,
            ScalingQueryType::ProportionalRange50,
            ScalingQueryType::FullRange,
            ScalingQueryType::HybridBaselineLookup,
        ]
    )]
    query_types: Vec<ScalingQueryType>,
    #[arg(long, value_enum, default_value_t = ExecutionMode::Warm)]
    mode: ExecutionMode,
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
        .unwrap_or_else(|| default_benchmark_output_dir("paper_historical_scaling"));
    ensure_output_dir(&output_dir)?;
    let metadata = collect_repro_metadata();
    let mut all_rows = Vec::<ScalingRow>::new();

    for &dataset_size in &args.dataset_sizes {
        let warm_dataset: Option<HistoricalDataset> = if args.mode == ExecutionMode::Warm {
            Some(generate_citybench_dataset(dataset_size, &output_dir)?)
        } else {
            None
        };

        for &query_type in &args.query_types {
            for run_index in 0..args.warmup_runs {
                all_rows.push(run_scaling_query(ScalingRunConfig {
                    mode: args.mode,
                    dataset_size_quads: dataset_size,
                    query_type,
                    metadata: &metadata,
                    run_index,
                    is_warmup: true,
                    warm_dataset: warm_dataset.as_ref(),
                    output_dir: &output_dir,
                })?);
            }
            for run_index in 0..args.runs {
                all_rows.push(run_scaling_query(ScalingRunConfig {
                    mode: args.mode,
                    dataset_size_quads: dataset_size,
                    query_type,
                    metadata: &metadata,
                    run_index,
                    is_warmup: false,
                    warm_dataset: warm_dataset.as_ref(),
                    output_dir: &output_dir,
                })?);
            }
        }
    }

    let mut output_rows = if args.include_warmups {
        all_rows.clone()
    } else {
        all_rows.iter().filter(|row| !row.is_warmup).cloned().collect::<Vec<_>>()
    };

    fill_scaling_percentiles(&mut output_rows);
    let jsonl_path = output_dir.join("paper_historical_scaling.raw.jsonl");
    let csv_path = output_dir.join("paper_historical_scaling.summary.csv");
    let fit_csv_path = output_dir.join("paper_historical_scaling.fit.csv");
    write_jsonl(&jsonl_path, &output_rows)?;
    write_scaling_summary_csv(&csv_path, &summarize_scaling(&output_rows))?;
    write_scaling_fit_csv(&fit_csv_path, &summarize_scaling_fit(&output_rows))?;

    print_benchmark_stdout(
        "paper_historical_scaling",
        None,
        Some(args.warmup_runs),
        Some(args.runs),
        &output_dir,
        &[
            BenchmarkArtifact { label: "raw_jsonl", path: &jsonl_path },
            BenchmarkArtifact { label: "summary_csv", path: &csv_path },
            BenchmarkArtifact { label: "fit_csv", path: &fit_csv_path },
        ],
    );

    if args.verbose {
        print_verbose_rows(&output_rows)?;
    }

    Ok(())
}
