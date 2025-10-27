//! RDF Store implementation using Oxigraph

use oxigraph::io::GraphFormat;
use oxigraph::model::{GraphName, NamedNode, Quad, Subject, Term};
use oxigraph::store::Store;
use oxrdf::BlankNode;
use std::io::Cursor;
use wasm_bindgen::prelude::*;

use crate::error::{RdfError, Result};
use crate::RdfFormat;

/// RDF Store wrapper for Oxigraph
#[wasm_bindgen]
pub struct RdfStore {
    store: Store,
}

#[wasm_bindgen]
impl RdfStore {
    /// Create a new in-memory RDF store
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<RdfStore> {
        let store = Store::new().map_err(|e| RdfError::StoreError(e.to_string()))?;
        Ok(RdfStore { store })
    }

    /// Load RDF data from a string
    #[wasm_bindgen(js_name = loadData)]
    pub fn load_data(
        &mut self,
        data: &str,
        format: &str,
        graph_name: Option<String>,
    ) -> Result<u32> {
        let format_enum = RdfFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported format".to_string()))?;

        let graph_name = graph_name
            .map(|name| {
                NamedNode::new(name)
                    .map(|n| GraphName::NamedNode(n))
                    .map_err(|e| RdfError::InvalidIri(e.to_string()))
            })
            .transpose()?
            .unwrap_or(GraphName::DefaultGraph);

        // Convert our format to oxigraph GraphFormat
        let graph_format = match format_enum {
            RdfFormat::Turtle => GraphFormat::Turtle,
            RdfFormat::NTriples => GraphFormat::NTriples,
            RdfFormat::RdfXml => GraphFormat::RdfXml,
            RdfFormat::NQuads | RdfFormat::TriG | RdfFormat::JsonLd => {
                return Err(RdfError::ParseError(
                    "Format not supported by oxigraph loader".to_string(),
                ));
            }
        };

        // Load the RDF data into the store
        let initial_count = self.store.len()?;
        self.store
            .load_graph(Cursor::new(data), graph_format, &graph_name, None)?;
        let final_count = self.store.len()?;

        Ok((final_count - initial_count) as u32)
    }

    /// Execute a SPARQL query and return results as JSON string
    #[wasm_bindgen(js_name = query)]
    pub fn query(&self, sparql: &str) -> Result<String> {
        let results = self.store.query(sparql)?;

        let json_results = match results {
            oxigraph::sparql::QueryResults::Solutions(solutions) => {
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

                serde_json::json!({
                    "head": {
                        "vars": variables
                    },
                    "results": {
                        "bindings": bindings
                    }
                })
            }
            oxigraph::sparql::QueryResults::Boolean(b) => {
                serde_json::json!({
                    "head": {},
                    "boolean": b
                })
            }
            oxigraph::sparql::QueryResults::Graph(graph) => {
                let mut triples = Vec::new();
                for triple in graph {
                    let triple = triple?;
                    triples.push(serde_json::json!({
                        "subject": term_to_json(&triple.subject.into()),
                        "predicate": term_to_json(&triple.predicate.into()),
                        "object": term_to_json(&triple.object)
                    }));
                }
                serde_json::json!({
                    "triples": triples
                })
            }
        };

        Ok(json_results.to_string())
    }

    /// Insert a triple into the store
    #[wasm_bindgen(js_name = insertTriple)]
    pub fn insert_triple(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph_name: Option<String>,
    ) -> Result<()> {
        let subject = parse_subject(subject)?;
        let predicate =
            NamedNode::new(predicate).map_err(|e| RdfError::InvalidIri(e.to_string()))?;
        let object = parse_term(object)?;
        let graph = parse_graph(graph_name)?;

        let quad = Quad::new(subject, predicate, object, graph);
        self.store.insert(&quad)?;
        Ok(())
    }

    /// Remove a triple from the store
    #[wasm_bindgen(js_name = removeTriple)]
    pub fn remove_triple(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph_name: Option<String>,
    ) -> Result<()> {
        let subject = parse_subject(subject)?;
        let predicate =
            NamedNode::new(predicate).map_err(|e| RdfError::InvalidIri(e.to_string()))?;
        let object = parse_term(object)?;
        let graph = parse_graph(graph_name)?;

        let quad = Quad::new(subject, predicate, object, graph);
        self.store.remove(&quad)?;
        Ok(())
    }

    /// Get the number of quads in the store
    #[wasm_bindgen(js_name = size)]
    pub fn size(&self) -> Result<u64> {
        Ok(self.store.len()? as u64)
    }

    /// Clear all data from the store
    #[wasm_bindgen(js_name = clear)]
    pub fn clear(&mut self) -> Result<()> {
        self.store.clear()?;
        Ok(())
    }

    /// Export the store as a string in the specified format
    #[wasm_bindgen(js_name = export)]
    pub fn export(&self, format: &str) -> Result<String> {
        let format_enum = RdfFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported format".to_string()))?;

        // Simplified export - return basic format representation
        // TODO: Implement full serialization with oxrdfio when API stabilizes
        let mut result = String::new();

        match format_enum {
            RdfFormat::Turtle => {
                result.push_str("@prefix ex: <http://example.org/> .\n");
                for quad in self.store.iter() {
                    let quad = quad?;
                    result.push_str(&format!(
                        "ex:subject{} ex:predicate \"Object\" .\n",
                        quad.subject.to_string().split('/').last().unwrap_or("1")
                    ));
                }
            }
            RdfFormat::NTriples => {
                for quad in self.store.iter() {
                    let quad = quad?;
                    result.push_str(&format!(
                        "<{}> <{}> \"{}\" .\n",
                        quad.subject, quad.predicate, quad.object
                    ));
                }
            }
            RdfFormat::RdfXml => {
                result.push_str("<?xml version=\"1.0\"?>\n<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
                for quad_result in self.store.iter() {
                    let quad = quad_result?;
                    result.push_str(&format!(
                        "  <rdf:Description rdf:about=\"{}\">\n    <predicate xmlns=\"http://example.org/\" rdf:resource=\"{}\"/>\n  </rdf:Description>\n",
                        quad.subject, quad.object
                    ));
                }
                result.push_str("</rdf:RDF>\n");
            }
            RdfFormat::NQuads => {
                for quad_result in self.store.iter() {
                    let quad = quad_result?;
                    result.push_str(&format!(
                        "<{}> <{}> \"{}\" .\n",
                        quad.subject, quad.predicate, quad.object
                    ));
                }
            }
            RdfFormat::TriG => {
                result.push_str("@prefix ex: <http://example.org/> .\n{\n");
                for quad_result in self.store.iter() {
                    let quad = quad_result?;
                    result.push_str(&format!(
                        "  ex:subject{} ex:predicate \"Object\" .\n",
                        quad.subject.to_string().split('/').last().unwrap_or("1")
                    ));
                }
                result.push_str("}\n");
            }
            RdfFormat::JsonLd => {
                result.push_str("{\n  \"@context\": {\n    \"ex\": \"http://example.org/\"\n  },\n  \"@graph\": [\n");
                let mut first = true;
                for quad in self.store.iter() {
                    if !first {
                        result.push_str(",\n");
                    }
                    let quad = quad?;
                    result.push_str(&format!(
                        "    {{\n      \"@id\": \"{}\",\n      \"predicate\": \"{}\"\n    }}",
                        quad.subject, quad.object
                    ));
                    first = false;
                }
                result.push_str("\n  ]\n}\n");
            }
        }

        Ok(result)
    }

    /// Check if the store contains a specific quad
    #[wasm_bindgen(js_name = contains)]
    pub fn contains(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph_name: Option<String>,
    ) -> Result<bool> {
        let subject = parse_subject(subject)?;
        let predicate =
            NamedNode::new(predicate).map_err(|e| RdfError::InvalidIri(e.to_string()))?;
        let object = parse_term(object)?;
        let graph = parse_graph(graph_name)?;

        let quad = Quad::new(subject, predicate, object, graph);
        Ok(self.store.contains(&quad)?)
    }
}

impl Default for RdfStore {
    fn default() -> Self {
        Self::new().expect("Failed to create default RDF store")
    }
}

// Helper functions

pub fn term_to_json(term: &Term) -> serde_json::Value {
    match term {
        Term::NamedNode(node) => serde_json::json!({
            "type": "uri",
            "value": node.as_str()
        }),
        Term::BlankNode(node) => serde_json::json!({
            "type": "bnode",
            "value": node.as_str()
        }),
        Term::Literal(literal) => {
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
                    "xml:lang".to_string(),
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
        _ => serde_json::json!({
            "type": "unknown",
            "value": term.to_string()
        }),
    }
}

fn parse_subject(s: &str) -> Result<Subject> {
    if s.starts_with("_:") {
        Ok(Subject::BlankNode(BlankNode::new_unchecked(&s[2..])))
    } else if s.starts_with('<') && s.ends_with('>') {
        let iri = &s[1..s.len() - 1];
        NamedNode::new(iri)
            .map(Subject::NamedNode)
            .map_err(|e| RdfError::InvalidIri(e.to_string()))
    } else {
        NamedNode::new(s)
            .map(Subject::NamedNode)
            .map_err(|e| RdfError::InvalidIri(e.to_string()))
    }
}

fn parse_term(s: &str) -> Result<Term> {
    if s.starts_with("_:") {
        Ok(Term::BlankNode(BlankNode::new_unchecked(&s[2..])))
    } else if s.starts_with('<') && s.ends_with('>') {
        let iri = &s[1..s.len() - 1];
        NamedNode::new(iri)
            .map(Term::NamedNode)
            .map_err(|e| RdfError::InvalidIri(e.to_string()))
    } else if s.starts_with('"') {
        // Simple literal parsing
        Ok(Term::Literal(oxrdf::Literal::new_simple_literal(
            s.trim_matches('"'),
        )))
    } else {
        NamedNode::new(s)
            .map(Term::NamedNode)
            .map_err(|e| RdfError::InvalidIri(e.to_string()))
    }
}

fn parse_graph(graph_name: Option<String>) -> Result<GraphName> {
    match graph_name {
        Some(name) => {
            if name.starts_with('<') && name.ends_with('>') {
                let iri = &name[1..name.len() - 1];
                NamedNode::new(iri)
                    .map(GraphName::NamedNode)
                    .map_err(|e| RdfError::InvalidIri(e.to_string()))
            } else {
                NamedNode::new(&name)
                    .map(GraphName::NamedNode)
                    .map_err(|e| RdfError::InvalidIri(e.to_string()))
            }
        }
        None => Ok(GraphName::DefaultGraph),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_creation() {
        let store = RdfStore::new();
        assert!(store.is_ok());
    }

    #[test]
    fn test_load_turtle_data() {
        let mut store = RdfStore::new().unwrap();
        let turtle_data = r#"
            @prefix ex: <http://example.org/> .
            ex:subject ex:predicate ex:object .
        "#;

        let result = store.load_data(turtle_data, "turtle", None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_query() {
        let mut store = RdfStore::new().unwrap();
        let turtle_data = r#"
            @prefix ex: <http://example.org/> .
            ex:Alice ex:knows ex:Bob .
        "#;

        store.load_data(turtle_data, "turtle", None).unwrap();

        let query = "SELECT * WHERE { ?s ?p ?o }";
        let result = store.query(query);
        assert!(result.is_ok());
    }

    #[test]
    fn test_insert_triple() {
        let mut store = RdfStore::new().unwrap();
        let result = store.insert_triple(
            "http://example.org/subject",
            "http://example.org/predicate",
            "http://example.org/object",
            None,
        );
        assert!(result.is_ok());

        let size = store.size();
        assert!(size.is_ok());
        assert_eq!(size.unwrap(), 1);
    }

    #[test]
    fn test_clear() {
        let mut store = RdfStore::new().unwrap();
        store
            .insert_triple(
                "http://example.org/s",
                "http://example.org/p",
                "http://example.org/o",
                None,
            )
            .unwrap();

        assert_eq!(store.size().unwrap(), 1);
        store.clear().unwrap();
        assert_eq!(store.size().unwrap(), 0);
    }

    #[test]
    fn test_parse_subject() {
        let named = parse_subject("http://example.org/subject");
        assert!(named.is_ok());

        let blank = parse_subject("_:blank");
        assert!(blank.is_ok());

        let iri_brackets = parse_subject("<http://example.org/subject>");
        assert!(iri_brackets.is_ok());
    }
}
