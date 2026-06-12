use clap::Parser;
use janus::paper_bench::cli_output::{
    default_benchmark_output_dir, print_benchmark_stdout, print_verbose_rows, BenchmarkArtifact,
};
use janus::paper_bench::{
    external::OxigraphExternalAdapter,
    harness::{
        collect_repro_metadata, ensure_output_dir, prepare_coordination_workload,
        run_coordination_pair, summarize_coordination, write_coordination_summary_csv, write_jsonl,
        CoordinationRow, CoordinationRunConfig, ExecutionMode,
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
    #[arg(long, default_value_t = 32)]
    live_events: usize,
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
        .unwrap_or_else(|| default_benchmark_output_dir("paper_hybrid_coordination"));
    ensure_output_dir(&output_dir)?;

    let metadata = collect_repro_metadata();
    let adapter = OxigraphExternalAdapter::new();
    let warm_workload = if args.mode == ExecutionMode::Warm {
        Some(prepare_coordination_workload(args.historical_events, args.live_events)?)
    } else {
        None
    };

    let mut all_rows = Vec::<CoordinationRow>::with_capacity((args.warmup_runs + args.runs) * 2);

    for run_index in 0..args.warmup_runs {
        let pair = run_coordination_pair(CoordinationRunConfig {
            mode: args.mode,
            run_index,
            is_warmup: true,
            historical_events: args.historical_events,
            live_events: args.live_events,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: warm_workload.as_ref(),
            debug_output_dir: args.debug_equivalence.then_some(output_dir.as_path()),
        })?;
        all_rows.push(pair.unified);
        all_rows.push(pair.decomposed);
    }

    for run_index in 0..args.runs {
        let pair = run_coordination_pair(CoordinationRunConfig {
            mode: args.mode,
            run_index,
            is_warmup: false,
            historical_events: args.historical_events,
            live_events: args.live_events,
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

    let jsonl_path = output_dir.join("paper_hybrid_coordination.raw.jsonl");
    let csv_path = output_dir.join("paper_hybrid_coordination.summary.csv");
    write_jsonl(&jsonl_path, &output_rows)?;
    write_coordination_summary_csv(&csv_path, &summarize_coordination(&output_rows))?;

    print_benchmark_stdout(
        "paper_hybrid_coordination",
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
