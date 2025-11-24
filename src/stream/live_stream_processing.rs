pub struct LiveStreamProcessing {}
#[derive(Debug)]
pub struct LiveStreamProcessingError(String);

impl std::fmt::Display for LiveStreamProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LiveStreamProcessingError: {}", self.0)
    }
}

impl std::error::Error for LiveStreamProcessingError {}

impl LiveStreamProcessing {
    pub fn new(rspql_query: String) -> Result<Self, LiveStreamProcessingError> {
        Ok(Self {})
    }
}
