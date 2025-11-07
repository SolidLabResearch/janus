use crate::storage::indexing::{dense, sparse};
use crate::indexing::shared::LogWriter;
use std::fs;
use std::time::Instant;

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

fn main() -> std::io::Result<()> {
    println!("RDF Indexing Benchmark : Dense vs Sparse");
    println!("Setting up data...");
    let number_of_records = 1_000_000u64;
    setup_data(number_of_records)?;

    benchmark_indexing()?;
    benchmark_queries()?;

    println!(
        "\n=== Summary ===\nSparse interval: {}\nUse this data to decide \
         which approach suits your use case best.",
        SPARSE_INTERVAL
    );
    Ok(())
}
