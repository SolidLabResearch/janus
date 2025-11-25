use crate::core::RDFEvent;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

pub trait StreamSource: Send + Sync {
    // Subscribe to the stream and invoke a callback for each RDF event.
    fn subscribe(
        &self,
        topics: Vec<String>,
        callback: Arc<dyn Fn(RDFEvent) + Send + Sync>,
    ) -> Result<(), StreamError>;

    // Unsubscribe from the stream or stop the subscription.
    fn stop(&self) -> Result<(), StreamError>;
}

#[derive(Debug)]
pub enum StreamError {
    ConnectionError(String),
    SubscriptionError(String),
    Other(String),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            StreamError::SubscriptionError(msg) => write!(f, "Subscription error: {}", msg),
            StreamError::Other(msg) => write!(f, "Other error: {}", msg),
        }
    }
}

impl Error for StreamError {}
