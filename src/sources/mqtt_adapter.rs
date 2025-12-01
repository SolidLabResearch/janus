use crate::core::RDFEvent;
use crate::parsing::rdf_parser;
use crate::sources::stream_source::{StreamError, StreamSource};
use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct MqttSource {
    client: Client,
    callback_store: Arc<Mutex<Option<Arc<dyn Fn(RDFEvent) + Send + Sync>>>>,
}

impl MqttSource {
    pub fn new(broker: &str, port: u16, client_id: &str) -> Result<Self, StreamError> {
        let mut mqtt_options = MqttOptions::new(client_id, broker, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(30));

        let (client, mut connection) = Client::new(mqtt_options, 10);
        let callback_store = Arc::new(Mutex::new(None::<Arc<dyn Fn(RDFEvent) + Send + Sync>>));
        let callback_store_clone = Arc::clone(&callback_store);

        // Starting a thread to handle incoming messages
        thread::spawn(move || {
            for notification in connection.iter() {
                match notification {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if let Ok(payload) = std::str::from_utf8(&publish.payload) {
                            // Parse the RDF line
                            // We assume live data, so we don't force add_timestamps=true here
                            // because the source might already have timestamps.
                            // However, if parsing fails to find a timestamp, it defaults to now.
                            match rdf_parser::parse_rdf_line(payload, false) {
                                Ok(event) => {
                                    let store = callback_store_clone.lock().unwrap();
                                    if let Some(cb) = &*store {
                                        cb(event);
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Failed to parse MQTT message: {} - Error: {}",
                                        payload, e
                                    );
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("MQTT connection error: {:?}", e);
                        // In a real app we might want to reconnect or exit
                        // For now, we just log and continue (rumqttc handles reconnection usually)
                        // But connection.iter() might end on error?
                        // rumqttc 0.18 iter() blocks and handles reconnects?
                        // Actually rumqttc's iter() returns Result<Event, ConnectionError>.
                        // If it returns error, it might be fatal or temporary.
                        // Let's assume it keeps going or we break.
                        // If we break, the source stops working.
                        // For this demo, let's break to avoid infinite error loops if broker is down.
                        // But wait, if broker is down, we want to retry?
                        // rumqttc handles reconnects internally, so we might just get error notifications.
                    }
                }
            }
        });

        Ok(MqttSource { client, callback_store })
    }
}

impl StreamSource for MqttSource {
    fn subscribe(
        &self,
        topics: Vec<String>,
        callback: Arc<dyn Fn(RDFEvent) + Send + Sync>,
    ) -> Result<(), StreamError> {
        // Store the callback
        {
            let mut store = self.callback_store.lock().unwrap();
            *store = Some(callback);
        }

        for topic in topics {
            self.client
                .subscribe(&topic, QoS::AtLeastOnce)
                .map_err(|e| StreamError::SubscriptionError(e.to_string()))?;
        }

        Ok(())
    }

    fn stop(&self) -> Result<(), StreamError> {
        self.client
            .disconnect()
            .map_err(|e| StreamError::ConnectionError(e.to_string()))
    }
}
