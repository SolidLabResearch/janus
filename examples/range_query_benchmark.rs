use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::error::Error;
use std::time::Instant;

#[derive(Debug)]
struct BenchmarkResults {
    range_10_percent_times: Vec<f64>,
    range_50_percent_times: Vec<f64>,
    range_100_percent_times: Vec<f64>,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Realistic Range Query Benchmark by Range Size");
    println!("================================================");
    println!("Using realistic IoT sensor data (5 quads per observation)");
    println!("Testing range query performance for 10%, 50%, and 100% of time range");
    println!("Running 33 iterations, analyzing middle 30 runs\n");

    let predicates = vec![
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string(),
        "https://saref.etsi.org/core/isMeasuredByDevice".to_string(),
        "https://saref.etsi.org/core/relatesToProperty".to_string(),
        "http://purl.org/dc/terms/created".to_string(),
        "https://saref.etsi.org/core/hasValue".to_string(),
    ];

    // Test specific quad counts (each observation generates 5 quads)
    let target_quad_counts = vec![10, 100, 1_000, 10_000, 100_000, 1_000_000];
    let num_runs = 33;
    let warmup_runs = 3;
    let outlier_runs = 2;

    for &quad_count in &target_quad_counts {
        // Calculate observations needed (each generates 5 quads)
        let observations_needed = (quad_count + 4) / 5; // Round up division
        let actual_quads = observations_needed * 5;

        println!(
            "Testing {} target quads ({} observations → {} actual quads)",
            quad_count, observations_needed, actual_quads
        );
        println!("{}", "-".repeat(70));

        let mut all_results = Vec::new();

        // Run benchmark multiple times
        for run in 1..=num_runs {
            if run % 10 == 0 || run == 1 {
                println!("   Run {}/{}...", run, num_runs);
            }

            let result = run_range_query_benchmark(observations_needed, &predicates, run)?;
            all_results.push(result);
        }

        // Analyze results (middle 30 runs: exclude first 3 and last 2)
        let start_idx = warmup_runs;
        let end_idx = num_runs - outlier_runs;
        let analysis_results = &all_results[start_idx..end_idx];

        println!(
            "\nRange Query Results (Middle 30 runs, excluding first {} and last {} runs)",
            warmup_runs, outlier_runs
        );
        println!("{}", "-".repeat(80));

        // 10% range performance
        let range_10_times: Vec<f64> =
            analysis_results.iter().map(|r| r.range_10_percent_times[0]).collect();
        analyze_and_print(&format!("10% Range Query ({} quads)", actual_quads), &range_10_times, "ms");

        // 50% range performance
        let range_50_times: Vec<f64> =
            analysis_results.iter().map(|r| r.range_50_percent_times[0]).collect();
        analyze_and_print(&format!("50% Range Query ({} quads)", actual_quads), &range_50_times, "ms");

        // 100% range performance
        let range_100_times: Vec<f64> =
            analysis_results.iter().map(|r| r.range_100_percent_times[0]).collect();
        analyze_and_print(&format!("100% Range Query ({} quads)", actual_quads), &range_100_times, "ms");

        println!();
    }

    println!("Realistic Range Query Benchmark Complete!");
    Ok(())
}

fn run_range_query_benchmark(
    observations: usize,
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
        segment_base_path: format!("data/range_query_benchmark_{}_{}", observations, run_id),
    };

    let mut storage = StreamingSegmentedStorage::new(config.clone())?;
    storage.start_background_flushing();

    let base_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut min_timestamp = u64::MAX;
    let mut max_timestamp = 0u64;

    // Generate realistic RDF quads - each observation has 5 quads (same as main benchmark)
    for i in 0..observations {
        // Each observation has a unique timestamp (1ms apart)
        let timestamp = base_timestamp + i as u64;
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
            "https://dahcc.idlab.ugent.be/Homelab/SensorsAndActuators/70:ee:50:67:30:{}",
            format!("{:02x}", (i % 256) as u8)
        );

        // Property type - rotating through different measurement types
        let properties = vec![
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

    // Ensure all data is written before querying
    storage.shutdown()?;

    // Recreate storage for clean read-only access
    let storage = StreamingSegmentedStorage::new(config.clone())?;

    let time_range = max_timestamp - min_timestamp;

    // Debug: Print timestamp range
    if observations == 2000 {
        println!("DEBUG 10K: min_timestamp={}, max_timestamp={}, time_range={}", 
                 min_timestamp, max_timestamp, time_range);
    }

    // 10% range query - query 10% of the total time range
    let range_10_start = min_timestamp;
    let range_10_end = min_timestamp + (time_range / 10);
    let range_10_start_time = Instant::now();
    let range_10_results = storage.query_rdf(range_10_start, range_10_end)?;
    let range_10_duration = range_10_start_time.elapsed();
    let range_10_time_ms = (range_10_duration.as_micros() as f64) / 1000.0;

    // 50% range query - query 50% of the total time range
    let range_50_start = min_timestamp;
    let range_50_end = min_timestamp + (time_range / 2);
    let range_50_start_time = Instant::now();
    let range_50_results = storage.query_rdf(range_50_start, range_50_end)?;
    let range_50_duration = range_50_start_time.elapsed();
    let range_50_time_ms = (range_50_duration.as_micros() as f64) / 1000.0;

    // 100% range query - query entire time range
    let range_100_start = min_timestamp;
    let range_100_end = max_timestamp;
    
    // Debug: Print query parameters
    if observations == 2000 {
        println!("DEBUG 10K: 100% query from {} to {}", range_100_start, range_100_end);
    }
    
    let range_100_start_time = Instant::now();
    let range_100_results = storage.query_rdf(range_100_start, range_100_end)?;
    let range_100_duration = range_100_start_time.elapsed();
    let range_100_time_ms = (range_100_duration.as_micros() as f64) / 1000.0;

    let actual_quads = observations * 5;

    // Debug: Print result counts
    println!("DEBUG: Dataset {} quads - 10% range returned {} results, 50% range returned {} results, 100% range returned {} results",
             actual_quads, range_10_results.len(), range_50_results.len(), range_100_results.len());

    // Cleanup
    let _ = std::fs::remove_dir_all(&config.segment_base_path);

    Ok(BenchmarkResults {
        range_10_percent_times: vec![range_10_time_ms],
        range_50_percent_times: vec![range_50_time_ms],
        range_100_percent_times: vec![range_100_time_ms],
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

    // Calculate percentiles
    let p25 = sorted_times[times.len() / 4];
    let p75 = sorted_times[(times.len() * 3) / 4];

    println!(
        "{}: {:.2} ± {:.2} {} (median: {:.2}, range: {:.2}-{:.2}, p25: {:.2}, p75: {:.2})",
        label, mean, std_dev, unit, median, min, max, p25, p75
    );
}