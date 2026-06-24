use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, RwLock},
    thread::JoinHandle,
};

use crate::{
    core::{Event, RDFEvent},
    storage::{
        indexing::dictionary::Dictionary,
        util::{BatchBuffer, EnhancedSegmentMetadata, StreamingConfig},
    },
};

mod background;
mod query;
mod segment;

/// Struct for the Implementation of the Segmented Storage of RDF Streams.
pub struct StreamingSegmentedStorage {
    pub(super) batch_buffer: Arc<RwLock<BatchBuffer>>,
    pub(super) segments: Arc<RwLock<Vec<EnhancedSegmentMetadata>>>,
    pub(super) dictionary: Arc<RwLock<Dictionary>>,
    pub(super) flush_lock: Arc<Mutex<()>>,
    pub(super) flush_handle: Option<JoinHandle<()>>,
    pub(super) shutdown_signal: Arc<Mutex<bool>>,
    pub(super) background_flush_error: Arc<Mutex<Option<String>>>,
    pub(super) config: StreamingConfig,
}

impl StreamingSegmentedStorage {
    /// Create a new StreamingSegmentedStorage system.
    pub fn new(config: StreamingConfig) -> std::io::Result<Self> {
        std::fs::create_dir_all(&config.segment_base_path)?;

        // Load or create dictionary
        let dict_path = std::path::Path::new(&config.segment_base_path).join("dictionary.bin");
        let dictionary = if dict_path.exists() {
            match Dictionary::load_from_file(&dict_path) {
                Ok(dict) => dict,
                Err(e) => {
                    eprintln!("Warning: Failed to load dictionary: {}, creating new one", e);
                    Dictionary::new()
                }
            }
        } else {
            Dictionary::new()
        };

        let storage = Self {
            batch_buffer: Arc::new(RwLock::new(BatchBuffer {
                events: VecDeque::new(),
                total_bytes: 0,
                oldest_timestamp_bound: None,
                newest_timestamp_bound: None,
            })),
            segments: Arc::new(RwLock::new(Vec::new())),
            dictionary: Arc::new(RwLock::new(dictionary)),
            flush_lock: Arc::new(Mutex::new(())),
            flush_handle: None,
            shutdown_signal: Arc::new(Mutex::new(false)),
            background_flush_error: Arc::new(Mutex::new(None)),
            config,
        };
        storage.load_existing_segments()?;
        Ok(storage)
    }

    /// Start the background flushing thread for the storage system.
    pub fn start_background_flushing(&mut self) {
        let batch_buffer_clone = Arc::clone(&self.batch_buffer);
        let segments_clone = Arc::clone(&self.segments);
        let shutdown_clone = Arc::clone(&self.shutdown_signal);
        let background_error_clone = Arc::clone(&self.background_flush_error);
        let config_clone = self.config.clone();
        let dictionary_clone = Arc::clone(&self.dictionary);
        let flush_lock_clone = Arc::clone(&self.flush_lock);

        let handle = std::thread::spawn(move || {
            Self::background_flush_loop(
                batch_buffer_clone,
                segments_clone,
                shutdown_clone,
                background_error_clone,
                config_clone,
                dictionary_clone,
                flush_lock_clone,
            );
        });

        self.flush_handle = Some(handle);
    }

    /// Get a reference to the dictionary for decoding events.
    pub fn get_dictionary(&self) -> &Arc<RwLock<Dictionary>> {
        &self.dictionary
    }

    /// Return the most recent background flush error, if one has occurred.
    pub fn background_flush_error(&self) -> Option<String> {
        self.background_flush_error.lock().unwrap().clone()
    }

    pub(super) fn ensure_background_flush_healthy(&self) -> std::io::Result<()> {
        let background_error = self.background_flush_error.lock().unwrap();
        if let Some(message) = background_error.as_ref() {
            return Err(std::io::Error::other(message.clone()));
        }
        Ok(())
    }

    /// Write an event into the storage system.
    pub fn write(&self, event: Event) -> std::io::Result<()> {
        self.ensure_background_flush_healthy()?;
        let event_size = std::mem::size_of::<Event>();

        {
            let mut batch_buffer = self.batch_buffer.write().unwrap();

            if batch_buffer.oldest_timestamp_bound.is_none() {
                batch_buffer.oldest_timestamp_bound = Some(event.timestamp);
            }

            batch_buffer.newest_timestamp_bound = Some(event.timestamp);
            batch_buffer.total_bytes += event_size;
            batch_buffer.events.push_back(event);
        }
        Ok(())
    }

    /// User-friendly API: Write RDF data directly with URI strings.
    pub fn write_rdf(
        &self,
        timestamp: u64,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: &str,
    ) -> std::io::Result<()> {
        let rdf_event = RDFEvent::new(timestamp, subject, predicate, object, graph);
        let encoded_event = {
            let mut dict = self.dictionary.write().unwrap();
            rdf_event.encode(&mut dict)
        };
        self.write(encoded_event)
    }

    /// User-friendly API: Write an RDFEvent directly.
    pub fn write_rdf_event(&self, event: RDFEvent) -> std::io::Result<()> {
        let encoded_event = {
            let mut dict = self.dictionary.write().unwrap();
            event.encode(&mut dict)
        };
        self.write(encoded_event)
    }

    /// Force flush the current batch buffer to disk.
    /// This is useful when you need to ensure data is persisted immediately.
    pub fn flush(&self) -> std::io::Result<()> {
        self.ensure_background_flush_healthy()?;
        self.flush_batch_buffer_to_segment()?;
        self.save_dictionary()?;
        Ok(())
    }

    /// Shutdown the storage system gracefully, ensuring all data is flushed to disk.
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        let background_error = self.ensure_background_flush_healthy().err();
        *self.shutdown_signal.lock().unwrap() = true;

        if let Some(handle) = self.flush_handle.take() {
            handle.join().unwrap();
        }

        // Final Flush after background thread has stopped
        if background_error.is_none() {
            self.flush_batch_buffer_to_segment()?;
        }

        if let Some(err) = background_error {
            return Err(err);
        }

        Ok(())
    }
}
