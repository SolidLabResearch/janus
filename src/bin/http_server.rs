//! HTTP Server Binary for Janus API
//!
//! This binary starts the Janus HTTP API server, providing REST and WebSocket endpoints
//! for query management and stream bus replay control.
//!
//! Usage:
//!   cargo run --bin http_server -- --host 0.0.0.0 --port 8080 --storage-dir ./data/storage

use clap::Parser;
use janus::{
    api::janus_api::JanusApi,
    http::start_server,
    parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "Janus HTTP Server")]
#[command(about = "HTTP API server for Janus RDF Stream Processing Engine", long_about = None)]
struct Args {
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    #[arg(short, long, default_value = "8080")]
    port: u16,

    #[arg(short, long, default_value = "./data/storage")]
    storage_dir: String,

    #[arg(long, default_value = "10485760")]
    max_batch_size_bytes: usize,

    #[arg(long, default_value = "5000")]
    flush_interval_ms: u64,

    #[arg(long, default_value = "1024")]
    max_total_memory_mb: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║             Janus RDF Stream Processing Engine                ║");
    println!("║                    HTTP API Server                            ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    // Initialize storage
    println!("Initializing storage at: {}", args.storage_dir);
    let storage_config = StreamingConfig {
        segment_base_path: args.storage_dir.clone(),
        max_batch_bytes: args.max_batch_size_bytes,
        max_batch_age_seconds: args.flush_interval_ms / 1000,
        max_batch_events: 100_000,
        sparse_interval: 1000,
        entries_per_index_block: 1024,
    };

    let mut storage =
        StreamingSegmentedStorage::new(storage_config).expect("Failed to initialize storage");

    // Start background flushing thread
    storage.start_background_flushing();
    println!("  - Background flushing: enabled");

    let storage = Arc::new(storage);
    println!("  - Max batch size: {} bytes", args.max_batch_size_bytes);
    println!("  - Max batch age: {} seconds", args.flush_interval_ms / 1000);
    println!();

    // Initialize query registry
    println!("Initializing query registry...");
    let registry = Arc::new(QueryRegistry::new());
    println!();

    // Initialize JanusQL parser
    println!("Initializing JanusQL parser...");
    let parser = JanusQLParser::new().expect("Failed to initialize JanusQL parser");
    println!();

    // Initialize Janus API
    println!("Initializing Janus API...");
    let janus_api = Arc::new(
        JanusApi::new(parser, Arc::clone(&registry), Arc::clone(&storage))
            .expect("Failed to initialize Janus API"),
    );
    println!();

    // Start HTTP server
    let addr = format!("{}:{}", args.host, args.port);
    println!("Starting HTTP server...");
    println!();

    // Set up graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c().await.expect("Failed to install CTRL+C signal handler");
        println!();
        println!("Shutdown signal received, stopping server...");
    };

    // Run server with graceful shutdown
    tokio::select! {
        result = start_server(&addr, janus_api, registry, storage) => {
            if let Err(e) = result {
                eprintln!("Server error: {}", e);
            }
        }
        _ = shutdown_signal => {
            println!("Server shut down gracefully");
        }
    }

    Ok(())
}
