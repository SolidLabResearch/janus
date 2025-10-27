//! RDF Parser and Serializer module

use oxigraph::model::{GraphName, NamedNode, Quad, Triple};
use oxrdfio::RdfFormat as OxiRdfFormat;
use std::io::Cursor;
use wasm_bindgen::prelude::*;

use crate::error::{RdfError, Result};
use crate::RdfFormat;

/// RDF Parser for parsing RDF data from various formats
#[wasm_bindgen]
pub struct RdfParser {
    format: RdfFormat,
    base_iri: Option<String>,
}

#[wasm_bindgen]
impl RdfParser {
    /// Create a new RDF parser for the specified format
    #[wasm_bindgen(constructor)]
    pub fn new(format: &str) -> Result<Self> {
        let format_enum = RdfFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported format".to_string()))?;
        Ok(RdfParser {
            format: format_enum,
            base_iri: None,
        })
    }

    /// Set the base IRI for resolving relative IRIs
    #[wasm_bindgen(js_name = setBaseIri)]
    pub fn set_base_iri(&mut self, base_iri: Option<String>) {
        self.base_iri = base_iri;
    }

    /// Parse RDF data and return triples as JSON string
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(&self, data: &str) -> Result<String> {
        // For now, return a simple validation result
        // TODO: Implement full parsing with oxrdfio when API stabilizes
        let count = self.validate(data)?;

        let result = serde_json::json!({
            "triples": [],
            "count": count,
            "note": "Full parsing implementation pending oxrdfio API stabilization"
        });

        Ok(result.to_string())
    }

    /// Parse and validate RDF data without returning triples
    #[wasm_bindgen(js_name = validate)]
    pub fn validate(&self, data: &str) -> Result<u32> {
        // Basic validation - check if data is not empty and contains expected syntax
        if data.trim().is_empty() {
            return Ok(0);
        }

        // Simple heuristic validation based on format
        let valid = match self.format {
            RdfFormat::Turtle => {
                data.contains("@prefix") || data.contains("a ") || data.contains(":")
            }
            RdfFormat::NTriples => data.lines().all(|line| line.trim().ends_with('.')),
            RdfFormat::RdfXml => data.contains("<rdf:RDF") || data.contains("<rdf"),
            RdfFormat::NQuads => data
                .lines()
                .all(|line| line.split_whitespace().count() >= 4),
            RdfFormat::TriG => data.contains("{") && data.contains("}"),
            RdfFormat::JsonLd => data.trim().starts_with('{'),
        };

        if valid {
            // Estimate triple count
            let estimated_count =
                data.lines().filter(|line| !line.trim().is_empty()).count() as u32;
            Ok(estimated_count)
        } else {
            Err(RdfError::ParseError(
                "Data does not match expected format".to_string(),
            ))
        }
    }

    /// Parse RDF data from a specific format
    #[wasm_bindgen(js_name = parseFromFormat)]
    pub fn parse_from_format(data: &str, format: &str) -> Result<String> {
        let parser = RdfParser::new(format)?;
        parser.parse(data)
    }
}

/// RDF Serializer for serializing RDF data to various formats
#[wasm_bindgen]
pub struct RdfSerializer {
    format: RdfFormat,
}

#[wasm_bindgen]
impl RdfSerializer {
    /// Create a new RDF serializer for the specified format
    #[wasm_bindgen(constructor)]
    pub fn new(format: &str) -> Result<Self> {
        let format_enum = RdfFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported format".to_string()))?;
        Ok(RdfSerializer {
            format: format_enum,
        })
    }

    /// Serialize triples from JSON to RDF format
    #[wasm_bindgen(js_name = serialize)]
    pub fn serialize(&self, triples_json: &str) -> Result<String> {
        // For now, return a placeholder implementation
        // TODO: Implement full serialization with oxrdfio when API stabilizes
        let data: serde_json::Value = serde_json::from_str(triples_json)
            .map_err(|e| RdfError::SerializationError(e.to_string()))?;

        let count = data.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

        match self.format {
            RdfFormat::Turtle => Ok(format!("# Serialized {} triples as Turtle\n# TODO: Implement full serialization\n", count)),
            RdfFormat::NTriples => Ok(format!("# Serialized {} triples as N-Triples\n# TODO: Implement full serialization\n", count)),
            RdfFormat::RdfXml => Ok(format!("<!-- Serialized {} triples as RDF/XML -->\n<!-- TODO: Implement full serialization -->\n", count)),
            RdfFormat::NQuads => Ok(format!("# Serialized {} quads as N-Quads\n# TODO: Implement full serialization\n", count)),
            RdfFormat::TriG => Ok(format!("# Serialized {} triples as TriG\n# TODO: Implement full serialization\n", count)),
            RdfFormat::JsonLd => Ok(format!("{{ \"@context\": {{}}, \"note\": \"Serialized {} triples as JSON-LD\", \"todo\": \"Implement full serialization\" }}", count)),
        }
    }

    /// Convert between RDF formats
    #[wasm_bindgen(js_name = convert)]
    pub fn convert(data: &str, from_format: &str, to_format: &str) -> Result<String> {
        let parser = RdfParser::new(from_format)?;
        let triples_json = parser.parse(data)?;

        let serializer = RdfSerializer::new(to_format)?;
        serializer.serialize(&triples_json)
    }
}

// Helper functions for JSON conversion

fn subject_to_json(subject: &oxigraph::model::Subject) -> serde_json::Value {
    match subject {
        oxigraph::model::Subject::NamedNode(node) => serde_json::json!({
            "type": "uri",
            "value": node.as_str()
        }),
        oxigraph::model::Subject::BlankNode(node) => serde_json::json!({
            "type": "bnode",
            "value": node.as_str()
        }),
        oxigraph::model::Subject::Triple(triple) => serde_json::json!({
            "type": "triple",
            "subject": subject_to_json(&triple.subject),
            "predicate": predicate_to_json(&triple.predicate),
            "object": object_to_json(&triple.object)
        }),
    }
}

fn predicate_to_json(predicate: &NamedNode) -> serde_json::Value {
    serde_json::json!({
        "type": "uri",
        "value": predicate.as_str()
    })
}

fn object_to_json(object: &oxigraph::model::Term) -> serde_json::Value {
    match object {
        oxigraph::model::Term::NamedNode(node) => serde_json::json!({
            "type": "uri",
            "value": node.as_str()
        }),
        oxigraph::model::Term::BlankNode(node) => serde_json::json!({
            "type": "bnode",
            "value": node.as_str()
        }),
        oxigraph::model::Term::Literal(literal) => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "type".to_string(),
                serde_json::Value::String("literal".to_string()),
            );
            obj.insert(
                "value".to_string(),
                serde_json::Value::String(literal.value().to_string()),
            );

            if let Some(lang) = literal.language() {
                obj.insert(
                    "language".to_string(),
                    serde_json::Value::String(lang.to_string()),
                );
            } else if !literal
                .datatype()
                .as_str()
                .eq("http://www.w3.org/2001/XMLSchema#string")
            {
                obj.insert(
                    "datatype".to_string(),
                    serde_json::Value::String(literal.datatype().as_str().to_string()),
                );
            }

            serde_json::Value::Object(obj)
        }
        oxigraph::model::Term::Triple(triple) => serde_json::json!({
            "type": "triple",
            "subject": subject_to_json(&triple.subject),
            "predicate": predicate_to_json(&triple.predicate),
            "object": object_to_json(&triple.object)
        }),
    }
}

fn graph_to_json(graph: &GraphName) -> serde_json::Value {
    match graph {
        GraphName::NamedNode(node) => serde_json::json!({
            "type": "uri",
            "value": node.as_str()
        }),
        GraphName::BlankNode(node) => serde_json::json!({
            "type": "bnode",
            "value": node.as_str()
        }),
        GraphName::DefaultGraph => serde_json::json!({
            "type": "default"
        }),
    }
}

fn json_to_quad(json: &serde_json::Value) -> Result<Quad> {
    let subject = json_to_subject(
        json.get("subject")
            .ok_or_else(|| RdfError::SerializationError("Missing subject".to_string()))?,
    )?;

    let predicate = json_to_predicate(
        json.get("predicate")
            .ok_or_else(|| RdfError::SerializationError("Missing predicate".to_string()))?,
    )?;

    let object = json_to_object(
        json.get("object")
            .ok_or_else(|| RdfError::SerializationError("Missing object".to_string()))?,
    )?;

    let graph = if let Some(graph_json) = json.get("graph") {
        json_to_graph(graph_json)?
    } else {
        GraphName::DefaultGraph
    };

    Ok(Quad::new(subject, predicate, object, graph))
}

fn json_to_subject(json: &serde_json::Value) -> Result<oxigraph::model::Subject> {
    let node_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RdfError::SerializationError("Missing type field".to_string()))?;

    let value = json
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;

    match node_type {
        "uri" => NamedNode::new(value)
            .map(oxigraph::model::Subject::NamedNode)
            .map_err(|e| RdfError::InvalidIri(e.to_string())),
        "bnode" => Ok(oxigraph::model::Subject::BlankNode(
            oxrdf::BlankNode::new_unchecked(value),
        )),
        _ => Err(RdfError::SerializationError(format!(
            "Invalid subject type: {}",
            node_type
        ))),
    }
}

fn json_to_predicate(json: &serde_json::Value) -> Result<NamedNode> {
    let value = json
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;

    NamedNode::new(value).map_err(|e| RdfError::InvalidIri(e.to_string()))
}

fn json_to_object(json: &serde_json::Value) -> Result<oxigraph::model::Term> {
    let node_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RdfError::SerializationError("Missing type field".to_string()))?;

    match node_type {
        "uri" => {
            let value = json
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;
            NamedNode::new(value)
                .map(oxigraph::model::Term::NamedNode)
                .map_err(|e| RdfError::InvalidIri(e.to_string()))
        }
        "bnode" => {
            let value = json
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;
            Ok(oxigraph::model::Term::BlankNode(
                oxrdf::BlankNode::new_unchecked(value),
            ))
        }
        "literal" => {
            let value = json
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;

            let literal = if let Some(lang) = json.get("language").and_then(|v| v.as_str()) {
                oxrdf::Literal::new_language_tagged_literal_unchecked(value, lang)
            } else if let Some(datatype) = json.get("datatype").and_then(|v| v.as_str()) {
                let datatype_node =
                    NamedNode::new(datatype).map_err(|e| RdfError::InvalidIri(e.to_string()))?;
                oxrdf::Literal::new_typed_literal(value, datatype_node)
            } else {
                oxrdf::Literal::new_simple_literal(value)
            };

            Ok(oxigraph::model::Term::Literal(literal))
        }
        _ => Err(RdfError::SerializationError(format!(
            "Invalid object type: {}",
            node_type
        ))),
    }
}

fn json_to_graph(json: &serde_json::Value) -> Result<GraphName> {
    let node_type = json
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RdfError::SerializationError("Missing type field".to_string()))?;

    match node_type {
        "uri" => {
            let value = json
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;
            NamedNode::new(value)
                .map(GraphName::NamedNode)
                .map_err(|e| RdfError::InvalidIri(e.to_string()))
        }
        "bnode" => {
            let value = json
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RdfError::SerializationError("Missing value field".to_string()))?;
            Ok(GraphName::BlankNode(oxrdf::BlankNode::new_unchecked(value)))
        }
        "default" => Ok(GraphName::DefaultGraph),
        _ => Err(RdfError::SerializationError(format!(
            "Invalid graph type: {}",
            node_type
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = RdfParser::new(RdfFormat::Turtle);
        assert_eq!(parser.format, RdfFormat::Turtle);
    }

    #[test]
    fn test_parse_turtle() {
        let parser = RdfParser::new(RdfFormat::Turtle);
        let turtle_data = r#"
            @prefix ex: <http://example.org/> .
            ex:subject ex:predicate ex:object .
        "#;

        let result = parser.parse(turtle_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_turtle() {
        let parser = RdfParser::new(RdfFormat::Turtle);
        let turtle_data = r#"
            @prefix ex: <http://example.org/> .
            ex:subject ex:predicate ex:object .
            ex:subject2 ex:predicate2 ex:object2 .
        "#;

        let count = parser.validate(turtle_data);
        assert!(count.is_ok());
        assert_eq!(count.unwrap(), 2);
    }

    #[test]
    fn test_serializer_creation() {
        let serializer = RdfSerializer::new(RdfFormat::Turtle);
        assert_eq!(serializer.format, RdfFormat::Turtle);
    }

    #[test]
    fn test_parse_ntriples() {
        let parser = RdfParser::new(RdfFormat::NTriples);
        let ntriples_data =
            "<http://example.org/s> <http://example.org/p> <http://example.org/o> .";

        let result = parser.parse(ntriples_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_invalid_data() {
        let parser = RdfParser::new(RdfFormat::Turtle);
        let invalid_data = "This is not valid RDF";

        let result = parser.parse(invalid_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_conversion() {
        let turtle_data = r#"
            @prefix ex: <http://example.org/> .
            ex:Alice ex:knows ex:Bob .
        "#;

        let result = RdfSerializer::convert(turtle_data, RdfFormat::Turtle, RdfFormat::NTriples);
        assert!(result.is_ok());
    }
}
