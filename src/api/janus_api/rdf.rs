use crate::{
    api::janus_api::types::JanusApiError,
    parsing::janusql_parser::{GraphTermTemplate, TripleTemplate},
};
use oxigraph::model::{BlankNode, Literal, NamedNode, NamedOrBlankNode, Term};
use std::collections::HashMap;

pub(crate) fn resolve_subject_template_term(
    baseline_name: &str,
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<NamedOrBlankNode, JanusApiError> {
    match &triple.subject {
        GraphTermTemplate::Variable(variable_name) => {
            let raw_value = binding.get(variable_name).ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "Baseline '{}' binding is missing GRAPH template variable '?{}'",
                    baseline_name, variable_name
                ))
            })?;
            parse_subject_term(raw_value).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Failed to materialize baseline '{}' subject from variable '?{}' with value '{}': {}",
                    baseline_name, variable_name, raw_value, e
                ))
            })
        }
        GraphTermTemplate::Iri(iri) => parse_named_or_blank_node(iri).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Failed to materialize baseline '{}' subject IRI '{}': {}",
                baseline_name, iri, e
            ))
        }),
        GraphTermTemplate::Literal(raw_literal) => Err(JanusApiError::ExecutionError(format!(
            "GRAPH template for baseline '{}' has a literal subject '{}', but subjects must be IRIs or blank nodes",
            baseline_name, raw_literal
        ))),
    }
}

pub(crate) fn resolve_predicate_template_term(
    baseline_name: &str,
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<NamedNode, JanusApiError> {
    match &triple.predicate {
        GraphTermTemplate::Iri(iri) => NamedNode::new(iri.clone()).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Failed to materialize baseline '{}' predicate '{}': {}",
                baseline_name, iri, e
            ))
        }),
        GraphTermTemplate::Variable(variable_name) => {
            let _ = binding;
            Err(JanusApiError::ExecutionError(format!(
                "GRAPH template for baseline '{}' uses variable predicate '?{}', but predicates must be concrete IRIs for now",
                baseline_name, variable_name
            )))
        }
        GraphTermTemplate::Literal(raw_literal) => Err(JanusApiError::ExecutionError(format!(
            "GRAPH template for baseline '{}' has a literal predicate '{}', but predicates must be IRIs",
            baseline_name, raw_literal
        ))),
    }
}

pub(crate) fn resolve_object_template_term(
    baseline_name: &str,
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<Term, JanusApiError> {
    match &triple.object {
        GraphTermTemplate::Variable(variable_name) => {
            let raw_value = binding.get(variable_name).ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "Baseline '{}' binding is missing GRAPH template variable '?{}'",
                    baseline_name, variable_name
                ))
            })?;
            parse_term(raw_value).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Failed to materialize baseline '{}' object from variable '?{}' with value '{}': {}",
                    baseline_name, variable_name, raw_value, e
                ))
            })
        }
        GraphTermTemplate::Iri(iri) => parse_term(iri).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Failed to materialize baseline '{}' object IRI '{}': {}",
                baseline_name, iri, e
            ))
        }),
        GraphTermTemplate::Literal(raw_literal) => {
            parse_literal_term(raw_literal).map(Term::Literal).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Failed to materialize baseline '{}' literal object '{}': {}",
                    baseline_name, raw_literal, e
                ))
            })
        }
    }
}

pub(crate) fn parse_subject_term(raw: &str) -> Result<NamedOrBlankNode, String> {
    parse_named_or_blank_node(raw)
}

pub(crate) fn parse_term(raw: &str) -> Result<Term, String> {
    if let Some(blank_node) = normalize_blank_node_term(raw) {
        return BlankNode::new(blank_node).map(Term::BlankNode).map_err(|e| e.to_string());
    }
    if let Some(iri) = normalize_iri_term(raw) {
        return NamedNode::new(iri).map(Term::NamedNode).map_err(|e| e.to_string());
    }

    parse_literal_term(raw).map(Term::Literal)
}

pub(crate) fn parse_named_or_blank_node(raw: &str) -> Result<NamedOrBlankNode, String> {
    if let Some(blank_node) = normalize_blank_node_term(raw) {
        return BlankNode::new(blank_node)
            .map(NamedOrBlankNode::BlankNode)
            .map_err(|e| e.to_string());
    }
    if let Some(iri) = normalize_iri_term(raw) {
        return NamedNode::new(iri).map(NamedOrBlankNode::NamedNode).map_err(|e| e.to_string());
    }
    Err(format!("expected IRI or blank node subject but found {}", raw.trim()))
}

pub(crate) fn parse_literal_term(raw: &str) -> Result<Literal, String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('"') {
        if trimmed.parse::<i64>().is_ok() {
            return Ok(Literal::new_typed_literal(
                trimmed,
                NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            ));
        }
        if trimmed.parse::<f64>().is_ok() {
            return Ok(Literal::new_typed_literal(
                trimmed,
                NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
            ));
        }
        return Ok(Literal::new_simple_literal(trimmed));
    }

    let (lexical, suffix) = split_literal_lexical_and_suffix(trimmed)?;
    let lexical = unescape_literal_lexical(lexical);

    if let Some(language) = suffix.strip_prefix('@') {
        return Literal::new_language_tagged_literal(lexical, language).map_err(|e| e.to_string());
    }

    if let Some(datatype_iri) = suffix.strip_prefix("^^") {
        let datatype = if datatype_iri.starts_with('<') && datatype_iri.ends_with('>') {
            &datatype_iri[1..datatype_iri.len() - 1]
        } else {
            datatype_iri
        };
        return Ok(Literal::new_typed_literal(
            lexical,
            NamedNode::new(datatype).map_err(|e| e.to_string())?,
        ));
    }

    Ok(Literal::new_simple_literal(lexical))
}

pub(crate) fn split_literal_lexical_and_suffix(raw: &str) -> Result<(&str, &str), String> {
    let mut escaped = false;

    for (index, ch) in raw.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Ok((&raw[1..index], raw[index + 1..].trim())),
            _ => {}
        }
    }

    Err(format!("invalid RDF literal '{}'", raw))
}

pub(crate) fn unescape_literal_lexical(raw: &str) -> String {
    raw.replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
}

pub(crate) fn normalize_binding_term(raw: &str) -> String {
    normalize_iri_term(raw)
        .or_else(|| normalize_literal_term(raw))
        .unwrap_or_else(|| raw.trim().to_string())
}

pub(crate) fn normalize_iri_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() > 2 {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub(crate) fn normalize_blank_node_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    trimmed.strip_prefix("_:").map(str::to_string)
}

pub(crate) fn normalize_literal_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('"') {
        return None;
    }

    let mut escaped = false;
    for (index, ch) in trimmed.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => {
                let lexical = &trimmed[1..index];
                return Some(
                    lexical
                        .replace("\\\"", "\"")
                        .replace("\\\\", "\\")
                        .replace("\\n", "\n")
                        .replace("\\t", "\t"),
                );
            }
            _ => {}
        }
    }

    None
}
