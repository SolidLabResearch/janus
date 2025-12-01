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
        let converted_bindings = self.parse_rsprs_binding_string(&binding.bindings);

        QueryResult {
            query_id: self.query_id.clone(),
            timestamp: binding.timestamp_to as u64,
            source: ResultSource::Live,
            bindings: vec![converted_bindings],
        }
    }

    /// Parses RSP-RS binding string to HashMap format.
    ///
    /// RSP-RS bindings field is a String representation of the bindings.
    /// This parser extracts variable names and values from the debug format:
    /// {Variable { name: "sensor" }: NamedNode(NamedNode { iri: "http://..." }), ...}
    ///
    /// # Arguments
    ///
    /// * `binding_str` - String representation of bindings
    ///
    /// # Returns
    ///
    /// HashMap with variable names as keys and values as strings
    fn parse_rsprs_binding_string(&self, binding_str: &str) -> HashMap<String, String> {
        let mut result = HashMap::new();

        // Split by comma to get individual bindings
        // Format: {Variable { name: "sensor" }: NamedNode(...), Variable { name: "temp" }: Literal(...)}
        let bindings_str = binding_str.trim_matches(|c| c == '{' || c == '}').trim();

        // Split by ", Variable" to separate individual variable bindings
        let parts: Vec<&str> = bindings_str.split(", Variable").collect();

        for (i, part) in parts.iter().enumerate() {
            let binding = if i == 0 {
                // First part already has "Variable" stripped or starts with it
                part.trim_start_matches("Variable")
            } else {
                // Subsequent parts need "Variable" added back
                part
            };

            // Extract variable name
            if let Some(name_start) = binding.find("name: \"") {
                let name_offset = name_start + 7; // length of "name: \""
                if let Some(name_end) = binding[name_offset..].find('"') {
                    let var_name = &binding[name_offset..name_offset + name_end];

                    // Extract value based on type
                    // IMPORTANT: Check TypedLiteral BEFORE NamedNode since TypedLiteral contains NamedNode (for datatype)
                    let value = if binding.contains("TypedLiteral") {
                        // Extract value from TypedLiteral { value: "...", datatype: ... }
                        if let Some(val_start) = binding.find("value: \"") {
                            let val_offset = val_start + 8; // length of "value: \""
                            if let Some(val_end) = binding[val_offset..].find('"') {
                                binding[val_offset..val_offset + val_end].to_string()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else if binding.contains("NamedNode") {
                        // Extract URI from NamedNode(NamedNode { iri: "..." })
                        if let Some(iri_start) = binding.find("iri: \"") {
                            let iri_offset = iri_start + 6; // length of "iri: \""
                            if let Some(iri_end) = binding[iri_offset..].find('"') {
                                binding[iri_offset..iri_offset + iri_end].to_string()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else if binding.contains("Literal(Literal(String(\"") {
                        // Extract string from Literal(Literal(String("...")))
                        if let Some(str_start) = binding.find("String(\"") {
                            let str_offset = str_start + 8; // length of "String(\""
                            if let Some(str_end) = binding[str_offset..].find("\")") {
                                binding[str_offset..str_offset + str_end].to_string()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else if binding.contains("Literal(") {
                        // Other literal types - try to extract the value
                        if let Some(lit_start) = binding.find("Literal(Literal(") {
                            let lit_offset = lit_start + 16;
                            if let Some(lit_end) = binding[lit_offset..].find("))") {
                                binding[lit_offset..lit_offset + lit_end].to_string()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        // Unknown format, skip
                        continue;
                    };

                    result.insert(var_name.to_string(), value);
                }
            }
        }

        result
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
        let converter = ResultConverter::new("test_query".into());

        // Simulate RSP-RS binding string with TypedLiteral (numeric aggregation result)
        let binding_str = r#"{Variable { name: "avgTemp" }: Literal(Literal(TypedLiteral { value: "23.7", datatype: NamedNode { iri: "http://www.w3.org/2001/XMLSchema#decimal" } }))}"#;

        let result = converter.parse_rsprs_binding_string(binding_str);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get("avgTemp"), Some(&"23.7".to_string()));
    }

    #[test]
    fn test_parse_multiple_typed_literals() {
        let converter = ResultConverter::new("test_query".into());

        // Multiple TypedLiterals in one binding
        let binding_str = r#"{Variable { name: "avgTemp" }: Literal(Literal(TypedLiteral { value: "23.7", datatype: NamedNode { iri: "http://www.w3.org/2001/XMLSchema#decimal" } })), Variable { name: "count" }: Literal(Literal(TypedLiteral { value: "24", datatype: NamedNode { iri: "http://www.w3.org/2001/XMLSchema#integer" } }))}"#;

        let result = converter.parse_rsprs_binding_string(binding_str);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("avgTemp"), Some(&"23.7".to_string()));
        assert_eq!(result.get("count"), Some(&"24".to_string()));
    }
}
