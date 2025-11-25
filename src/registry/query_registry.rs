use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::parsing::janusql_parser::ParsedJanusQuery;

pub type QueryId = String;

/// Metadata associated with a registered query
#[derive(Debug, Clone)]
pub struct QueryMetadata {
    pub query_id: QueryId,
    pub query_text: String,
    pub parsed: ParsedJanusQuery,
    pub registered_at: u64,
    pub execution_count: u64,
    pub subscribers: Vec<QueryId>,
}

#[derive(Debug, Clone, Default)]
pub struct QueryRegistryConfig {
    /// Maximum number of queries that can be registered
    pub max_queries: Option<usize>,
}

/// Defining usual errors specific to the Query Registry Operations
#[derive(Debug)]
pub enum QueryRegistryError {
    QueryNotFound(QueryId),
    QueryAlreadyExists(QueryId),
    MaxQueriesReached,
    InvalidQuery(String),
}

impl std::fmt::Display for QueryRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryRegistryError::QueryNotFound(id) => write!(f, "Query not found : {}", id),
            QueryRegistryError::QueryAlreadyExists(id) => {
                write!(f, "Query already exists : {}", id)
            }
            QueryRegistryError::MaxQueriesReached => {
                write!(f, "Maximum number of registered queries reached")
            }
            QueryRegistryError::InvalidQuery(msg) => write!(f, "Invalid query: {}", msg),
        }
    }
}

impl std::error::Error for QueryRegistryError {}

/// Core Query Registry structure which is the foundation for further query optimization and analysis.
#[allow(dead_code)]
pub struct QueryRegistry {
    queries: Arc<RwLock<HashMap<QueryId, QueryMetadata>>>,
    config: QueryRegistryConfig,
}

impl QueryRegistry {
    /// Create a new Query Registry with the given configuration
    pub fn new() -> Self {
        QueryRegistry {
            queries: Arc::new(RwLock::new(HashMap::new())),
            config: QueryRegistryConfig::default(),
        }
    }

    /// Create option if you wish to create with a custom configuration
    pub fn with_config(config: QueryRegistryConfig) -> Self {
        QueryRegistry { queries: Arc::new(RwLock::new(HashMap::new())), config }
    }

    /// Register a query. Returns the stored metadata on success.
    pub fn register(
        &self,
        query_id: QueryId,
        query_text: String,
        parsed: ParsedJanusQuery,
    ) -> Result<QueryMetadata, QueryRegistryError> {
        // Check if query ID already exists
        {
            let queries = self.queries.read().unwrap();
            if queries.contains_key(&query_id) {
                return Err(QueryRegistryError::QueryAlreadyExists(query_id));
            }
        }

        // Check registry capacity
        if let Some(max) = self.config.max_queries {
            let queries = self.queries.read().unwrap();
            if queries.len() >= max {
                return Err(QueryRegistryError::MaxQueriesReached);
            }
        }

        let metadata = QueryMetadata {
            query_id: query_id.clone(),
            query_text,
            parsed,
            registered_at: Self::current_timestamp(),
            execution_count: 0,
            subscribers: Vec::new(),
        };

        // Store the query in the registry
        {
            let mut queries = self.queries.write().unwrap();
            queries.insert(query_id.clone(), metadata.clone());
        }

        Ok(metadata)
    }

    /// Find a query by the given QueryId
    pub fn get(&self, query_id: &QueryId) -> Option<QueryMetadata> {
        let queries = self.queries.read().unwrap();
        queries.get(query_id).cloned()
    }

    /// Function to add the subscriber to a query
    pub fn add_subscriber(
        &self,
        query_id: &QueryId,
        subscriber_id: QueryId,
    ) -> Result<(), QueryRegistryError> {
        let mut queries = self.queries.write().unwrap();
        if let Some(metadata) = queries.get_mut(query_id) {
            metadata.subscribers.push(subscriber_id);
            Ok(())
        } else {
            Err(QueryRegistryError::QueryNotFound(query_id.clone()))
        }
    }

    pub fn increment_execution_count(&self, query_id: &QueryId) -> Result<(), QueryRegistryError> {
        let mut queries = self.queries.write().unwrap();
        let query = queries
            .get_mut(query_id)
            .ok_or_else(|| QueryRegistryError::QueryNotFound(query_id.clone()))?;

        query.execution_count += 1;
        Ok(())
    }

    /// To remove a query from the registry
    pub fn unregister(&self, query_id: &QueryId) -> Result<QueryMetadata, QueryRegistryError> {
        let mut queries = self.queries.write().unwrap();
        queries
            .remove(query_id)
            .ok_or_else(|| QueryRegistryError::QueryNotFound(query_id.clone()))
    }

    /// Get all the registered queries by their Query IDs.
    pub fn list_all(&self) -> Vec<QueryId> {
        let queries = self.queries.read().unwrap();
        queries.keys().cloned().collect()
    }

    /// Clear all queries from the registry
    pub fn clear(&self) {
        let mut queries = self.queries.write().unwrap();
        queries.clear();
    }

    pub fn get_statistics(&self) -> RegistryStatistics {
        let queries = self.queries.read().unwrap();
        let total_queries = queries.len();
        let total_subscribers = queries.values().map(|q| q.subscribers.len()).sum();

        RegistryStatistics { total_queries, total_subscribers }
    }

    fn current_timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
        since_the_epoch.as_secs()
    }
}

impl Default for QueryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RegistryStatistics {
    pub total_queries: usize,
    pub total_subscribers: usize,
}
