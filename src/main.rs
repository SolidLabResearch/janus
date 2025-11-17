//! Janus - A hybrid engine for unified Live and Historical RDF Stream Processing
//!
//! This is the main entry point for the Janus command-line interface.

use janus::core::Event;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SEGMENT_BASE_PATH: &str = "data/rdf_benchmark";

fn benchmark_segmented_storage_rdf() -> std::io::Result<()> {
    // println!("RDF Segmented Storage Benchmark");
    // println!("==================================");

    // Clean up and create directories
    let _ = fs::remove_dir_all(SEGMENT_BASE_PATH);
    fs::create_dir_all(SEGMENT_BASE_PATH)?;

    // Configure storage
    let config = StreamingConfig {
        max_batch_events: 500_000,
        max_batch_age_seconds: 1,
        max_batch_bytes: 50_000_000,
        sparse_interval: 1000,
        entries_per_index_block: 100,
        segment_base_path: SEGMENT_BASE_PATH.to_string(),
    };

    let mut storage = StreamingSegmentedStorage::new(config)?;
    storage.start_background_flushing();

    // Record initial memory
    // storage.record_memory("before_writing");

    // Benchmark writing 1 million RDF events
    // println!("\nWriting 1,000,000 RDF events...");
    let start_time = Instant::now();
    let base_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

    for i in 0..1_000_000u64 {
        let timestamp = base_timestamp + i; // 1ms intervals
        let subject = format!("http://example.org/person/person_{}", i % 10000);
        let predicate = match i % 10 {
            0..=3 => "http://example.org/knows",
            4..=6 => "http://example.org/worksAt",
            7..=8 => "http://example.org/livesIn",
            _ => "http://example.org/hasAge",
        };
        let object = match i % 10 {
            0..=3 => format!("http://example.org/person/person_{}", (i + 1) % 10000),
            4..=6 => format!("http://example.org/organization/org_{}", i % 1000),
            7..=8 => format!("http://example.org/location/city_{}", i % 100),
            _ => format!("\"{}\"^^http://www.w3.org/2001/XMLSchema#integer", 20 + (i % 60)),
        };
        let graph = format!("http://example.org/graph/graph_{}", i % 100);

        storage.write_rdf(timestamp, &subject, predicate, &object, &graph)?;

        if i > 0 && i % 100_000 == 0 {
            // println!("  ✓ Written {} events", i);
            // storage.record_memory(&format!("after_{}_events", i));
        }
    }

    let write_duration = start_time.elapsed();
    let _write_throughput = 1_000_000.0 / write_duration.as_secs_f64();

    // println!("\nWrite completed!");
    // println!("   Duration: {:.3} seconds", write_duration.as_secs_f64());
    // println!("   Throughput: {:.0} events/sec", write_throughput);

    // Wait a bit for background flushing
    std::thread::sleep(Duration::from_secs(2));
    // storage.record_memory("after_background_flush");

    // Benchmark reading different amounts of data
    // println!("\nReading Benchmarks");
    // println!("====================");

    let read_sizes = vec![100, 1_000, 10_000, 100_000, 1_000_000];

    for &size in &read_sizes {
        // Query the first 'size' events
        let query_start_ts = base_timestamp;
        let query_end_ts = base_timestamp + size as u64;

        // println!("\n📖 Querying {} events...", size);
        let start_time = Instant::now();

        let results = storage.query_rdf(query_start_ts, query_end_ts)?;

        let query_duration = start_time.elapsed();
        let _read_throughput = results.len() as f64 / query_duration.as_secs_f64();

        // println!("   Results found: {}", results.len());
        // println!("   Query time: {:.3} ms", query_duration.as_millis());
        // println!("   Read throughput: {:.0} events/sec", read_throughput);

        // Show a sample result for verification
        if !results.is_empty() {
            let sample = &results[0];
            println!(
                "   Sample result: {} {} {} in {} at {}",
                sample.subject, sample.predicate, sample.object, sample.graph, sample.timestamp
            );
        }
    }

    // Shutdown storage
    storage.shutdown()?;

    // Print memory statistics
    // println!("\nMemory Usage Statistics");
    // println!("==========================");
    // let memory_stats = storage.get_memory_stats();
    // // println!("Peak memory: {}", MemoryTracker::format_bytes(memory_stats.peak_bytes));
    // // println!("Current memory: {}", MemoryTracker::format_bytes(memory_stats.current_bytes));
    // println!(
    //     "Average memory: {}",
    //     MemoryTracker::format_bytes(memory_stats.avg_bytes as usize)
    // );
    // // println!("Total measurements: {}", memory_stats.total_measurements);

    // Print storage component breakdown
    // let component_sizes = storage.get_storage_component_sizes();
    // println!("\n🧩 Storage Component Breakdown");
    // println!("=============================");
    // println!(
    //     "Batch buffer: {}",
    //     MemoryTracker::format_bytes(component_sizes.batch_buffer_bytes)
    // );
    // // println!("Dictionary: {}", MemoryTracker::format_bytes(component_sizes.dictionary_bytes));
    // // println!("Segments count: {}", component_sizes.segments_count);
    // println!(
    //     "Estimated total: {}",
    //     MemoryTracker::format_bytes(component_sizes.estimated_total_bytes)
    // );

    // if memory_stats.measurements.len() > 1 {
    //     // println!("\nDetailed measurements:");
    //     for measurement in &memory_stats.measurements {
    //         println!(
    //             "  {}: {}",
    //             measurement.description,
    //             MemoryTracker::format_bytes(measurement.memory_bytes)
    //         );
    //     }
    // }

    // println!("\nBenchmark completed successfully!");
    Ok(())
}

fn benchmark_storage_performance() -> std::io::Result<()> {
    // println!("=== WAL-Based Segmented Storage Performance Benchmark ===\n");

    let record_counts = vec![100, 1000, 10000, 100000, 1000000];

    for &num_records in &record_counts {
        // println!("Testing with {} records", num_records);
        // println!("──────────────────────────────────────────────────");

        // Configure storage
        let config = StreamingConfig {
            max_batch_events: 250_000,
            max_batch_age_seconds: 1,
            max_batch_bytes: 100 * 1024 * 1024,
            sparse_interval: 100,
            entries_per_index_block: 512,
            segment_base_path: format!("./benchmark_data_{}", num_records),
        };

        // Clean up any existing data
        let _ = std::fs::remove_dir_all(&config.segment_base_path);

        let mut storage = StreamingSegmentedStorage::new(config.clone())?;
        storage.start_background_flushing();

        // Benchmark writes
        // println!("Writing {} records...", num_records);
        let write_start = Instant::now();
        let mut min_timestamp = u64::MAX;
        let mut max_timestamp = 0u64;

        for i in 0..num_records {
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 + i;
            min_timestamp = min_timestamp.min(timestamp);
            max_timestamp = max_timestamp.max(timestamp);

            let event = Event {
                timestamp,
                subject: (i % 10) as u32,
                predicate: 1,
                object: (20 + (i % 10)) as u32,
                graph: 1,
            };
            storage.write(event)?;
        }

        let write_duration = write_start.elapsed();
        let _write_throughput = num_records as f64 / write_duration.as_secs_f64();

        // println!("Write Performance:");
        // println!("  Duration: {:.3}s", write_duration.as_secs_f64());
        // println!("  Throughput: {:.0} records/sec", write_throughput);
        // println!("  Timestamp range: {} to {}", min_timestamp, max_timestamp);

        // Benchmark queries immediately after writing (data is still in WAL)
        let query_ranges = vec![(0.1, "10% of data"), (0.5, "50% of data"), (1.0, "100% of data")];

        // println!("\nQuery Performance:");

        for (fraction, _description) in query_ranges {
            let query_count = 100.min(num_records / 10); // Run 100 queries or 10% of records, whichever is smaller
            let mut query_times = Vec::new();
            let mut total_records_read = 0;

            for q in 0..query_count {
                // Use a deterministic but varied offset for queries within the actual data range
                let timestamp_range = max_timestamp - min_timestamp;
                let start_offset =
                    (timestamp_range as f64 * fraction * (q as f64 / query_count as f64)) as u64;
                let query_window = (timestamp_range as f64 * 0.01).max(100.0) as u64; // 1% of data or 100 records minimum

                let start_timestamp = min_timestamp + start_offset;
                let end_timestamp = (start_timestamp + query_window).min(max_timestamp);

                let query_start = Instant::now();
                let results = storage.query(start_timestamp, end_timestamp)?;
                let query_duration = query_start.elapsed();

                total_records_read += results.len();
                query_times.push(query_duration.as_secs_f64());
            }

            let avg_query_time = query_times.iter().sum::<f64>() / query_times.len() as f64;
            let _queries_per_sec = 1.0 / avg_query_time;
            let total_query_time = query_times.iter().sum::<f64>();
            let _records_per_sec = if total_query_time > 0.0 {
                total_records_read as f64 / total_query_time
            } else {
                0.0
            };
            let _avg_records_per_query = total_records_read as f64 / query_count as f64;
            let _min_time = query_times.iter().cloned().fold(f64::INFINITY, f64::min);
            let _max_time = query_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

            // println!("  {} queries ({}):", description, query_count);
            // println!("    Avg query time: {:.3}ms", avg_query_time * 1000.0);
            // println!("    Query throughput: {:.1} queries/sec", queries_per_sec);
            // println!("    Read throughput: {:.0} records/sec", records_per_sec);
            // println!("    Avg records per query: {:.1}", avg_records_per_query);
            // println!("    Total records read: {}", total_records_read);
            // println!("    Min/Max time: {:.3}ms / {:.3}ms", min_time * 1000.0, max_time * 1000.0);
        }

        // Force flush remaining WAL data and shutdown
        storage.shutdown()?;
        println!();
    }

    // println!("Benchmark completed!");
    Ok(())
}

fn main() -> std::io::Result<()> {
    // Run the new RDF benchmark
    benchmark_segmented_storage_rdf()?;

    // println!("\n{}", "=".repeat(50));
    // println!("Running legacy benchmark for comparison...");
    // println!("{}", "=".repeat(50));

    // Also run the old benchmark for comparison
    benchmark_storage_performance()
}
