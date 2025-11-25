use crate::core::RDFEvent;
use crate::sources::stream_source::{StreamError, StreamSource};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, BaseConsumer};
use rdkafka::message::Message;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct KafkaSource {
    consumer: Arc<BaseConsumer>,
    running: Arc<AtomicBool>,
}

impl KafkaSource {
    /// Creates a new Kafka source
    ///
    /// # Arguments
    /// `brokers` - Comma-seperated list of the Kafka brokers
    /// `group_id` - The consumer group id for offset management
    /// `auto_offset_reset` - Offset reset policy ("earliest" or "latest")
    pub fn new(
        brokers: &str,
        group_id: &str,
        auto_offset_reset: &str,
    ) -> Result<Self, StreamError> {
        let consumer: BaseConsumer = ClientConfig::new()
            .set("group.id", group_id)
            .set("bootstrap.servers", brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", auto_offset_reset)
            .create()
            .map_err(|e| StreamError::ConnectionError(e.to_string()))?;

        Ok(KafkaSource { consumer: Arc::new(consumer), running: Arc::new(AtomicBool::new(false)) })
    }
}

