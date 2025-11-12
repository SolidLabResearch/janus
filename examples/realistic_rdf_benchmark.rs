use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct BenchmarkResults {
    write_times: Vec<f64>,
    read_times_1_percent: Vec<f64>,
    read_times_10_percent: Vec<f64>,
    read_times_50_percent: Vec<f64>,
    read_times_100_percent: Vec<f64>,
    point_query_times: Vec<f64>,
}

fn main() -> std::io::Result<()> {
    println!("\nRealistic RDF Benchmark - IoT Sensor Observations");
    println!("=====================================================");
    println!("Running 33 iterations per test size, using middle 30 for statistics\n");

    // Test sizes: 10, 100, 1k, 10k, 100k, 1M
    let test_sizes = vec![10, 100, 1_000, 10_000, 100_000, 1_000_000];
    let num_runs = 33;
    let warmup_runs = 3;
    let outlier_runs = 2;

    // Define realistic RDF predicates for sensor observations
    let predicates: Vec<String> = vec![
        "http://rdfs.org/ns/void#inDataset".to_string(),
        "https://saref.etsi.org/core/measurementMadeBy".to_string(),
        "https://saref.etsi.org/core/relatesToProperty".to_string(),
        "https://saref.etsi.org/core/hasTimestamp".to_string(),
        "https://saref.etsi.org/core/hasValue".to_string(),
    ];

    for &size in &test_sizes {
        println!("\n{}", "=".repeat(80));
        println!(
            "Testing with {} RDF Quads ({} runs, using middle 30)",
            format_number(size),
            num_runs
        );
        println!("{}\n", "=".repeat(80));

        let mut all_results = Vec::new();

        // Run benchmark multiple times
        for run in 1..=num_runs {
            if run % 10 == 0 || run == 1 {
                println!("   Run {}/{}...", run, num_runs);
            }

            let result = run_single_benchmark(size, &predicates, run)?;
            all_results.push(result);
        }

        // Analyze results (middle 30 runs: exclude first 3 and last 2)
        let start_idx = warmup_runs;
        let end_idx = num_runs - outlier_runs;
        let analysis_results = &all_results[start_idx..end_idx];

        println!(
            "\nResults (Middle 30 runs, excluding first {} and last {} runs)",
            warmup_runs, outlier_runs
        );
        println!("{}", "-".repeat(80));

        // Write performance
        let write_times: Vec<f64> = analysis_results.iter().map(|r| r.write_times[0]).collect();
        analyze_and_print("Write Throughput", &write_times, "quads/sec");

        // Read performance for different ranges
        let read_1_times: Vec<f64> =
            analysis_results.iter().map(|r| r.read_times_1_percent[0]).collect();
        let read_10_times: Vec<f64> =
            analysis_results.iter().map(|r| r.read_times_10_percent[0]).collect();
        let read_50_times: Vec<f64> =
            analysis_results.iter().map(|r| r.read_times_50_percent[0]).collect();
        let read_100_times: Vec<f64> =
            analysis_results.iter().map(|r| r.read_times_100_percent[0]).collect();

        analyze_and_print("Read (1% range)", &read_1_times, "quads/sec");
        analyze_and_print("Read (10% range)", &read_10_times, "quads/sec");
        analyze_and_print("Read (50% range)", &read_50_times, "quads/sec");
        analyze_and_print("Read (100% range)", &read_100_times, "quads/sec");

        // Point query performance
        let point_times: Vec<f64> =
            analysis_results.iter().map(|r| r.point_query_times[0]).collect();
        analyze_and_print("Point Query", &point_times, "ms");
    }

    println!("\n{}", "=".repeat(80));
    println!("Benchmark Complete!");
    println!("{}\n", "=".repeat(80));

    Ok(())
}

fn run_single_benchmark(
    size: u64,
    predicates: &[String],
    run_id: usize,
) -> std::io::Result<BenchmarkResults> {
    // Create storage
    let config = StreamingConfig {
        max_batch_events: 10000,
        max_batch_bytes: 10 * 1024 * 1024,
        max_batch_age_seconds: 5,
        sparse_interval: 1000,
        entries_per_index_block: 1000,
        segment_base_path: format!("data/realistic_benchmark_{}_{}", size, run_id),
    };

    let mut storage = StreamingSegmentedStorage::new(config.clone())?;
    storage.start_background_flushing();

    let base_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

    let write_start = Instant::now();
    let mut min_timestamp = u64::MAX;
    let mut max_timestamp = 0u64;

    // Generate realistic RDF quads - each with unique subject and timestamp
    for i in 0..size {
        // Each observation has a unique timestamp (1ms apart)
        let timestamp = base_timestamp + i;
        min_timestamp = min_timestamp.min(timestamp);
        max_timestamp = max_timestamp.max(timestamp);

        // Create a unique subject for each observation
        // Format: https://dahcc.idlab.ugent.be/Protego/_participant{participant}/obs{i}
        let subject = format!(
            "https://dahcc.idlab.ugent.be/Protego/_participant{}/obs{}",
            (i % 100) + 1, // Rotate through 100 participants
            i
        );

        // Sensor data - rotating through different sensors
        let sensor = format!(
            "https://dahcc.idlab.ugent.be/Homelab/SensorsAndActuators/70:ee:50:67:30:{:02x}",
            (i % 256) as u8
        );

        // Property type - rotating through different measurement types
        let properties = [
            "org.dyamand.types.common.AtmosphericPressure",
            "org.dyamand.types.common.Temperature",
            "org.dyamand.types.common.Humidity",
            "org.dyamand.types.common.LightLevel",
        ];
        let property = format!(
            "https://dahcc.idlab.ugent.be/Homelab/SensorsAndActuators/{}",
            properties[(i % 4) as usize]
        );

        // Dataset
        let dataset = format!("https://dahcc.idlab.ugent.be/Protego/_participant{}", (i % 100) + 1);

        // Create 5 quads per observation (matching your example)
        let quads = vec![
            (
                subject.clone(),
                predicates[0].clone(),
                dataset,
                "http://example.org/graph1".to_string(),
            ),
            (
                subject.clone(),
                predicates[1].clone(),
                sensor,
                "http://example.org/graph1".to_string(),
            ),
            (
                subject.clone(),
                predicates[2].clone(),
                property,
                "http://example.org/graph1".to_string(),
            ),
            (
                subject.clone(),
                predicates[3].clone(),
                format!("2022-01-03T09:04:{:02}.000000", (i % 60) as u32),
                "http://example.org/graph1".to_string(),
            ),
            (
                subject.clone(),
                predicates[4].clone(),
                format!("{:.1}", 1000.0 + (i as f64 * 0.1) % 100.0),
                "http://example.org/graph1".to_string(),
            ),
        ];

        // Write all 5 quads for this observation
        for (s, p, o, g) in quads {
            storage.write_rdf(timestamp, &s, &p, &o, &g)?;
        }
    }

    let write_duration = write_start.elapsed();
    let write_throughput = (size * 5) as f64 / write_duration.as_secs_f64();

    // Wait for all data to be flushed to disk before read benchmarks
    println!("   Waiting for background flush to complete...");
    storage.shutdown()?;

    // Restart storage for read-only benchmarks
    let mut storage = StreamingSegmentedStorage::new(config.clone())?;

    // Read benchmarks
    let mut read_times_1_percent = Vec::new();
    let mut read_times_10_percent = Vec::new();
    let mut read_times_50_percent = Vec::new();
    let mut read_times_100_percent = Vec::new();

    // Test different query ranges
    let query_percentages = vec![0.01, 0.1, 0.5, 1.0];

    for &percentage in &query_percentages {
        let range_size = ((max_timestamp - min_timestamp) as f64 * percentage) as u64;
        let query_start_ts = min_timestamp;
        let query_end_ts = min_timestamp + range_size;

        let read_start = Instant::now();
        let results = storage.query_rdf(query_start_ts, query_end_ts)?;
        let read_duration = read_start.elapsed();

        // Use microseconds for better precision, avoid division by zero
        let duration_secs = read_duration.as_secs_f64().max(0.000001); // At least 1 microsecond
        let read_throughput = results.len() as f64 / duration_secs;

        match percentage {
            0.01 => read_times_1_percent.push(read_throughput),
            0.1 => read_times_10_percent.push(read_throughput),
            0.5 => read_times_50_percent.push(read_throughput),
            1.0 => read_times_100_percent.push(read_throughput),
            _ => {}
        }
    }

    // Point query benchmark - query for a specific observation (should return 5 quads)
    // Query for the very first timestamp we wrote (we know it exists)
    let single_ts = min_timestamp; // This is base_timestamp + 0

    let point_start = Instant::now();
    let point_results = storage.query_rdf(single_ts, single_ts)?;
    let point_duration = point_start.elapsed();
    // Use microseconds for sub-millisecond precision
    let point_time_us = point_duration.as_micros() as f64;
    let point_time_ms = point_time_us / 1000.0;

    // Debug: show results count for small datasets
    if size <= 10_000 {
        eprintln!("   DEBUG: Point query at ts={} (min_ts, size={}) returned {} quads (duration: {:.3} µs = {:.3} ms)",
                  single_ts, size, point_results.len(), point_time_us, point_time_ms);
    }

    // Cleanup
    storage.shutdown()?;
    let _ = std::fs::remove_dir_all(&config.segment_base_path);

    Ok(BenchmarkResults {
        write_times: vec![write_throughput],
        read_times_1_percent,
        read_times_10_percent,
        read_times_50_percent,
        read_times_100_percent,
        point_query_times: vec![point_time_ms],
    })
}

fn analyze_and_print(label: &str, times: &[f64], unit: &str) {
    let mut sorted_times = times.to_vec();
    sorted_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let median = sorted_times[times.len() / 2];
    let min = *sorted_times.first().unwrap();
    let max = *sorted_times.last().unwrap();

    // Calculate standard deviation
    let variance = times.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / times.len() as f64;
    let std_dev = variance.sqrt();

    // Use higher precision for millisecond times (point queries)
    if unit == "ms" {
        println!(
            "   {:<20}: {:.3} {} (median: {:.3}, std: {:.3}, range: {:.3} - {:.3})",
            label, mean, unit, median, std_dev, min, max
        );
    } else {
        println!(
            "   {:<20}: {:.0} {} (median: {:.0}, std: {:.0}, range: {:.0} - {:.0})",
            label, mean, unit, median, std_dev, min, max
        );
    }
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
