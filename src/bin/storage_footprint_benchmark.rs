use clap::{Parser, ValueEnum};
use janus::paper_bench::{
    cli_output::{
        default_benchmark_output_dir, print_benchmark_stdout, print_verbose_rows, BenchmarkArtifact,
    },
    storage_footprint::{
        run_storage_footprint_benchmark, StorageFootprintConfig, StorageSystemSelection,
    },
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SystemArg {
    Janus,
    Oxigraph,
    Both,
}

impl From<SystemArg> for StorageSystemSelection {
    fn from(value: SystemArg) -> Self {
        match value {
            SystemArg::Janus => Self::Janus,
            SystemArg::Oxigraph => Self::Oxigraph,
            SystemArg::Both => Self::Both,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "storage_footprint_benchmark")]
struct Args {
    #[arg(
        long,
        value_delimiter = ',',
        default_values_t = vec![10_000usize, 50_000, 100_000, 1_000_000]
    )]
    event_counts: Vec<usize>,

    #[arg(long, default_value_t = 1)]
    iterations: usize,

    #[arg(long)]
    output_dir: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    include_10m: bool,

    #[arg(long, default_value_t = false)]
    cleanup_runs_after_measurement: bool,

    #[arg(long, value_enum, default_value_t = SystemArg::Both)]
    system: SystemArg,

    #[arg(long)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let output_dir = args
        .output_dir
        .clone()
        .unwrap_or_else(|| default_benchmark_output_dir("storage_footprint_benchmark"));

    let outcome = run_storage_footprint_benchmark(&StorageFootprintConfig {
        event_counts: args.event_counts,
        iterations: args.iterations,
        output_dir: output_dir.clone(),
        include_10m: args.include_10m,
        cleanup_runs_after_measurement: args.cleanup_runs_after_measurement,
        system_selection: args.system.into(),
    })?;

    print_benchmark_stdout(
        "storage_footprint_benchmark",
        None,
        None,
        Some(args.iterations),
        &output_dir,
        &[
            BenchmarkArtifact { label: "raw_csv", path: &outcome.raw_csv_path },
            BenchmarkArtifact { label: "summary_csv", path: &outcome.summary_csv_path },
            BenchmarkArtifact { label: "ratio_csv", path: &outcome.ratio_csv_path },
            BenchmarkArtifact { label: "markdown", path: &outcome.markdown_path },
        ],
    );

    if args.verbose {
        print_verbose_rows(&outcome.raw_rows)?;
    }

    Ok(())
}
