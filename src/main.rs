//! Janus - A hybrid engine for unified Live and Historical RDF Stream Processing
//!
//! This is the main entry point for the Janus command-line interface.

use janus::indexing::shared::Event;
use janus::indexing::{dense, shared::LogWriter, sparse};
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::fs;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DATA_DIR: &str = "data/benchmark";
const LOG_FILE: &str = "data/benchmark/log.dat";
const DENSE_INDEX_FILE: &str = "data/benchmark/dense.idx";
const SPARSE_INDEX_FILE: &str = "data/benchmark/sparse.idx";
const SPARSE_INTERVAL: usize = 1000;

fn setup_data(number_records: u64) -> std::io::Result<()> {
    let _ = fs::remove_dir_all(DATA_DIR);
    fs::create_dir_all(DATA_DIR)?;

    let mut writer = LogWriter::create(LOG_FILE)?;

    for i in 0..number_records {
        let timestamp = i;
        let subject = (i % 1000) as u64;
        let predicate = (i % 500) as u64;
        let object = (i % 2000) as u64;
        let graph: u64 = 1;
        writer.append_record(timestamp, subject, predicate, object, graph)?;
    }

    writer.flush()?;

    println!("Generated log file with {} records", writer.record_count());

    Ok(())
}

fn benchmark_indexing() -> std::io::Result<()> {
    println!("Indexing Benchmark");

    let start = Instant::now();
    dense::build_dense_index(LOG_FILE, DENSE_INDEX_FILE)?;
    let dense_time = start.elapsed();
    println!("Dense index build time: {:.3} ms", dense_time.as_secs_f64() * 1000.0);

    let start = Instant::now();
    sparse::build_sparse_index(LOG_FILE, SPARSE_INDEX_FILE, &SPARSE_INTERVAL)?;
    let sparse_time = start.elapsed();
    println!("Sparse index build time: {:.3} ms", sparse_time.as_secs_f64() * 1000.0);

    let dense_reader = dense::DenseIndexReader::open(DENSE_INDEX_FILE)?;
    let sparse_reader = sparse::SparseReader::open(SPARSE_INDEX_FILE, SPARSE_INTERVAL)?;
    println!(
        "\n Dense Index Size: {} MB",
        dense_reader.index_size_bytes() as f64 / 1_000_000.0
    );

    println!(
        "\n Sparse Index Size: {} MB",
        sparse_reader.index_size_bytes() as f64 / 1_000_000.0
    );
    Ok(())
}

fn benchmark_queries() -> std::io::Result<()> {
    println!("Query Benchmark");
    let dense_reader = dense::DenseIndexReader::open(DENSE_INDEX_FILE)?;
    let sparse_reader = sparse::SparseReader::open(SPARSE_INDEX_FILE, SPARSE_INTERVAL)?;

    let query_ranges = vec![
        (0u64, 100u64, "100 records"),
        (5000u64, 5100u64, "100 records (mid-range)"),
        (0u64, 10000u64, "10K records"),
        (0u64, 100000u64, "100K records"),
        (0u64, 1000000u64, "1M records"),
    ];

    for (timestamp_start, timestamp_end, description) in query_ranges {
        println!("\n Query: {} from {} to {}", description, timestamp_start, timestamp_end);

        let start = Instant::now();
        let dense_results = dense_reader.query(LOG_FILE, timestamp_start, timestamp_end)?;
        let dense_time = start.elapsed();

        let start = Instant::now();
        let sparse_results = sparse_reader.query(LOG_FILE, timestamp_start, timestamp_end)?;
        let sparse_time = start.elapsed();

        println!(
            " Dense Index Query Time: {:.3} ms, Results: {}",
            dense_time.as_secs_f64() * 1000.0,
            dense_results.len()
        );

        println!(
            " Sparse Index Query Time: {:.3} ms, Results: {}",
            sparse_time.as_secs_f64() * 1000.0,
            sparse_results.len()
        );

        let speedup = sparse_time.as_secs_f64() / dense_time.as_secs_f64();

        if speedup > 1.0 {
            println!(" Sparse index is {:.2} times faster than Dense index", speedup);
        } else {
            println!(" Dense index is {:.2} times faster than Sparse index", 1.0 / speedup);
        }

        assert_eq!(
            dense_results.len(),
            sparse_results.len(),
            "Mismatch in result counts between Dense and Sparse index queries"
        );
    }
    Ok(())
}

// fn main() -> std::io::Result<()> {
//     println!("RDF Indexing Benchmark : Dense vs Sparse");
//     println!("Setting up data...");
//     let number_of_records = 1_000_000u64;
//     setup_data(number_of_records)?;

//     benchmark_indexing()?;
//     benchmark_queries()?;

//     println!(
//         "\n=== Summary ===\nSparse interval: {}\nUse this data to decide \
//          which approach suits your use case best.",
//         SPARSE_INTERVAL
//     );
//     Ok(())
// }

fn benchmark_storage_performance() -> std::io::Result<()> {
    println!("=== WAL-Based Segmented Storage Performance Benchmark ===\n");

    let record_counts = vec![100, 1000, 10000, 100000, 1000000];

    for &num_records in &record_counts {
        println!("Testing with {} records", num_records);
        println!("──────────────────────────────────────────────────");

        // Configure storage
        let config = StreamingConfig {
            max_wal_events: 5000,
            max_wal_age_seconds: 30,
            max_wal_bytes: 5 * 1024 * 1024,
            sparse_interval: 100,
            entries_per_index_block: 512,
            segment_base_path: format!("./benchmark_data_{}", num_records),
            ..Default::default()
        };

        // Clean up any existing data
        let _ = std::fs::remove_dir_all(&config.segment_base_path);

        let mut storage = StreamingSegmentedStorage::new(config.clone())?;
        storage.start_background_flushing();

        // Benchmark writes
        println!("Writing {} records...", num_records);
        let write_start = Instant::now();
        let mut min_timestamp = u64::MAX;
        let mut max_timestamp = 0u64;

        for i in 0..num_records {
            let timestamp =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64 + i;
            min_timestamp = min_timestamp.min(timestamp);
            max_timestamp = max_timestamp.max(timestamp);

            let event = Event {
                timestamp,
                subject: (i % 10) as u64,
                predicate: 1,
                object: (20 + (i % 10)) as u64,
                graph: 1,
            };
            storage.write(event)?;
        }

        let write_duration = write_start.elapsed();
        let write_throughput = num_records as f64 / write_duration.as_secs_f64();

        println!("Write Performance:");
        println!("  Duration: {:.3}s", write_duration.as_secs_f64());
        println!("  Throughput: {:.0} records/sec", write_throughput);
        println!("  Timestamp range: {} to {}", min_timestamp, max_timestamp);

        // Benchmark queries immediately after writing (data is still in WAL)
        let query_ranges = vec![(0.1, "10% of data"), (0.5, "50% of data"), (1.0, "100% of data")];

        println!("\nQuery Performance:");

        for (fraction, description) in query_ranges {
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
            let queries_per_sec = 1.0 / avg_query_time;
            let total_query_time = query_times.iter().sum::<f64>();
            let records_per_sec = if total_query_time > 0.0 {
                total_records_read as f64 / total_query_time
            } else {
                0.0
            };
            let avg_records_per_query = total_records_read as f64 / query_count as f64;
            let min_time = query_times.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_time = query_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

            println!("  {} queries ({}):", description, query_count);
            println!("    Avg query time: {:.3}ms", avg_query_time * 1000.0);
            println!("    Query throughput: {:.1} queries/sec", queries_per_sec);
            println!("    Read throughput: {:.0} records/sec", records_per_sec);
            println!("    Avg records per query: {:.1}", avg_records_per_query);
            println!("    Total records read: {}", total_records_read);
            println!("    Min/Max time: {:.3}ms / {:.3}ms", min_time * 1000.0, max_time * 1000.0);
        }

        // Force flush remaining WAL data and shutdown
        storage.shutdown()?;
        println!();
    }

    println!("Benchmark completed!");
    Ok(())
}

fn main() -> std::io::Result<()> {
    benchmark_storage_performance()
}
