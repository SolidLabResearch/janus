#[cfg(not(windows))]
use crate::core::RDFEvent;
use crate::sources::stream_source::{StreamError, StreamSource};
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::message::Message;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Type alias for the complex callback type to reduce type complexity
type CallbackType = Arc<dyn Fn(RDFEvent) + Send + Sync>;

pub struct KafkaSource {
    consumer: Arc<BaseConsumer>,
    callback: Arc<Mutex<Option<CallbackType>>>,
}

impl KafkaSource {
    /// Creates a new Kafka source with a group ID and list of brokers.
    /// # Arguments
    /// * `group_id` - The consumer group ID.
    /// * `brokers` - A comma-separated list of Kafka brokers.
    /// * `auto_offset_reset` - Policy for resetting offsets ("earliest" or "latest").
    pub fn new(
        brokers: &str,
        group_id: &str,
        auto_offset_reset: &str,
    ) -> Result<Self, StreamError> {
        let raw_consumer: BaseConsumer = ClientConfig::new()
            .set("group.id", group_id)
            .set("bootstrap.servers", brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", auto_offset_reset)
            .create()
            .map_err(|e| StreamError::ConnectionError(e.to_string()))?;
        let consumer: Arc<BaseConsumer> = Arc::new(raw_consumer);

        let callback = Arc::new(Mutex::new(None::<CallbackType>));

        let consumer_clone = Arc::clone(&consumer);

        let callback_clone = Arc::clone(&callback);

        // Spawn a thread to handle Kafka events
        thread::spawn(move || {
            loop {
                match consumer_clone.poll(Duration::from_millis(100)) {
                    Some(Ok(message)) => {
                        // TODO: Parse message payload into RDFEvent and call callback
                        if let Some(payload) = message.payload() {
                            // For now, create a dummy RDFEvent
                            let timestamp = message.timestamp().to_millis().unwrap_or(0);
                            let timestamp_u64 = u64::try_from(timestamp).unwrap_or(0);
                            let rdf_event = RDFEvent::new(
                                timestamp_u64,
                                "http://example.org/subject", // subject
                                "http://example.org/predicate", // predicate
                                &String::from_utf8_lossy(payload), // object as string
                                "http://example.org/graph",   // graph
                            );
                            if let Ok(callback_opt) = callback_clone.lock() {
                                if let Some(ref callback) = *callback_opt {
                                    callback(rdf_event);
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("Kafka error: {}", e);
                        break;
                    }
                    None => {
                        // No message, continue polling
                    }
                }
            }
        });

        Ok(KafkaSource { consumer, callback })
    }
}

impl StreamSource for KafkaSource {
    fn subscribe(
        &self,
        topics: Vec<String>,
        callback: Arc<dyn Fn(RDFEvent) + Send + Sync>,
    ) -> Result<(), StreamError> {
        let topic_refs: Vec<&str> = topics.iter().map(|s| s.as_str()).collect();
        self.consumer
            .subscribe(&topic_refs)
            .map_err(|e| StreamError::SubscriptionError(e.to_string()))?;
        if let Ok(mut callback_opt) = self.callback.lock() {
            *callback_opt = Some(callback);
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), StreamError> {
        self.consumer.unsubscribe();
        Ok(())
    }
}
