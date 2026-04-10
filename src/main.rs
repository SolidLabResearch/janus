//! Janus package entry point.
//!
//! This binary exists to provide a clear top-level entry point when users run
//! `cargo run --bin janus`. The operational binaries remain:
//!
//! - `http_server` for the HTTP/WebSocket API
//! - `stream_bus_cli` for replay and ingestion

use clap::{Parser, Subcommand};
use janus::core::Event;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SEGMENT_BASE_PATH: &str = "data/rdf_benchmark";

#[derive(Parser)]
#[command(
    name = "janus",
    about = "Janus package entry point",
    long_about = "Use this binary for package-level help and internal storage benchmarks. For the backend API, run `http_server`. For RDF replay and ingestion, run `stream_bus_cli`."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show the primary Janus entry points.
    Info,
    /// Run the RDF segmented storage benchmark.
    BenchmarkStorageRdf,
    /// Run the event storage benchmark matrix.
    BenchmarkStorage,
}

fn print_overview() {
    println!("Janus package entry point");
    println!();
    println!("Primary binaries:");
    println!("  http_server     REST and WebSocket API");
    println!("  stream_bus_cli  RDF replay and ingestion CLI");
    println!();
    println!("Useful commands:");
    println!("  cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage");
    println!("  cargo run --bin stream_bus_cli -- --help");
    println!("  cargo run --example http_client_example");
    println!();
    println!("Benchmark subcommands:");
    println!("  cargo run --bin janus -- benchmark-storage-rdf");
    println!("  cargo run --bin janus -- benchmark-storage");
}

fn benchmark_segmented_storage_rdf() -> std::io::Result<()> {
    let _ = fs::remove_dir_all(SEGMENT_BASE_PATH);
    fs::create_dir_all(SEGMENT_BASE_PATH)?;

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

    let start_time = Instant::now();
    let base_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

    for i in 0..1_000_000u64 {
        let timestamp = base_timestamp + i;
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
    }

    let _write_duration = start_time.elapsed();

    std::thread::sleep(Duration::from_secs(2));

    let read_sizes = vec![100, 1_000, 10_000, 100_000, 1_000_000];

    for &size in &read_sizes {
        let query_start_ts = base_timestamp;
        let query_end_ts = base_timestamp + size as u64;
        let results = storage.query_rdf(query_start_ts, query_end_ts)?;

        if let Some(sample) = results.first() {
            println!(
                "Sample result: {} {} {} in {} at {}",
                sample.subject, sample.predicate, sample.object, sample.graph, sample.timestamp
            );
        }
    }

    storage.shutdown()?;
    Ok(())
}

fn benchmark_storage_performance() -> std::io::Result<()> {
    let record_counts = vec![100, 1000, 10000, 100000, 1000000];

    for &num_records in &record_counts {
        let config = StreamingConfig {
            max_batch_events: 250_000,
            max_batch_age_seconds: 1,
            max_batch_bytes: 100 * 1024 * 1024,
            sparse_interval: 100,
            entries_per_index_block: 512,
            segment_base_path: format!("./benchmark_data_{}", num_records),
        };

        let _ = std::fs::remove_dir_all(&config.segment_base_path);

        let mut storage = StreamingSegmentedStorage::new(config.clone())?;
        storage.start_background_flushing();

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

        let query_ranges = vec![(0.1, "10% of data"), (0.5, "50% of data"), (1.0, "100% of data")];

        for (fraction, _description) in query_ranges {
            let query_count = 100.min(num_records / 10);

            for q in 0..query_count {
                let timestamp_range = max_timestamp - min_timestamp;
                let start_offset =
                    (timestamp_range as f64 * fraction * (q as f64 / query_count as f64)) as u64;
                let query_window = (timestamp_range as f64 * 0.01).max(100.0) as u64;

                let start_timestamp = min_timestamp + start_offset;
                let end_timestamp = (start_timestamp + query_window).min(max_timestamp);

                let _ = storage.query(start_timestamp, end_timestamp)?;
            }
        }

        storage.shutdown()?;
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Info) {
        Command::Info => {
            print_overview();
            Ok(())
        }
        Command::BenchmarkStorageRdf => benchmark_segmented_storage_rdf(),
        Command::BenchmarkStorage => benchmark_storage_performance(),
    }
}
