use std::collections::{HashMap, HashSet};
use crate::parsing::janusql_parser::JanusQLParser;
use crate::parsing::janusql_parser::ast::{WhereWindowClause, WindowDefinition, SourceKind};

impl JanusQLParser {
    pub(crate) fn adapt_where_clause_for_live(
        &self,
        where_windows: &[WhereWindowClause],
        where_clause: &str,
        live_windows: &[WindowDefinition],
        prefixes: &HashMap<String, String>,
    ) -> String {
        let mut where_patterns = Vec::new();
        let non_window_patterns = self.extract_non_window_where_patterns(where_clause);

        if !non_window_patterns.is_empty() {
            where_patterns.push(non_window_patterns);
        }

        for window in live_windows {
            if let Some(inner_pattern) = self.find_window_body(where_windows, window, prefixes) {
                let window_identifier = self.wrap_iri(&window.window_name, prefixes);
                where_patterns
                    .push(format!("WINDOW {} {{\n    {}\n  }}", window_identifier, inner_pattern));
            }
        }

        if where_patterns.is_empty() {
            where_clause.to_string()
        } else {
            format!("WHERE {{\n  {}\n}}", where_patterns.join("\n  "))
        }
    }

    pub(crate) fn extract_non_window_where_patterns(&self, where_clause: &str) -> String {
        let inner = self.extract_where_inner(where_clause);
        if inner.is_empty() {
            return String::new();
        }

        let mut preserved = String::new();
        let mut offset = 0usize;

        while let Some(found) = inner[offset..].find("WINDOW") {
            let start = offset + found;
            preserved.push_str(&inner[offset..start]);

            let after_keyword = start + "WINDOW".len();
            let mut cursor = after_keyword;

            while let Some(ch) = inner[cursor..].chars().next() {
                if ch.is_whitespace() {
                    cursor += ch.len_utf8();
                } else {
                    break;
                }
            }

            while let Some(ch) = inner[cursor..].chars().next() {
                if ch.is_whitespace() || ch == '{' {
                    break;
                }
                cursor += ch.len_utf8();
            }

            while let Some(ch) = inner[cursor..].chars().next() {
                if ch.is_whitespace() {
                    cursor += ch.len_utf8();
                } else {
                    break;
                }
            }

            if !inner[cursor..].starts_with('{') {
                preserved.push_str("WINDOW");
                offset = after_keyword;
                continue;
            }

            let Some(body_end) = self.find_matching_brace(&inner, cursor) else {
                preserved.push_str(&inner[start..]);
                offset = inner.len();
                break;
            };

            offset = body_end + 1;
        }

        if offset < inner.len() {
            preserved.push_str(&inner[offset..]);
        }

        preserved
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n  ")
    }

    pub(crate) fn extract_where_inner(&self, where_clause: &str) -> String {
        let trimmed = where_clause.trim();
        let without_where = trimmed
            .strip_prefix("WHERE")
            .or_else(|| trimmed.strip_prefix("where"))
            .map_or(trimmed, str::trim);

        if without_where.starts_with('{') {
            if let Some(end) = self.find_matching_brace(without_where, 0) {
                return without_where[1..end].trim().to_string();
            }
        }

        without_where.to_string()
    }

    pub(crate) fn find_window_body<'a>(
        &self,
        where_windows: &'a [WhereWindowClause],
        window: &WindowDefinition,
        prefixes: &HashMap<String, String>,
    ) -> Option<&'a str> {
        let mut candidates = Vec::new();
        let wrapped = self.wrap_iri(&window.window_name, prefixes);
        candidates.push(wrapped.clone());
        candidates.push(window.window_name.clone());

        if let Some(local) = self.local_name(&window.window_name) {
            candidates.push(format!(":{}", local));
        }

        where_windows
            .iter()
            .find(|clause| candidates.iter().any(|candidate| candidate == &clause.identifier))
            .map(|clause| clause.body.as_str())
    }

    pub(crate) fn extract_where_windows(&self, where_clause: &str) -> Vec<WhereWindowClause> {
        let mut clauses = Vec::new();
        let mut offset = 0;

        while let Some(found) = where_clause[offset..].find("WINDOW") {
            let start = offset + found;
            let after_keyword = start + "WINDOW".len();
            let mut cursor = after_keyword;

            while let Some(ch) = where_clause[cursor..].chars().next() {
                if ch.is_whitespace() {
                    cursor += ch.len_utf8();
                } else {
                    break;
                }
            }

            let identifier_start = cursor;
            while let Some(ch) = where_clause[cursor..].chars().next() {
                if ch.is_whitespace() || ch == '{' {
                    break;
                }
                cursor += ch.len_utf8();
            }

            let identifier = where_clause[identifier_start..cursor].trim().to_string();
            while let Some(ch) = where_clause[cursor..].chars().next() {
                if ch.is_whitespace() {
                    cursor += ch.len_utf8();
                } else {
                    break;
                }
            }

            if !where_clause[cursor..].starts_with('{') {
                offset = cursor;
                continue;
            }

            let body_start = cursor + 1;
            let Some(body_end) = self.find_matching_brace(where_clause, cursor) else {
                break;
            };

            clauses.push(WhereWindowClause {
                identifier,
                body: where_clause[body_start..body_end].trim().to_string(),
            });
            offset = body_end + 1;
        }

        clauses
    }

    pub(crate) fn generate_where_and_extract_vars(
        &self,
        where_windows: &[WhereWindowClause],
        where_clause: &str,
        window: &WindowDefinition,
        prefixes: &HashMap<String, String>,
    ) -> (String, HashSet<String>) {
        let mut bound_vars = HashSet::new();

        let where_string = if let Some(inner_pattern) =
            self.find_window_body(where_windows, window, prefixes)
        {
            for variable in self.extract_variables(inner_pattern) {
                bound_vars.insert(variable);
            }

            match window.source_kind {
                SourceKind::Log => {
                    format!(
                        "WHERE {{\n  GRAPH ?__janus_log_graph {{\n    {}\n  }}\n}}",
                        inner_pattern
                    )
                }
                SourceKind::Stream => {
                    let stream_uri = self.wrap_iri(&window.stream_name, prefixes);
                    format!("WHERE {{\n  GRAPH {} {{\n    {}\n  }}\n}}", stream_uri, inner_pattern)
                }
            }
        } else {
            where_clause.to_string()
        };

        (where_string, bound_vars)
    }
}
