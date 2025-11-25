use crate::core::RDFEvent;
use crate::sources::stream_source::StreamSource;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use std::sync::Arc;

pub struct StreamIngestionPipeline {
    storage: Arc<StreamingSegmentedStorage>,
    sources: Vec<Box<dyn StreamSource>>,
}

impl StreamIngestionPipeline {
    pub fn new(storage: Arc<StreamingSegmentedStorage>) -> Self {
        StreamIngestionPipeline { storage, sources: Vec::new() }
    }

    /// Adding the source for the stream ingestion pipeline (which can be MQTT, Kafka, etc.)
    pub fn add_source(&mut self, source: Box<dyn StreamSource>) {
        self.sources.push(source);
    }

    /// Start the stream ingestion pipeline by subscribing to the sources and ingesting data
    /// into storage as well as the live stream processing RSP Engine.
    pub fn start(&self, topics: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
        let storage = Arc::clone(&self.storage);

        // Shared callback writes to the storage (handles both storage and live processing)
        let callback: Arc<dyn Fn(RDFEvent) + Send + Sync> = Arc::new(move |event: RDFEvent| {
            // Storage will handle the background flushing.
            // TODO: Add live stream processing here as a process.
            if let Err(e) = storage.write_rdf_event(event) {
                eprintln!("Error writing to storage: {:?}", e);
            }
        });

        for source in &self.sources {
            source.subscribe(topics.clone(), Arc::clone(&callback))?;
        }

        Ok(())
    }
}
