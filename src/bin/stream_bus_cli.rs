//! Stream Bus CLI - Command Line tool for the Stream Bus to publish the data to a broker and storage.
//!
//! Usage:
//!   stream-bus-cli --input data/sensors.nq --broker kafka --topics sensors --rate 64
//!   stream-bus-cli --input data/sensors.nq --broker mqtt --topics sensors --rate 64 --loop-file
//!   stream-bus-cli --input data/sensors.nq --broker none --rate 0

use clap::Parser;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::stream_bus::{BrokerType, KafkaConfig, MqttConfig, StreamBus, StreamBusConfig};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "stream-bus-cli")]
#[command(about = "Stream Bus - Publish RDF Events to brokers and the data storage")]
struct Args {
    /// Input file path (N-Triples or N-Quads)
    #[arg(short, long)]
    input: String,

    /// Broker type: kafka, mqtt, or none
    #[arg(short, long, default_value = "kafka")]
    broker: String,

    /// Topics to publish to (comma-separated)
    #[arg(short, long, default_value = "sensors")]
    topics: String,

    /// Publishing rate in Hz (0 = unlimited)
    #[arg(short, long, default_value = "64")]
    rate: u64,

    /// Loop the file indefinitely
    #[arg(long)]
    loop_file: bool,

    /// Add timestamps if not present
    #[arg(long)]
    add_timestamps: bool,

    /// Kafka bootstrap servers
    #[arg(long, default_value = "localhost:9092")]
    kafka_servers: String,

    /// MQTT host
    #[arg(long, default_value = "localhost")]
    mqtt_host: String,

    /// MQTT port
    #[arg(long, default_value = "1883")]
    mqtt_port: u16,

    /// Storage path
    #[arg(long, default_value = "data/stream_bus_storage")]
    storage_path: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("Stream Bus CLI");
    println!("==============\n");

    let storage_config = StreamingConfig {
        max_batch_events: 500_000,
        max_batch_age_seconds: 1,
        max_batch_bytes: 50_000_000,
        sparse_interval: 1000,
        entries_per_index_block: 100,
        segment_base_path: args.storage_path.clone(),
    };

    let mut storage = StreamingSegmentedStorage::new(storage_config)?;
    storage.start_background_flushing();
    let storage = Arc::new(storage);

    let broker_type = match args.broker.to_lowercase().as_str() {
        "kafka" => BrokerType::Kafka,
        "mqtt" => BrokerType::Mqtt,
        "none" => BrokerType::None,
        _ => {
            eprintln!("Error: Unknown broker type: {}", args.broker);
            eprintln!("Valid options: kafka, mqtt, none");
            std::process::exit(1);
        }
    };

    let topics: Vec<String> = args.topics.split(',').map(|s| s.trim().to_string()).collect();

    let bus_config = StreamBusConfig {
        input_file: args.input.clone(),
        broker_type: broker_type.clone(),
        topics: topics.clone(),
        rate_of_publishing: args.rate,
        loop_file: args.loop_file,
        add_timestamps: args.add_timestamps,
        kafka_config: match broker_type {
            BrokerType::Kafka => {
                Some(KafkaConfig { bootstrap_servers: args.kafka_servers, ..Default::default() })
            }
            _ => None,
        },
        mqtt_config: match broker_type {
            BrokerType::Mqtt => Some(MqttConfig {
                host: args.mqtt_host,
                port: args.mqtt_port,
                ..Default::default()
            }),
            _ => None,
        },
    };

    println!("Configuration:");
    println!("  Input file: {}", args.input);
    println!("  Broker: {:?}", broker_type);
    println!("  Topics: {:?}", topics);
    println!(
        "  Rate: {} Hz",
        if args.rate == 0 {
            "unlimited".to_string()
        } else {
            args.rate.to_string()
        }
    );
    println!("  Loop file: {}", args.loop_file);
    println!("  Add timestamps: {}", args.add_timestamps);
    println!("  Storage: {}", args.storage_path);
    println!();

    let bus = StreamBus::new(bus_config, Arc::clone(&storage));

    let should_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, stopping...");
        should_stop_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    })?;

    let handle = bus.start_async();

    let metrics = handle.join().expect("Thread panicked")?;

    println!("\nStream Bus Complete!");
    println!("====================");
    println!("Events read:      {}", metrics.events_read);
    println!(
        "Events published: {} ({:.1}%)",
        metrics.events_published,
        metrics.publish_success_rate()
    );
    println!(
        "Events stored:    {} ({:.1}%)",
        metrics.events_stored,
        metrics.storage_success_rate()
    );
    println!("Publish errors:   {}", metrics.publish_errors);
    println!("Storage errors:   {}", metrics.storage_errors);
    println!("Elapsed time:     {:.2}s", metrics.elapsed_seconds);
    println!("Throughput:       {:.1} events/sec", metrics.events_per_second());

    Ok(())
}
