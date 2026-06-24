use oxigraph::model::{BlankNode, GraphName, NamedNode, NamedOrBlankNode, Quad, Term};
use std::collections::{HashMap, HashSet};

use crate::parsing::janusql_parser::{
    BaselineDefinition, BaselineGraphTemplate, GraphTermTemplate, ParsedJanusQuery, TripleTemplate,
};
use super::{BASELINE_GRAPH, BASELINE_QUERY_NAME};

pub fn parse_numeric(raw: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    let cleaned = trimmed
        .strip_prefix('"')
        .and_then(|value| value.split('"').next())
        .unwrap_or(trimmed)
        .split("^^")
        .next()
        .unwrap_or(trimmed);
    Ok(cleaned.parse::<f64>()?)
}

pub fn materialize_query_defined_baseline_quads(
    parsed: &ParsedJanusQuery,
    bindings: &[HashMap<String, String>],
) -> Result<Vec<Quad>, Box<dyn std::error::Error>> {
    let definition = parsed
        .ast
        .baseline_definitions
        .iter()
        .find(|definition| definition.name == BASELINE_QUERY_NAME)
        .ok_or("missing baseline definition")?;
    let template = parsed
        .baseline_graph_templates
        .iter()
        .find(|template| template.baseline_name == BASELINE_QUERY_NAME)
        .ok_or("missing baseline graph template")?;

    let graph_name = GraphName::NamedNode(NamedNode::new(BASELINE_GRAPH)?);
    let mut quads = Vec::new();
    for binding in bindings {
        for triple in &template.triples {
            let subject = resolve_subject_term(triple, binding)?;
            let predicate = resolve_predicate_term(triple)?;
            let object = resolve_object_term(triple, binding)?;
            quads.push(Quad::new(subject, predicate, object, graph_name.clone()));
        }
    }

    validate_template_against_definition(definition, template)?;
    Ok(quads)
}

pub fn validate_template_against_definition(
    definition: &BaselineDefinition,
    template: &BaselineGraphTemplate,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_variables = definition
        .output_variables
        .iter()
        .map(|variable| variable.trim_start_matches('?'))
        .collect::<HashSet<_>>();

    for triple in &template.triples {
        for term in [&triple.subject, &triple.object] {
            if let GraphTermTemplate::Variable(variable_name) = term {
                if !output_variables.contains(variable_name.as_str()) {
                    return Err(format!(
                        "template references variable '?{}' that is not produced by the baseline SELECT output",
                        variable_name
                    )
                    .into());
                }
            }
        }

        if matches!(triple.predicate, GraphTermTemplate::Variable(_)) {
            return Err("baseline GRAPH template predicates must be concrete IRIs".into());
        }
    }

    Ok(())
}

pub fn resolve_subject_term(
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<NamedOrBlankNode, Box<dyn std::error::Error>> {
    match &triple.subject {
        GraphTermTemplate::Variable(name) => parse_named_or_blank_node(
            binding
                .get(name)
                .ok_or_else(|| format!("missing GRAPH template variable '?{}'", name))?,
        ),
        GraphTermTemplate::Iri(iri) => parse_named_or_blank_node(iri),
        GraphTermTemplate::Literal(raw) => Err(format!(
            "GRAPH template has a literal subject '{}', but subjects must be IRIs or blank nodes",
            raw
        )
        .into()),
    }
}

pub fn resolve_predicate_term(
    triple: &TripleTemplate,
) -> Result<NamedNode, Box<dyn std::error::Error>> {
    match &triple.predicate {
        GraphTermTemplate::Iri(iri) => Ok(NamedNode::new(iri.clone())?),
        GraphTermTemplate::Variable(name) => {
            Err(format!("GRAPH template uses variable predicate '?{}'", name).into())
        }
        GraphTermTemplate::Literal(raw) => Err(format!(
            "GRAPH template has a literal predicate '{}', but predicates must be IRIs",
            raw
        )
        .into()),
    }
}

pub fn resolve_object_term(
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<Term, Box<dyn std::error::Error>> {
    match &triple.object {
        GraphTermTemplate::Variable(name) => {
            let raw_value = binding
                .get(name)
                .ok_or_else(|| format!("missing GRAPH template variable '?{}'", name))?;
            Ok(parse_term(raw_value)?)
        }
        GraphTermTemplate::Iri(iri) => Ok(parse_term(iri)?),
        GraphTermTemplate::Literal(raw) => Ok(Term::Literal(parse_literal_term(raw)?)),
    }
}

pub fn parse_named_or_blank_node(raw: &str) -> Result<NamedOrBlankNode, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    if let Some(name) = trimmed.strip_prefix("_:") {
        Ok(NamedOrBlankNode::BlankNode(BlankNode::new(name)?))
    } else {
        let iri = trimmed.trim_start_matches('<').trim_end_matches('>');
        Ok(NamedOrBlankNode::NamedNode(NamedNode::new(iri)?))
    }
}

pub fn parse_term(raw: &str) -> Result<Term, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    if trimmed.starts_with("_:")
        || trimmed.starts_with('<')
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
    {
        if let Some(name) = trimmed.strip_prefix("_:") {
            return Ok(Term::BlankNode(BlankNode::new(name)?));
        }
        let iri = trimmed.trim_start_matches('<').trim_end_matches('>');
        return Ok(Term::NamedNode(NamedNode::new(iri)?));
    }

    Ok(Term::Literal(parse_literal_term(trimmed)?))
}

pub fn parse_literal_term(raw: &str) -> Result<oxigraph::model::Literal, Box<dyn std::error::Error>> {
    let trimmed = raw.trim();
    if let Ok(value) = trimmed.parse::<i64>() {
        return Ok(oxigraph::model::Literal::new_typed_literal(
            value.to_string(),
            NamedNode::new("http://www.w3.org/2001/XMLSchema#integer")?,
        ));
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return Ok(oxigraph::model::Literal::new_typed_literal(
            value.to_string(),
            NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal")?,
        ));
    }

    if !trimmed.starts_with('"') {
        return Ok(oxigraph::model::Literal::new_simple_literal(trimmed));
    }

    let (lexical, suffix) = split_literal_lexical_and_suffix(trimmed)?;
    let lexical = unescape_literal_lexical(lexical);

    if let Some(language) = suffix.strip_prefix('@') {
        return Ok(oxigraph::model::Literal::new_language_tagged_literal(lexical, language)?);
    }

    if let Some(datatype_iri) = suffix.strip_prefix("^^") {
        let datatype = if datatype_iri.starts_with('<') && datatype_iri.ends_with('>') {
            &datatype_iri[1..datatype_iri.len() - 1]
        } else {
            datatype_iri
        };
        return Ok(oxigraph::model::Literal::new_typed_literal(lexical, NamedNode::new(datatype)?));
    }

    Ok(oxigraph::model::Literal::new_simple_literal(lexical))
}

pub fn split_literal_lexical_and_suffix(raw: &str) -> Result<(&str, &str), Box<dyn std::error::Error>> {
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

    Err("missing closing quote in literal".into())
}

pub fn unescape_literal_lexical(lexical: &str) -> String {
    let mut result = String::with_capacity(lexical.len());
    let mut chars = lexical.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    other => {
                        result.push('\\');
                        result.push(other);
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }

    result
}

pub fn normalize_binding_term(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}
