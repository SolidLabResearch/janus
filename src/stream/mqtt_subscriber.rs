//! MQTT Subscriber for Live Stream Processing
//!
//! This module provides MQTT subscription functionality to receive RDF events
//! from message brokers and feed them to the live query processor.

use crate::{
    core::RDFEvent,
    parsing::rdf_parser,
    stream::live_stream_processing::{LiveStreamProcessing, LiveStreamProcessingError},
};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;

/// Configuration for MQTT subscriber
#[derive(Debug, Clone)]
pub struct MqttSubscriberConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub keep_alive_secs: u64,
    pub topic: String,
    pub stream_uri: String,
    pub window_graph: String,
}

impl Default for MqttSubscriberConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 1883,
            client_id: "janus_subscriber".to_string(),
            keep_alive_secs: 30,
            topic: "sensors".to_string(),
            stream_uri: "http://example.org/sensorStream".to_string(),
            window_graph: "".to_string(),
        }
    }
}

/// MQTT Subscriber that feeds events to live query processor
pub struct MqttSubscriber {
    config: MqttSubscriberConfig,
    runtime: Arc<Runtime>,
    should_stop: Arc<AtomicBool>,
    events_received: Arc<Mutex<u64>>,
    errors: Arc<Mutex<u64>>,
}

#[derive(Debug)]
pub enum MqttSubscriberError {
    ConnectionError(String),
    SubscriptionError(String),
    ParseError(String),
    RuntimeError(String),
}

impl std::fmt::Display for MqttSubscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttSubscriberError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            MqttSubscriberError::SubscriptionError(msg) => {
                write!(f, "Subscription error: {}", msg)
            }
            MqttSubscriberError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            MqttSubscriberError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
        }
    }
}

impl std::error::Error for MqttSubscriberError {}

impl MqttSubscriber {
    /// Create a new MQTT subscriber
    pub fn new(config: MqttSubscriberConfig) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
        );

        Self {
            config,
            runtime,
            should_stop: Arc::new(AtomicBool::new(false)),
            events_received: Arc::new(Mutex::new(0)),
            errors: Arc::new(Mutex::new(0)),
        }
    }

    /// Start subscribing to MQTT and feed events to live processor
    pub fn start(
        &self,
        live_processor: Arc<Mutex<LiveStreamProcessing>>,
    ) -> Result<(), MqttSubscriberError> {
        println!("Starting MQTT subscriber...");
        println!("  Host: {}:{}", self.config.host, self.config.port);
        println!("  Topic: {}", self.config.topic);
        println!("  Stream URI: {}", self.config.stream_uri);
        println!();

        let config = self.config.clone();
        let should_stop = Arc::clone(&self.should_stop);
        let events_received = Arc::clone(&self.events_received);
        let errors = Arc::clone(&self.errors);

        self.runtime.block_on(async move {
            let mut mqttoptions = MqttOptions::new(&config.client_id, &config.host, config.port);
            mqttoptions.set_keep_alive(Duration::from_secs(config.keep_alive_secs));

            let (client, mut eventloop) = AsyncClient::new(mqttoptions, 100);

            // Subscribe to topic
            if let Err(e) = client.subscribe(&config.topic, QoS::AtLeastOnce).await {
                eprintln!("Failed to subscribe to topic '{}': {:?}", config.topic, e);
                return Err(MqttSubscriberError::SubscriptionError(e.to_string()));
            }

            println!("✓ Subscribed to topic: {}", config.topic);
            println!("Listening for events...\n");

            // Event loop
            loop {
                if should_stop.load(Ordering::Relaxed) {
                    println!("Stop signal received, shutting down MQTT subscriber");
                    break;
                }

                match eventloop.poll().await {
                    Ok(notification) => {
                        if let Event::Incoming(Packet::Publish(publish)) = notification {
                            let payload = String::from_utf8_lossy(&publish.payload).to_string();
                            println!("MQTT received message: {}", payload);

                            match rdf_parser::parse_rdf_line(&payload, false) {
                                Ok(mut event) => {
                                    // Force timestamp to be current time for live simulation
                                    event.timestamp = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis() as u64;

                                    // Use empty graph - rsp-rs will assign it to the window's graph automatically
                                    event.graph = String::new();

                                    println!(
                                        "Parsed RDF event: subject={}, predicate={}, object={}, timestamp={}",
                                        event.subject, event.predicate, event.object, event.timestamp
                                    );

                                    let processor = live_processor.lock().unwrap();
                                    match processor.add_event(&config.stream_uri, event.clone()) {
                                        Ok(_) => {
                                            let mut count = events_received.lock().unwrap();
                                            *count += 1;
                                            println!("✓ Event #{} added to live processor", *count);
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to add event to processor: {}", e);
                                            let mut err_count = errors.lock().unwrap();
                                            *err_count += 1;
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to parse RDF line '{}': {}", payload, e);
                                    let mut err_count = errors.lock().unwrap();
                                    *err_count += 1;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("MQTT event loop error: {:?}", e);
                        // Don't break on connection errors, try to reconnect
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }

            Ok(())
        })
    }

    /// Stop the subscriber
    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }

    /// Get metrics
    pub fn get_metrics(&self) -> (u64, u64) {
        let events = *self.events_received.lock().unwrap();
        let errors = *self.errors.lock().unwrap();
        (events, errors)
    }
}
