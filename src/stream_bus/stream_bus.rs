//! Stream Bus to read the RDF data from a file and publishing to a Kafka and Streaming Storage at the same time.
//!
//! The module implements a high-throughput event bus that does the following things:
//! 1. Will read the RDF events from the file.
//! 2. It will publish the event to the Kafka / MQTT topic.
//! 3. It will write the event to the Janus Streaming Storage.
//! 4. It provides replay rate defined and will replay the event.

use crate::core::RDFEvent;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use core::str;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::fmt::write;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::{self, Runtime};
use tokio::time::sleep;

/// Defining the Broker Type
/// 1. Kafka
/// 2. MQTT
/// 3. None, in which case it won't write to a stream but rather only to the Segmented Storage.
#[derive(Debug, Clone)]
pub enum BrokerType {
    Kafka,
    Mqtt,
    None,
}

/// Defining the KafkaConfiguration
#[derive(Debug, Clone)]
pub struct KafkaConfig {
    pub bootstrap_servers: String,
    pub client_id: String,
    pub message_timeout_ms: String,
}

/// Definining the MQTT Configuration
#[derive(Debug, Clone)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub keep_alive_secs: u64,
}

/// Configuration for the Stream Bus
#[derive(Debug, Clone)]
pub struct StreamBusConfig {
    pub input_file: String,
    pub broker_type: BrokerType,
    pub topics: Vec<String>,
    pub rate_of_publishing: u64,
    pub loop_file: bool,
    pub add_timestamps: bool,
    pub kafka_config: Option<KafkaConfig>,
    pub mqtt_config: Option<MqttConfig>,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: "localhost:9092".to_string(),
            client_id: "janus_stream_bus".to_string(),
            message_timeout_ms: "5000".to_string(),
        }
    }
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 1883,
            client_id: "janus_stream_bus".to_string(),
            keep_alive_secs: 30,
        }
    }
}

/// Metrics collected by the Stream Bus.
pub struct StreamBusMetrics {
    pub events_read: u64,
    pub events_published: u64,
    pub events_stored: u64,
    pub publish_errors: u64,
    pub storage_errors: u64,
    pub elapsed_seconds: f64,
}

impl StreamBusMetrics {
    pub fn events_per_second(&self) -> f64 {
        if self.elapsed_seconds > 0.0 {
            self.events_read as f64 / self.elapsed_seconds
        } else {
            0.0
        }
    }

    pub fn publish_success_rate(&self) -> f64 {
        if self.events_read > 0 {
            (self.events_published as f64 / self.events_read as f64) * 100.0
        } else {
            0.0
        }
    }

    pub fn storage_success_rate(&self) -> f64 {
        if self.events_read > 0 {
            (self.events_stored as f64 / self.events_read as f64) * 100
        } else {
            0.0
        }
    }
}

/// Main Stream Bus's Architecture
pub struct StreamBus {
    config: StreamBusConfig,
    storage: Arc<StreamingSegmentedStorage>,
    runtime: Arc<Runtime>,
    events_read: Arc<AtomicU64>,
    events_published: Arc<AtomicU64>,
    events_stored: Arc<AtomicU64>,
    publish_errors: Arc<AtomicU64>,
    storage_erros: Arc<AtomicU64>,
    should_stop: Arc<AtomicBool>,
}

/// Error types for the Stream Bus
#[derive(Debug)]
pub enum StreamBusError {
    FileError(String),
    BrokerError(String),
    ConfigError(String),
}

impl std::fmt::Display for StreamBusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamBusError::FileError(msg) => write!(f, "File Error: {}", msg),
            StreamBusError::ConfigError(msg) => write!(f, "Config Error: {}", msg),
            StreamBusError::BrokerError(msg) => write!(f, "Broker Error: {}", msg),
        }
    }
}

impl std::error::Error for StreamBusError {}

impl StreamBus {
    pub fn new(config: StreamBusConfig, storage: Arc<StreamingSegmentedStorage>) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .expect("Failed to create the runtime for Tokio."),
        );

        Self {
            config,
            storage,
            runtime,
            events_read: Arc::new(AtomicU64::new(0)),
            events_published: Arc::new(AtomicU64::new(0)),
            events_stored: Arc::new(AtomicU64::new(0)),
            publish_errors: Arc::new(AtomicU64::new(0)),
            storage_erros: Arc::new(AtomicU64::new(0)),
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the stream bus.
    pub fn start(&self) -> Result<StreamBusMetrics, StreamBusError> {
        println!("Starting the Stream Bus");
        println!("Input: {}", self.config.input_file);
        println!("Broker: {:?}", self.config.broker_type);
        println!("Topics: {:?}", self.config.topics);
        println!("Rate of publishing: {} Hz", if self.config.rate_of_publishing == 0{
            "unlimited".to_string()
        } else {
            self.config.rate_of_publishing.to_string()
        });
        
        println!("Loop: {}", self.config.loop_file);
        println!();
        
        
        let start_time = Instant::now();
        
        match self.config.broker_type {
            BrokerType::Kafka => self.runtime.block_on(self.run_with_kafka())?,
            BrokerType::Mqtt => self.runtime.block_on(self.run_with_mqtt())?,
            BrokerType::None => self.runtime.block_on(self.run_storage_only())?,
        }
    
        let elapsed = start_time.elapsed().as_secs_f64();
        
        Ok(StreamBusMetrics { events_read: (), events_published: (), events_stored: (), publish_errors: (), storage_errors: (), elapsed_seconds: () })
    }
}
