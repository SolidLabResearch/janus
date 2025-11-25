use crate::core::RDFEvent;
use crate::sources::stream_source::{StreamError, StreamSource};
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::thread;

pub struct MqttSource {
    client: Client,
}

impl MqttSource {
    pub fn new(broker: &str, port: u16, client_id: &str) -> Result<Self, StreamError> {
        let mut mqtt_options = MqttOptions::new(client_id, broker, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(30));

        let (client, mut connection) = Client::new(mqtt_options, 10);

        // Starting a thread to handle incoming messages

        thread::spawn(move || {
            for notification in connection.iter() {
                if let Err(e) = notification {
                    eprintln!("MQTT connection error: {:?}", e);
                    break;
                }
            }
        });

        Ok(MqttSource { client })
    }
}

impl StreamSource for MqttSource {
    fn subscribe(
        &self,
        topics: Vec<String>,
        callback: Arc<dyn Fn(RDFEvent) + Send + Sync>,
    ) -> Result<(), StreamError> {
        for topic in topics {
            self.client
                .subscribe(&topic, QoS::AtLeastOnce)
                .map_err(|e| StreamError::SubscriptionError(e.to_string()))?;
        }

        // TODO : Here we would normally handle incoming messages and invoke the callback.
        Ok(())
    }

    fn stop(&self) -> Result<(), StreamError> {
        self.client
            .disconnect()
            .map_err(|e| StreamError::ConnectionError(e.to_string()))
    }
}
