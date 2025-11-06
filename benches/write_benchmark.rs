use janus::indexing::{dense::DenseIndexBuilder, shared::LogWriter, sparse::SparseIndexBuilder};
use std::fs;
use std::time::Instant;

const DATA_DIR: &str = "data/write_benchmark";
const DENSE_LOG_FILE: &str = "data/write_benchmark/dense_log.dat";
const SPARSE_LOG_FILE: &str = "data/write_benchmark/sparse_log.dat";
const DENSE_INDEX_FILE: &str = "data/write_benchmark/dense.idx";
const SPARSE_INDEX_FILE: &str = "data/write_benchmark/sparse.idx";
const SPARSE_INTERVAL: usize = 1000;

fn setup_dirs() -> std::io::Result<()> {
    let _ = fs::remove_dir_all(DATA_DIR);
    fs::create_dir_all(DATA_DIR)?;
    Ok(())
}

/// Benchmark writing records with dense indexing
/// This simulates real-time writing where each record is indexed immediately
fn benchmark_dense_writing(number_records: u64) -> std::io::Result<(f64, f64)> {
    println!("Benchmarking Dense Index Writing...");

    let mut log_writer = LogWriter::create(DENSE_LOG_FILE)?;
    let mut index_builder = DenseIndexBuilder::create(DENSE_INDEX_FILE)?;

    let start = Instant::now();
    let mut current_offset = 0u64;

    for i in 0..number_records {
        let timestamp = i;
        let subject = (i % 1000) as u64;
        let predicate = (i % 500) as u64;
        let object = (i % 2000) as u64;
        let graph: u64 = 1;

        // Write record to log
        log_writer.append_record(timestamp, subject, predicate, object, graph)?;

        // Add entry to index
        index_builder.add_entry(timestamp, current_offset)?;

        current_offset += 40; // RECORD_SIZE
    }

    let write_time = start.elapsed();

    // Finalize both log and index
    log_writer.flush()?;
    index_builder.finalize()?;

    let total_time = start.elapsed();

    Ok((write_time.as_secs_f64(), total_time.as_secs_f64()))
}

/// Benchmark writing records with sparse indexing
/// This simulates real-time writing where only periodic records are indexed
fn benchmark_sparse_writing(number_records: u64) -> std::io::Result<(f64, f64)> {
    println!("Benchmarking Sparse Index Writing...");

    let mut log_writer = LogWriter::create(SPARSE_LOG_FILE)?;
    let mut index_builder = SparseIndexBuilder::create(SPARSE_INDEX_FILE, SPARSE_INTERVAL)?;

    let start = Instant::now();
    let mut current_offset = 0u64;

    for i in 0..number_records {
        let timestamp = i;
        let subject = (i % 1000) as u64;
        let predicate = (i % 500) as u64;
        let object = (i % 2000) as u64;
        let graph: u64 = 1;

        // Write record to log
        log_writer.append_record(timestamp, subject, predicate, object, graph)?;

        // Add entry to index (will only add if i % interval == 0)
        index_builder.add_entry(i, timestamp, current_offset)?;

        current_offset += 40; // RECORD_SIZE
    }

    let write_time = start.elapsed();

    // Finalize both log and index
    log_writer.flush()?;
    index_builder.finalize()?;

    let total_time = start.elapsed();

    Ok((write_time.as_secs_f64(), total_time.as_secs_f64()))
}

/// Benchmark batch writing vs real-time writing
fn benchmark_batch_vs_realtime(number_records: u64) -> std::io::Result<()> {
    println!("\n=== Batch vs Real-time Writing Comparison ===");

    // Test 1: Real-time writing (as implemented above)
    setup_dirs()?;
    let (dense_write_time, dense_total_time) = benchmark_dense_writing(number_records)?;

    setup_dirs()?;
    let (sparse_write_time, sparse_total_time) = benchmark_sparse_writing(number_records)?;

    // Test 2: Batch writing (write log first, then build index)
    setup_dirs()?;
    println!("Benchmarking Batch Dense Index Creation...");

    let start = Instant::now();
    let mut log_writer = LogWriter::create(DENSE_LOG_FILE)?;
    for i in 0..number_records {
        let timestamp = i;
        let subject = (i % 1000) as u64;
        let predicate = (i % 500) as u64;
        let object = (i % 2000) as u64;
        let graph: u64 = 1;
        log_writer.append_record(timestamp, subject, predicate, object, graph)?;
    }
    log_writer.flush()?;
    let log_write_time = start.elapsed();

    let start = Instant::now();
    janus::indexing::dense::build_dense_index(DENSE_LOG_FILE, DENSE_INDEX_FILE)?;
    let index_build_time = start.elapsed();
    let batch_dense_total = log_write_time.as_secs_f64() + index_build_time.as_secs_f64();

    // Batch sparse
    setup_dirs()?;
    println!("Benchmarking Batch Sparse Index Creation...");

    let start = Instant::now();
    let mut log_writer = LogWriter::create(SPARSE_LOG_FILE)?;
    for i in 0..number_records {
        let timestamp = i;
        let subject = (i % 1000) as u64;
        let predicate = (i % 500) as u64;
        let object = (i % 2000) as u64;
        let graph: u64 = 1;
        log_writer.append_record(timestamp, subject, predicate, object, graph)?;
    }
    log_writer.flush()?;
    let log_write_time = start.elapsed();

    let start = Instant::now();
    janus::indexing::sparse::build_sparse_index(
        SPARSE_LOG_FILE,
        SPARSE_INDEX_FILE,
        &SPARSE_INTERVAL,
    )?;
    let index_build_time = start.elapsed();
    let batch_sparse_total = log_write_time.as_secs_f64() + index_build_time.as_secs_f64();

    // Print results
    println!("\n=== WRITING PERFORMANCE RESULTS ===");
    println!("Records: {}", number_records);
    println!("Sparse interval: {}", SPARSE_INTERVAL);

    println!("\n--- Real-time Writing (Index while writing) ---");
    println!(
        "Dense - Write time: {:.3} ms, Total time: {:.3} ms",
        dense_write_time * 1000.0,
        dense_total_time * 1000.0
    );
    println!(
        "Sparse - Write time: {:.3} ms, Total time: {:.3} ms",
        sparse_write_time * 1000.0,
        sparse_total_time * 1000.0
    );

    println!("\n--- Batch Writing (Index after writing) ---");
    println!(
        "Dense - Log write: {:.3} ms, Index build: {:.3} ms, Total: {:.3} ms",
        log_write_time.as_secs_f64() * 1000.0,
        index_build_time.as_secs_f64() * 1000.0,
        batch_dense_total * 1000.0
    );
    println!(
        "Sparse - Log write: {:.3} ms, Index build: {:.3} ms, Total: {:.3} ms",
        log_write_time.as_secs_f64() * 1000.0,
        index_build_time.as_secs_f64() * 1000.0,
        batch_sparse_total * 1000.0
    );

    println!("\n--- Performance Comparison ---");
    let realtime_speedup = dense_total_time / sparse_total_time;
    let batch_speedup = batch_dense_total / batch_sparse_total;

    if realtime_speedup > 1.0 {
        println!("Real-time: Sparse is {:.2}x faster than Dense", realtime_speedup);
    } else {
        println!("Real-time: Dense is {:.2}x faster than Sparse", 1.0 / realtime_speedup);
    }

    if batch_speedup > 1.0 {
        println!("Batch: Sparse is {:.2}x faster than Dense", batch_speedup);
    } else {
        println!("Batch: Dense is {:.2}x faster than Sparse", 1.0 / batch_speedup);
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    println!("RDF Writing Performance Benchmark: Dense vs Sparse");

    let test_sizes = vec![10_000u64, 100_000u64, 1_000_000u64];

    for &size in &test_sizes {
        println!("\n{:=<60}", "");
        println!("Testing with {} records", size);
        println!("{:=<60}", "");
        benchmark_batch_vs_realtime(size)?;
    }

    Ok(())
}
