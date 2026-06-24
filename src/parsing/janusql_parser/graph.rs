use std::collections::HashMap;
use crate::parsing::janusql_parser::JanusQLParser;
use crate::parsing::janusql_parser::ast::{BaselineGraphTemplate, TripleTemplate, GraphTermTemplate};

impl JanusQLParser {
    pub(crate) fn extract_baseline_graph_templates(
        &self,
        where_clause: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<Vec<BaselineGraphTemplate>, Box<dyn std::error::Error>> {
        let inner = self.extract_where_inner(where_clause);
        if inner.is_empty() {
            return Ok(Vec::new());
        }

        let mut templates = Vec::new();
        let mut offset = 0usize;

        while let Some(found) = inner[offset..].find("GRAPH") {
            let start = offset + found;
            let after_keyword = start + "GRAPH".len();
            let mut cursor = after_keyword;

            while let Some(ch) = inner[cursor..].chars().next() {
                if ch.is_whitespace() {
                    cursor += ch.len_utf8();
                } else {
                    break;
                }
            }

            let identifier_start = cursor;
            while let Some(ch) = inner[cursor..].chars().next() {
                if ch.is_whitespace() || ch == '{' {
                    break;
                }
                cursor += ch.len_utf8();
            }

            let identifier = inner[identifier_start..cursor].trim();
            while let Some(ch) = inner[cursor..].chars().next() {
                if ch.is_whitespace() {
                    cursor += ch.len_utf8();
                } else {
                    break;
                }
            }

            if !inner[cursor..].starts_with('{') {
                offset = cursor;
                continue;
            }

            let body_start = cursor + 1;
            let Some(body_end) = self.find_matching_brace(&inner, cursor) else {
                return Err(self.parse_error("Unclosed GRAPH block in WHERE clause"));
            };

            let baseline_name = self.unwrap_iri(identifier, prefix_mapper);
            let triples =
                self.parse_graph_template_body(inner[body_start..body_end].trim(), prefix_mapper)?;
            templates.push(BaselineGraphTemplate { baseline_name, triples });
            offset = body_end + 1;
        }

        Ok(templates)
    }

    pub(crate) fn parse_graph_template_body(
        &self,
        body: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<Vec<TripleTemplate>, Box<dyn std::error::Error>> {
        let statements = self.split_graph_template_statements(body);
        let mut triples = Vec::new();

        for statement in statements {
            let tokens = self.tokenize_graph_template_statement(&statement);
            if tokens.is_empty() {
                continue;
            }
            if tokens.len() != 3 {
                return Err(self.parse_error(format!(
                    "GRAPH template triple pattern must contain exactly 3 terms: {statement}"
                )));
            }

            triples.push(TripleTemplate {
                subject: self.parse_graph_term_template(&tokens[0], prefix_mapper),
                predicate: self.parse_graph_term_template(&tokens[1], prefix_mapper),
                object: self.parse_graph_term_template(&tokens[2], prefix_mapper),
            });
        }

        Ok(triples)
    }

    pub(crate) fn split_graph_template_statements(&self, body: &str) -> Vec<String> {
        let mut statements = Vec::new();
        let chars = body.chars().collect::<Vec<_>>();
        let mut current = String::new();
        let mut in_string = false;
        let mut in_iri = false;
        let mut escaped = false;

        for ch in chars {
            if in_string {
                current.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }

            if in_iri {
                current.push(ch);
                if ch == '>' {
                    in_iri = false;
                }
                continue;
            }

            match ch {
                '"' => {
                    in_string = true;
                    current.push(ch);
                }
                '<' => {
                    in_iri = true;
                    current.push(ch);
                }
                '.' => {
                    let trimmed = current.trim();
                    if !trimmed.is_empty() {
                        statements.push(trimmed.to_string());
                    }
                    current.clear();
                }
                _ => current.push(ch),
            }
        }

        let trimmed = current.trim();
        if !trimmed.is_empty() {
            statements.push(trimmed.to_string());
        }

        statements
    }

    pub(crate) fn tokenize_graph_template_statement(&self, statement: &str) -> Vec<String> {
        let chars = statement.chars().collect::<Vec<_>>();
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut index = 0usize;
        let mut in_string = false;
        let mut in_iri = false;
        let mut escaped = false;

        while index < chars.len() {
            let ch = chars[index];

            if in_string {
                current.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                index += 1;
                continue;
            }

            if in_iri {
                current.push(ch);
                if ch == '>' {
                    in_iri = false;
                }
                index += 1;
                continue;
            }

            match ch {
                '"' => {
                    in_string = true;
                    current.push(ch);
                }
                '<' => {
                    in_iri = true;
                    current.push(ch);
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                }
                _ => current.push(ch),
            }

            index += 1;
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        tokens
    }

    pub(crate) fn parse_graph_term_template(
        &self,
        token: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> GraphTermTemplate {
        let trimmed = token.trim();
        if trimmed.starts_with('?') {
            return GraphTermTemplate::Variable(trimmed.trim_start_matches('?').to_string());
        }
        if trimmed.starts_with('"') {
            return GraphTermTemplate::Literal(trimmed.to_string());
        }
        GraphTermTemplate::Iri(self.unwrap_iri(trimmed, prefix_mapper))
    }
}
