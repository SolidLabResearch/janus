//! Result Converter Utilities
//!
//! This module provides utilities for converting various query result formats
//! into the unified `QueryResult` type used by the JanusApi.
//!
//! # Supported Conversions
//!
//! - `HashMap<String, String>` (from HistoricalExecutor) → `QueryResult`
//! - `BindingWithTimestamp` (from LiveStreamProcessing) → `QueryResult`
//!
//! # Example
//!
//! ```ignore
//! use janus::execution::result_converter::ResultConverter;
//!
//! let converter = ResultConverter::new("query_1".into());
//!
//! // Convert historical bindings
//! let bindings = vec![hashmap!{"s" => "...", "p" => "..."}];
//! let results = converter.from_historical_bindings(bindings, timestamp);
//!
//! // Convert live bindings
//! let live_binding = BindingWithTimestamp { ... };
//! let result = converter.from_live_binding(live_binding);
//! ```

use crate::api::janus_api::{QueryResult, ResultSource};
use crate::registry::query_registry::QueryId;
use rsp_rs::BindingWithTimestamp;
use std::collections::HashMap;

/// Converter for transforming execution results into unified QueryResult format.
///
/// This utility encapsulates the logic for converting results from different
/// execution engines (historical and live) into the common `QueryResult` type.
pub struct ResultConverter {
    query_id: QueryId,
}

/// Parses an RSP-RS binding debug string into Janus' `HashMap<String, String>` binding format.
///
/// This intentionally preserves the existing ad-hoc parsing behavior. It is fragile because it
/// depends on the current `Debug` representation emitted by RSP-RS, but this refactor keeps that
/// behavior rather than redesigning the parser.
pub fn parse_rsprs_binding_string(binding_str: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let bindings_str = binding_str.trim_matches(|ch| ch == '{' || ch == '}').trim();
    let parts = bindings_str.split(", Variable").collect::<Vec<_>>();

    for (index, part) in parts.iter().enumerate() {
        let binding = if index == 0 {
            part.trim_start_matches("Variable")
        } else {
            part
        };
        let Some(name_start) = binding.find("name: \"") else {
            continue;
        };
        let name_offset = name_start + 7;
        let Some(name_end) = binding[name_offset..].find('"') else {
            continue;
        };
        let variable = &binding[name_offset..name_offset + name_end];
        let value = if binding.contains("TypedLiteral") {
            extract_between(binding, "value: \"", "\"")
        } else if binding.contains("NamedNode") {
            extract_between(binding, "iri: \"", "\"")
        } else if binding.contains("Literal(Literal(String(\"") {
            extract_between(binding, "String(\"", "\")")
        } else if binding.contains("Literal(Literal(") {
            extract_between(binding, "Literal(Literal(", "))")
        } else {
            None
        };
        if let Some(value) = value {
            result.insert(variable.to_string(), value);
        }
    }

    result
}

fn extract_between(input: &str, start: &str, end: &str) -> Option<String> {
    let start_index = input.find(start)? + start.len();
    let end_index = input[start_index..].find(end)?;
    Some(input[start_index..start_index + end_index].to_string())
}

impl ResultConverter {
    /// Creates a new ResultConverter for a specific query.
    ///
    /// # Arguments
    ///
    /// * `query_id` - The query identifier to attach to all results
    pub fn new(query_id: QueryId) -> Self {
        Self { query_id }
    }

    /// Converts historical SPARQL bindings to QueryResult.
    ///
    /// # Arguments
    ///
    /// * `bindings` - Vector of variable bindings from SPARQL execution
    /// * `timestamp` - Timestamp for this result (usually window end time)
    ///
    /// # Returns
    ///
    /// A QueryResult with Historical source
    pub fn from_historical_bindings(
        &self,
        bindings: Vec<HashMap<String, String>>,
        timestamp: u64,
    ) -> QueryResult {
        QueryResult {
            query_id: self.query_id.clone(),
            timestamp,
            source: ResultSource::Historical,
            bindings,
        }
    }

    /// Converts a single historical binding to QueryResult.
    ///
    /// # Arguments
    ///
    /// * `binding` - Single variable binding map
    /// * `timestamp` - Timestamp for this result
    ///
    /// # Returns
    ///
    /// A QueryResult with a single binding and Historical source
    pub fn from_historical_binding(
        &self,
        binding: HashMap<String, String>,
        timestamp: u64,
    ) -> QueryResult {
        QueryResult {
            query_id: self.query_id.clone(),
            timestamp,
            source: ResultSource::Historical,
            bindings: vec![binding],
        }
    }

    /// Converts a live stream binding to QueryResult.
    ///
    /// # Arguments
    ///
    /// * `binding` - BindingWithTimestamp from RSP-RS engine
    ///
    /// # Returns
    ///
    /// A QueryResult with Live source
    ///
    /// # Example
    ///
    /// ```ignore
    /// let live_result = converter.from_live_binding(rsp_binding);
    /// assert_eq!(live_result.source, ResultSource::Live);
    /// ```
    pub fn from_live_binding(&self, binding: BindingWithTimestamp) -> QueryResult {
        // Convert RSP-RS binding format to HashMap
        // Note: bindings is a String in rsp-rs, so we parse it
        let converted_bindings = parse_rsprs_binding_string(&binding.bindings);

        QueryResult {
            query_id: self.query_id.clone(),
            timestamp: binding.timestamp_to as u64,
            source: ResultSource::Live,
            bindings: vec![converted_bindings],
        }
    }

    /// Batch converts multiple historical bindings to QueryResults.
    ///
    /// Useful when you have multiple result rows from a single SPARQL query
    /// and want to emit them as individual QueryResults.
    ///
    /// # Arguments
    ///
    /// * `bindings` - Vector of binding maps
    /// * `timestamp` - Timestamp to use for all results
    ///
    /// # Returns
    ///
    /// Vector of QueryResults, one per binding
    pub fn from_historical_bindings_batch(
        &self,
        bindings: Vec<HashMap<String, String>>,
        timestamp: u64,
    ) -> Vec<QueryResult> {
        bindings
            .into_iter()
            .map(|binding| self.from_historical_binding(binding, timestamp))
            .collect()
    }

    /// Creates an empty QueryResult (for queries with no matches).
    ///
    /// # Arguments
    ///
    /// * `timestamp` - Timestamp for the empty result
    /// * `source` - Whether this is from Historical or Live processing
    ///
    /// # Returns
    ///
    /// QueryResult with empty bindings
    pub fn empty_result(&self, timestamp: u64, source: ResultSource) -> QueryResult {
        QueryResult { query_id: self.query_id.clone(), timestamp, source, bindings: vec![] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_historical_binding() {
        let converter = ResultConverter::new("test_query".into());

        let mut binding = HashMap::new();
        binding.insert("s".to_string(), "<http://example.org/alice>".to_string());
        binding.insert("p".to_string(), "<http://example.org/knows>".to_string());

        let result = converter.from_historical_binding(binding.clone(), 1000);

        assert_eq!(result.query_id, "test_query");
        assert_eq!(result.timestamp, 1000);
        assert!(matches!(result.source, ResultSource::Historical));
        assert_eq!(result.bindings.len(), 1);
        assert_eq!(result.bindings[0], binding);
    }

    #[test]
    fn test_from_historical_bindings() {
        let converter = ResultConverter::new("test_query".into());

        let mut binding1 = HashMap::new();
        binding1.insert("s".to_string(), "<http://example.org/alice>".to_string());

        let mut binding2 = HashMap::new();
        binding2.insert("s".to_string(), "<http://example.org/bob>".to_string());

        let bindings = vec![binding1.clone(), binding2.clone()];

        let result = converter.from_historical_bindings(bindings, 2000);

        assert_eq!(result.timestamp, 2000);
        assert_eq!(result.bindings.len(), 2);
        assert_eq!(result.bindings[0], binding1);
        assert_eq!(result.bindings[1], binding2);
    }

    #[test]
    fn test_from_historical_bindings_batch() {
        let converter = ResultConverter::new("test_query".into());

        let mut binding1 = HashMap::new();
        binding1.insert("s".to_string(), "<http://example.org/alice>".to_string());

        let mut binding2 = HashMap::new();
        binding2.insert("s".to_string(), "<http://example.org/bob>".to_string());

        let bindings = vec![binding1.clone(), binding2.clone()];

        let results = converter.from_historical_bindings_batch(bindings, 3000);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].bindings.len(), 1);
        assert_eq!(results[0].bindings[0], binding1);
        assert_eq!(results[1].bindings.len(), 1);
        assert_eq!(results[1].bindings[0], binding2);
    }

    #[test]
    fn test_empty_result_historical() {
        let converter = ResultConverter::new("test_query".into());

        let result = converter.empty_result(5000, ResultSource::Historical);

        assert_eq!(result.query_id, "test_query");
        assert_eq!(result.timestamp, 5000);
        assert!(matches!(result.source, ResultSource::Historical));
        assert!(result.bindings.is_empty());
    }

    #[test]
    fn test_empty_result_live() {
        let converter = ResultConverter::new("test_query".into());

        let result = converter.empty_result(6000, ResultSource::Live);

        assert_eq!(result.timestamp, 6000);
        assert!(matches!(result.source, ResultSource::Live));
        assert!(result.bindings.is_empty());
    }

    #[test]
    fn test_converter_reuse() {
        let converter = ResultConverter::new("reusable_query".into());

        let mut binding1 = HashMap::new();
        binding1.insert("x".to_string(), "value1".to_string());

        let mut binding2 = HashMap::new();
        binding2.insert("y".to_string(), "value2".to_string());

        let result1 = converter.from_historical_binding(binding1, 1000);
        let result2 = converter.from_historical_binding(binding2, 2000);

        assert_eq!(result1.query_id, "reusable_query");
        assert_eq!(result2.query_id, "reusable_query");
        assert_eq!(result1.timestamp, 1000);
        assert_eq!(result2.timestamp, 2000);
    }

    #[test]
    fn test_parse_typed_literal_binding() {
        // Simulate RSP-RS binding string with TypedLiteral (numeric aggregation result)
        let binding_str = r#"{Variable { name: "avgTemp" }: Literal(Literal(TypedLiteral { value: "23.7", datatype: NamedNode { iri: "http://www.w3.org/2001/XMLSchema#decimal" } }))}"#;

        let result = parse_rsprs_binding_string(binding_str);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get("avgTemp"), Some(&"23.7".to_string()));
    }

    #[test]
    fn test_parse_multiple_typed_literals() {
        // Multiple TypedLiterals in one binding
        let binding_str = r#"{Variable { name: "avgTemp" }: Literal(Literal(TypedLiteral { value: "23.7", datatype: NamedNode { iri: "http://www.w3.org/2001/XMLSchema#decimal" } })), Variable { name: "count" }: Literal(Literal(TypedLiteral { value: "24", datatype: NamedNode { iri: "http://www.w3.org/2001/XMLSchema#integer" } }))}"#;

        let result = parse_rsprs_binding_string(binding_str);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("avgTemp"), Some(&"23.7".to_string()));
        assert_eq!(result.get("count"), Some(&"24".to_string()));
    }

    #[test]
    fn test_parse_named_node_and_string_literal_bindings() {
        let binding_str = r#"{Variable { name: "sensor" }: NamedNode(NamedNode { iri: "http://example.org/sensor/1" }), Variable { name: "label" }: Literal(Literal(String("ok")))}"#;

        let result = parse_rsprs_binding_string(binding_str);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("sensor"), Some(&"http://example.org/sensor/1".to_string()));
        assert_eq!(result.get("label"), Some(&"ok".to_string()));
    }

    #[test]
    fn test_parse_other_literal_binding() {
        let binding_str = r#"{Variable { name: "flag" }: Literal(Literal(Boolean(true)))}"#;

        let result = parse_rsprs_binding_string(binding_str);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get("flag"), Some(&"Boolean(true".to_string()));
    }
}
