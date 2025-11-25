use std::{
    collections::HashMap,
    fmt::write,
    sync::{Arc, RwLock},
};

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
            config: (QueryRegistryConfig::default()),
        }
    }

    /// Create option if you wish to create with a custom configuration
    pub fn with_config(config: QueryRegistryConfig) -> Self {
        QueryRegistry { queries: Arc::new(RwLock::new(HashMap::new())), config }
    }
}
