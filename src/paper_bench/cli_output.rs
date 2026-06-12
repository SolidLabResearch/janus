use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct BenchmarkArtifact<'a> {
    pub label: &'a str,
    pub path: &'a Path,
}

pub fn default_benchmark_output_dir(benchmark: &str) -> PathBuf {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    PathBuf::from(format!("logs/benchmark/{benchmark}/{ts}"))
}

pub fn print_benchmark_stdout(
    benchmark: &str,
    correctness_passed: Option<bool>,
    warmup_runs: Option<usize>,
    measured_runs: Option<usize>,
    output_dir: &Path,
    artifacts: &[BenchmarkArtifact<'_>],
) {
    println!("benchmark={benchmark}");
    if let Some(correctness_passed) = correctness_passed {
        println!("correctness_passed={correctness_passed}");
    }
    if warmup_runs.is_some() || measured_runs.is_some() {
        println!(
            "warmup_runs={} measured_runs={}",
            warmup_runs.unwrap_or(0),
            measured_runs.unwrap_or(0)
        );
    }
    println!("output_dir={}", output_dir.display());
    for artifact in artifacts {
        println!("{}={}", artifact.label, artifact.path.display());
    }
}

pub fn print_verbose_rows<T: Serialize>(rows: &[T]) -> Result<(), serde_json::Error> {
    for row in rows {
        println!("{}", serde_json::to_string(row)?);
    }
    Ok(())
}
