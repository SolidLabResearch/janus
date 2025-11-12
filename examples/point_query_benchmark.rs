use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() -> std::io::Result<()> {
    println!("\nPoint Query Performance Benchmark");
    println!("=====================================");
    println!("Testing 100K and 1M datasets with 33 runs each\n");

    // Test sizes: 100K and 1M quads
    let test_sizes = vec![100_000, 1_000_000];
    let num_runs = 33;
    let warmup_runs = 3;
    let outlier_runs = 2;

    for &size in &test_sizes {
        println!(
            "Testing Point Queries for {} RDF Quads ({} runs, using middle 30)",
            format_number(size),
            num_runs
        );
        println!("{}", "=".repeat(80));

        let mut point_query_times = Vec::new();

        // Run benchmark multiple times
        for run in 1..=num_runs {
            if run % 10 == 0 || run == 1 {
                println!("   Run {}/{}...", run, num_runs);
            }

            let point_time = run_point_query_benchmark(size, run)?;
            point_query_times.push(point_time);
        }

        // Analyze results (middle 30 runs: exclude first 3 and last 2)
        let start_idx = warmup_runs;
        let end_idx = num_runs - outlier_runs;
        let analysis_times = &point_query_times[start_idx..end_idx];

        println!(
            "\nPoint Query Results (Middle 30 runs, excluding first {} and last {} runs)",
            warmup_runs, outlier_runs
        );
        println!("{}", "-".repeat(80));

        analyze_and_print_point_query("Point Query Latency", analysis_times);
        println!();
    }

    println!("Point Query Benchmark Complete!\n");
    Ok(())
}

fn run_point_query_benchmark(size: u64, run_id: usize) -> std::io::Result<f64> {
    // Create storage
    let config = StreamingConfig {
        max_batch_events: 10000,
        max_batch_bytes: 10 * 1024 * 1024,
        max_batch_age_seconds: 5,
        sparse_interval: 1000,
        entries_per_index_block: 1000,
        segment_base_path: format!("data/point_query_benchmark_{}_{}", size, run_id),
    };

    let mut storage = StreamingSegmentedStorage::new(config.clone())?;
    storage.start_background_flushing();

    let base_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    let mut min_timestamp = u64::MAX;
    let mut max_timestamp = 0u64;

    // Generate RDF quads with unique timestamps
    for i in 0..size {
        let timestamp = base_timestamp + i;
        min_timestamp = min_timestamp.min(timestamp);
        max_timestamp = max_timestamp.max(timestamp);

        storage.write_rdf(
            timestamp,
            &format!("subject{}", i),
            "predicate",
            &format!("object{}", i),
            "graph",
        )?;
    }

    // Wait for all data to be flushed to disk
    storage.shutdown()?;

    // Restart storage for read-only point query
    let storage = StreamingSegmentedStorage::new(config.clone())?;

    // Point query benchmark - query for middle timestamp
    let target_timestamp = min_timestamp + (max_timestamp - min_timestamp) / 2;

    let point_start = Instant::now();
    let _point_results = storage.query_rdf(target_timestamp, target_timestamp)?;
    let point_duration = point_start.elapsed();

    // Convert to milliseconds with microsecond precision
    let point_time_ms = (point_duration.as_micros() as f64) / 1000.0;

    // Cleanup
    let _ = std::fs::remove_dir_all(&config.segment_base_path);

    Ok(point_time_ms)
}

fn analyze_and_print_point_query(label: &str, times: &[f64]) {
    let mut sorted_times = times.to_vec();
    sorted_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let median = sorted_times[times.len() / 2];
    let min = *sorted_times.first().unwrap();
    let max = *sorted_times.last().unwrap();

    // Calculate standard deviation
    let variance = times.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / times.len() as f64;
    let std_dev = variance.sqrt();

    println!(
        "   {:<20}: {:.2} ms (median: {:.2}, std: {:.2}, range: {:.2} - {:.2})",
        label, mean, median, std_dev, min, max
    );

    // Additional statistics
    println!("   Sample Size         : {} measurements", times.len());
    println!("   Coefficient of Var  : {:.1}%", (std_dev / mean) * 100.0);

    // Percentiles
    let p95_idx = (times.len() as f64 * 0.95) as usize;
    let p99_idx = (times.len() as f64 * 0.99) as usize;
    println!("   95th Percentile     : {:.2} ms", sorted_times[p95_idx.min(times.len() - 1)]);
    println!("   99th Percentile     : {:.2} ms", sorted_times[p99_idx.min(times.len() - 1)]);
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
