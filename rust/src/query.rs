//! SPARQL Query execution module

use oxigraph::sparql::{Query, QueryResults};
use oxigraph::store::Store;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::error::{RdfError, Result};
use crate::QueryResultFormat;

/// Query executor for SPARQL queries
#[wasm_bindgen]
pub struct QueryExecutor {
    store: Store,
}

#[wasm_bindgen]
impl QueryExecutor {
    /// Create a new query executor with an in-memory store
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<QueryExecutor> {
        let store = Store::new().map_err(|e| RdfError::StoreError(e.to_string()))?;
        Ok(QueryExecutor { store })
    }

    /// Execute a SPARQL SELECT query
    #[wasm_bindgen(js_name = executeSelect)]
    pub fn execute_select(&self, query: &str) -> Result<String> {
        let results = self.store.query(query)?;

        match results {
            QueryResults::Solutions(solutions) => {
                let mut bindings = Vec::new();
                let mut variables = Vec::new();

                for solution_result in solutions {
                    let solution = solution_result?;

                    if variables.is_empty() {
                        variables = solution
                            .variables()
                            .into_iter()
                            .map(|v| v.as_str().to_string())
                            .collect();
                    }

                    let mut binding = serde_json::Map::new();
                    for var in solution.variables() {
                        if let Some(term) = solution.get(var) {
                            binding
                                .insert(var.as_str().to_string(), crate::store::term_to_json(term));
                        }
                    }
                    bindings.push(serde_json::Value::Object(binding));
                }

                let result = serde_json::json!({
                    "head": {
                        "vars": variables
                    },
                    "results": {
                        "bindings": bindings
                    }
                });

                Ok(result.to_string())
            }
            _ => Err(RdfError::QueryError(
                "Query is not a SELECT query".to_string(),
            )),
        }
    }

    /// Execute a SPARQL ASK query
    #[wasm_bindgen(js_name = executeAsk)]
    pub fn execute_ask(&self, query: &str) -> Result<bool> {
        let results = self.store.query(query)?;

        match results {
            QueryResults::Boolean(b) => Ok(b),
            _ => Err(RdfError::QueryError(
                "Query is not an ASK query".to_string(),
            )),
        }
    }

    /// Execute a SPARQL CONSTRUCT query
    #[wasm_bindgen(js_name = executeConstruct)]
    pub fn execute_construct(&self, query: &str) -> Result<String> {
        let results = self.store.query(query)?;

        match results {
            QueryResults::Graph(graph) => {
                let mut triples = Vec::new();
                for triple in graph {
                    let triple = triple?;
                    triples.push(serde_json::json!({
                        "subject": crate::store::term_to_json(&triple.subject.into()),
                        "predicate": crate::store::term_to_json(&triple.predicate.into()),
                        "object": crate::store::term_to_json(&triple.object)
                    }));
                }
                let result = serde_json::json!({
                    "triples": triples
                });
                Ok(result.to_string())
            }
            _ => Err(RdfError::QueryError(
                "Query is not a CONSTRUCT query".to_string(),
            )),
        }
    }

    /// Validate a SPARQL query without executing it
    #[wasm_bindgen(js_name = validateQuery)]
    pub fn validate_query(&self, query: &str) -> Result<String> {
        match Query::parse(query, None) {
            Ok(_) => Ok("VALID".to_string()),
            Err(e) => Err(RdfError::QueryError(e.to_string())),
        }
    }
}

impl Default for QueryExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default query executor")
    }
}

/// Query result representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct QueryResult {
    result_type: String,
    data: String,
}

#[wasm_bindgen]
impl QueryResult {
    #[wasm_bindgen(constructor)]
    pub fn new(result_type: String, data: String) -> Self {
        QueryResult { result_type, data }
    }

    #[wasm_bindgen(getter)]
    pub fn result_type(&self) -> String {
        self.result_type.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn data(&self) -> String {
        self.data.clone()
    }

    /// Parse the result as JSON
    #[wasm_bindgen(js_name = asJson)]
    pub fn as_json(&self) -> Result<JsValue> {
        let value: serde_json::Value = serde_json::from_str(&self.data)
            .map_err(|e| RdfError::SerializationError(e.to_string()))?;
        serde_wasm_bindgen::to_value(&value)
            .map_err(|e| RdfError::SerializationError(e.to_string()))
    }

    /// Get the result as a string
    #[wasm_bindgen(js_name = asString)]
    pub fn as_string(&self) -> String {
        self.data.clone()
    }
}

/// Query builder for constructing SPARQL queries
#[wasm_bindgen]
pub struct QueryBuilder {
    query_type: String,
    prefixes: Vec<String>,
    variables: Vec<String>,
    patterns: Vec<String>,
    filters: Vec<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Vec<String>,
}

#[wasm_bindgen]
impl QueryBuilder {
    /// Create a new SELECT query builder
    #[wasm_bindgen(js_name = select)]
    pub fn select(variables: Vec<String>) -> Self {
        QueryBuilder {
            query_type: "SELECT".to_string(),
            prefixes: Vec::new(),
            variables,
            patterns: Vec::new(),
            filters: Vec::new(),
            limit: None,
            offset: None,
            order_by: Vec::new(),
        }
    }

    /// Create a new CONSTRUCT query builder
    #[wasm_bindgen(js_name = construct)]
    pub fn construct(template: String) -> Self {
        QueryBuilder {
            query_type: "CONSTRUCT".to_string(),
            prefixes: Vec::new(),
            variables: vec![template],
            patterns: Vec::new(),
            filters: Vec::new(),
            limit: None,
            offset: None,
            order_by: Vec::new(),
        }
    }

    /// Create a new ASK query builder
    #[wasm_bindgen(js_name = ask)]
    pub fn ask() -> Self {
        QueryBuilder {
            query_type: "ASK".to_string(),
            prefixes: Vec::new(),
            variables: Vec::new(),
            patterns: Vec::new(),
            filters: Vec::new(),
            limit: None,
            offset: None,
            order_by: Vec::new(),
        }
    }

    /// Add a prefix
    #[wasm_bindgen(js_name = addPrefix)]
    pub fn add_prefix(&mut self, prefix: String, iri: String) {
        self.prefixes.push(format!("PREFIX {}: <{}>", prefix, iri));
    }

    /// Add a triple pattern
    #[wasm_bindgen(js_name = addPattern)]
    pub fn add_pattern(&mut self, subject: String, predicate: String, object: String) {
        self.patterns
            .push(format!("{} {} {} .", subject, predicate, object));
    }

    /// Add a FILTER clause
    #[wasm_bindgen(js_name = addFilter)]
    pub fn add_filter(&mut self, filter: String) {
        self.filters.push(format!("FILTER({})", filter));
    }

    /// Set the LIMIT
    #[wasm_bindgen(js_name = setLimit)]
    pub fn set_limit(&mut self, limit: u32) {
        self.limit = Some(limit);
    }

    /// Set the OFFSET
    #[wasm_bindgen(js_name = setOffset)]
    pub fn set_offset(&mut self, offset: u32) {
        self.offset = Some(offset);
    }

    /// Add ORDER BY clause
    #[wasm_bindgen(js_name = addOrderBy)]
    pub fn add_order_by(&mut self, variable: String, desc: bool) {
        if desc {
            self.order_by.push(format!("DESC({})", variable));
        } else {
            self.order_by.push(variable);
        }
    }

    /// Build the SPARQL query string
    #[wasm_bindgen(js_name = build)]
    pub fn build(&self) -> String {
        let mut query = String::new();

        // Add prefixes
        for prefix in &self.prefixes {
            query.push_str(&prefix);
            query.push('\n');
        }

        if !self.prefixes.is_empty() {
            query.push('\n');
        }

        // Add query type
        match self.query_type.as_str() {
            "SELECT" => {
                query.push_str("SELECT ");
                if self.variables.is_empty() {
                    query.push('*');
                } else {
                    query.push_str(&self.variables.join(" "));
                }
                query.push('\n');
            }
            "CONSTRUCT" => {
                query.push_str("CONSTRUCT {\n");
                if !self.variables.is_empty() {
                    query.push_str("  ");
                    query.push_str(&self.variables[0]);
                    query.push('\n');
                }
                query.push_str("}\n");
            }
            "ASK" => {
                query.push_str("ASK\n");
            }
            _ => {}
        }

        // Add WHERE clause
        query.push_str("WHERE {\n");
        for pattern in &self.patterns {
            query.push_str("  ");
            query.push_str(pattern);
            query.push('\n');
        }
        for filter in &self.filters {
            query.push_str("  ");
            query.push_str(filter);
            query.push('\n');
        }
        query.push_str("}\n");

        // Add ORDER BY
        if !self.order_by.is_empty() {
            query.push_str("ORDER BY ");
            query.push_str(&self.order_by.join(" "));
            query.push('\n');
        }

        // Add LIMIT
        if let Some(limit) = self.limit {
            query.push_str(&format!("LIMIT {}\n", limit));
        }

        // Add OFFSET
        if let Some(offset) = self.offset {
            query.push_str(&format!("OFFSET {}\n", offset));
        }

        query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_executor_creation() {
        let executor = QueryExecutor::new();
        assert!(executor.is_ok());
    }

    #[test]
    fn test_query_validation() {
        let executor = QueryExecutor::new().unwrap();
        let result = executor.validate_query("SELECT * WHERE { ?s ?p ?o }");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "SELECT");
    }

    #[test]
    fn test_query_builder_select() {
        let mut builder = QueryBuilder::select(vec!["?s".to_string(), "?p".to_string()]);
        builder.add_prefix("ex".to_string(), "http://example.org/".to_string());
        builder.add_pattern("?s".to_string(), "?p".to_string(), "?o".to_string());
        builder.set_limit(10);

        let query = builder.build();
        assert!(query.contains("PREFIX ex:"));
        assert!(query.contains("SELECT ?s ?p"));
        assert!(query.contains("?s ?p ?o"));
        assert!(query.contains("LIMIT 10"));
    }

    #[test]
    fn test_query_builder_ask() {
        let mut builder = QueryBuilder::ask();
        builder.add_pattern("?s".to_string(), "a".to_string(), "ex:Person".to_string());

        let query = builder.build();
        assert!(query.contains("ASK"));
        assert!(query.contains("WHERE"));
    }

    #[test]
    fn test_query_builder_with_filter() {
        let mut builder = QueryBuilder::select(vec!["?name".to_string()]);
        builder.add_pattern(
            "?person".to_string(),
            "ex:name".to_string(),
            "?name".to_string(),
        );
        builder.add_filter("?name = \"Alice\"".to_string());

        let query = builder.build();
        assert!(query.contains("FILTER"));
    }

    #[test]
    fn test_query_builder_order_by() {
        let mut builder = QueryBuilder::select(vec!["?s".to_string()]);
        builder.add_pattern("?s".to_string(), "?p".to_string(), "?o".to_string());
        builder.add_order_by("?s".to_string(), false);

        let query = builder.build();
        assert!(query.contains("ORDER BY ?s"));
    }
}
