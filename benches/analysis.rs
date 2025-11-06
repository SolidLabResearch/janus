use janus::indexing::{dense, sparse};
use std::fs;
use std::time::Instant;

/// Analyze different sparse intervals to find optimal configuration
fn analyze_sparse_intervals() -> std::io::Result<()> {
    println!("🔍 Analyzing Different Sparse Intervals");
    println!("=====================================");

    let intervals = vec![100, 500, 1000, 2000, 5000, 10000];
    let log_file = "data/benchmark/log.dat";
    let number_records = 100_000u64;

    // Create test data
    fs::create_dir_all("data/benchmark")?;
    let mut writer = janus::indexing::shared::LogWriter::create(log_file)?;
    for i in 0..number_records {
        writer.append_record(i, i % 1000, i % 500, i % 2000, 1)?;
    }
    writer.flush()?;

    println!("Testing {} records with different intervals:", number_records);
    println!("{:-<80}", "");
    println!(
        "{:<10} {:<15} {:<15} {:<20} {:<15}",
        "Interval", "Build Time(ms)", "Index Size(KB)", "Space Savings(%)", "Query Time(ms)"
    );
    println!("{:-<80}", "");

    // Get dense index stats for comparison
    let dense_start = Instant::now();
    dense::build_dense_index(log_file, "data/benchmark/dense_ref.idx")?;
    let dense_build_time = dense_start.elapsed();
    let dense_reader = dense::DenseIndexReader::open("data/benchmark/dense_ref.idx")?;
    let dense_size = dense_reader.index_size_bytes();

    // Test query performance on dense index
    let query_start = Instant::now();
    let _dense_results = dense_reader.query(log_file, 10000, 20000)?;
    let dense_query_time = query_start.elapsed();

    for interval in intervals {
        let index_file = format!("data/benchmark/sparse_{}.idx", interval);

        // Build sparse index
        let start = Instant::now();
        sparse::build_sparse_index(log_file, &index_file, &interval)?;
        let build_time = start.elapsed();

        // Get size info
        let reader = sparse::SparseReader::open(&index_file, interval)?;
        let sparse_size = reader.index_size_bytes();
        let space_savings = ((dense_size - sparse_size) as f64 / dense_size as f64) * 100.0;

        // Test query performance
        let query_start = Instant::now();
        let _sparse_results = reader.query(log_file, 10000, 20000)?;
        let query_time = query_start.elapsed();

        println!(
            "{:<10} {:<15.3} {:<15.2} {:<20.2} {:<15.3}",
            interval,
            build_time.as_secs_f64() * 1000.0,
            sparse_size as f64 / 1024.0,
            space_savings,
            query_time.as_secs_f64() * 1000.0
        );
    }

    println!("{:-<80}", "");
    println!(
        "Dense Reference: Build: {:.3}ms, Size: {:.2}KB, Query: {:.3}ms",
        dense_build_time.as_secs_f64() * 1000.0,
        dense_size as f64 / 1024.0,
        dense_query_time.as_secs_f64() * 1000.0
    );

    Ok(())
}

/// Analyze memory usage patterns
fn analyze_memory_usage() -> std::io::Result<()> {
    println!("\n🧠 Memory Usage Analysis");
    println!("=======================");

    let record_counts = vec![10_000, 50_000, 100_000, 500_000, 1_000_000];

    println!(
        "{:<12} {:<15} {:<15} {:<20}",
        "Records", "Dense Size(MB)", "Sparse Size(MB)", "Memory Ratio"
    );
    println!("{:-<62}", "");

    for &count in &record_counts {
        let log_file = format!("data/benchmark/log_{}.dat", count);
        let dense_index = format!("data/benchmark/dense_{}.idx", count);
        let sparse_index = format!("data/benchmark/sparse_{}.idx", count);

        // Create test data
        let mut writer = janus::indexing::shared::LogWriter::create(&log_file)?;
        for i in 0..count {
            writer.append_record(i, i % 1000, i % 500, i % 2000, 1)?;
        }
        writer.flush()?;

        // Build indexes
        dense::build_dense_index(&log_file, &dense_index)?;
        sparse::build_sparse_index(&log_file, &sparse_index, &1000)?;

        // Get sizes
        let dense_reader = dense::DenseIndexReader::open(&dense_index)?;
        let sparse_reader = sparse::SparseReader::open(&sparse_index, 1000)?;

        let dense_size = dense_reader.index_size_bytes() as f64 / 1_000_000.0;
        let sparse_size = sparse_reader.index_size_bytes() as f64 / 1_000_000.0;
        let ratio = dense_size / sparse_size;

        println!("{:<12} {:<15.3} {:<15.3} {:<20.2}x", count, dense_size, sparse_size, ratio);
    }

    Ok(())
}

/// Test write throughput under different conditions
fn analyze_write_throughput() -> std::io::Result<()> {
    println!("\n⚡ Write Throughput Analysis");
    println!("===========================");

    let test_configs = vec![
        ("Small batches", 1_000u64),
        ("Medium batches", 10_000u64),
        ("Large batches", 100_000u64),
    ];

    println!(
        "{:<15} {:<20} {:<20} {:<15}",
        "Batch Size", "Dense (rec/sec)", "Sparse (rec/sec)", "Speedup"
    );
    println!("{:-<70}", "");

    for (name, batch_size) in test_configs {
        fs::create_dir_all("data/benchmark")?;

        // Test dense writing
        let dense_log = "data/benchmark/dense_throughput.dat";
        let dense_index = "data/benchmark/dense_throughput.idx";

        let start = Instant::now();
        let mut log_writer = janus::indexing::shared::LogWriter::create(dense_log)?;
        let mut index_builder = janus::indexing::dense::DenseIndexBuilder::create(dense_index)?;

        for i in 0..batch_size {
            log_writer.append_record(i, i % 1000, i % 500, i % 2000, 1)?;
            index_builder.add_entry(i, i * 40)?;
        }
        log_writer.flush()?;
        index_builder.finalize()?;

        let dense_time = start.elapsed();
        let dense_throughput = batch_size as f64 / dense_time.as_secs_f64();

        // Test sparse writing
        let sparse_log = "data/benchmark/sparse_throughput.dat";
        let sparse_index = "data/benchmark/sparse_throughput.idx";

        let start = Instant::now();
        let mut log_writer = janus::indexing::shared::LogWriter::create(sparse_log)?;
        let mut index_builder =
            janus::indexing::sparse::SparseIndexBuilder::create(sparse_index, 1000)?;

        for i in 0..batch_size {
            log_writer.append_record(i, i % 1000, i % 500, i % 2000, 1)?;
            index_builder.add_entry(i, i, i * 40)?;
        }
        log_writer.flush()?;
        index_builder.finalize()?;

        let sparse_time = start.elapsed();
        let sparse_throughput = batch_size as f64 / sparse_time.as_secs_f64();

        let speedup = sparse_throughput / dense_throughput;

        println!(
            "{:<15} {:<20.0} {:<20.0} {:<15.2}x",
            name, dense_throughput, sparse_throughput, speedup
        );
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    println!("🔬 Advanced RDF Indexing Analysis Suite");
    println!("=======================================");

    analyze_sparse_intervals()?;
    analyze_memory_usage()?;
    analyze_write_throughput()?;

    println!("\n✨ Analysis Complete!");
    println!("\n💡 Recommendations:");
    println!("  • Use sparse indexing for write-heavy workloads");
    println!("  • Choose interval based on query precision requirements");
    println!("  • Consider hybrid approaches for different use cases");
    println!("  • Monitor memory usage with large datasets");

    Ok(())
}
