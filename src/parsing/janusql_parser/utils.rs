use std::collections::HashMap;
use crate::parsing::janusql_parser::JanusQLParser;

impl JanusQLParser {
    pub(crate) fn find_matching_brace(&self, input: &str, open_brace_index: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (relative_index, ch) in input[open_brace_index..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open_brace_index + relative_index);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn brace_balance(&self, input: &str) -> isize {
        input.chars().fold(0isize, |depth, ch| match ch {
            '{' => depth + 1,
            '}' => depth - 1,
            _ => depth,
        })
    }

    pub(crate) fn extract_variables(&self, input: &str) -> Vec<String> {
        let mut variables = Vec::new();
        let chars = input.chars().collect::<Vec<_>>();
        let mut index = 0;

        while index < chars.len() {
            if chars[index] == '?' {
                let start = index;
                index += 1;
                while index < chars.len()
                    && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
                {
                    index += 1;
                }
                if index > start + 1 {
                    variables.push(chars[start..index].iter().collect());
                    continue;
                }
            }
            index += 1;
        }

        variables
    }

    pub(crate) fn extract_projection_items(&self, input: &str) -> Vec<String> {
        let chars = input.chars().collect::<Vec<_>>();
        let mut items = Vec::new();
        let mut index = 0;

        while index < chars.len() {
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }

            if index >= chars.len() {
                break;
            }

            if chars[index] == '(' {
                let start = index;
                let mut depth = 0usize;
                while index < chars.len() {
                    match chars[index] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                index += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    index += 1;
                }
                items.push(chars[start..index].iter().collect::<String>());
            } else {
                let start = index;
                while index < chars.len() && !chars[index].is_whitespace() {
                    index += 1;
                }
                items.push(chars[start..index].iter().collect::<String>());
            }
        }

        items
    }

    pub(crate) fn extract_output_variables(&self, select_clause: &str) -> Vec<String> {
        let trimmed = select_clause.trim();
        let content = trimmed
            .strip_prefix("SELECT")
            .or_else(|| trimmed.strip_prefix("select"))
            .map_or(trimmed, str::trim);

        self.extract_projection_items(content)
            .into_iter()
            .filter_map(|item| {
                let trimmed_item = item.trim();
                if trimmed_item.starts_with('?') {
                    Some(trimmed_item.to_string())
                } else if let Some(as_pos) = trimmed_item.rfind(" AS ") {
                    let alias = trimmed_item[as_pos + 4..].trim().trim_end_matches(')').trim();
                    if alias.starts_with('?') {
                        Some(alias.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn select_baseline_anchor_variable(&self, output_variables: &[String]) -> Option<String> {
        for preferred in ["?sensor", "?subject", "?entity", "?s"] {
            if output_variables.iter().any(|variable| variable == preferred) {
                return Some(preferred.to_string());
            }
        }

        output_variables.first().cloned()
    }

    pub(crate) fn local_name<'a>(&self, iri: &'a str) -> Option<&'a str> {
        iri.rsplit(['#', '/']).next().filter(|local| !local.is_empty())
    }

    pub(crate) fn parse_error(&self, message: impl Into<String>) -> Box<dyn std::error::Error> {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into()))
    }

    pub(crate) fn unwrap_iri(&self, prefixed_iri: &str, prefix_mapper: &HashMap<String, String>) -> String {
        let trimmed = prefixed_iri.trim();

        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }

        if let Some(colon_pos) = trimmed.find(':') {
            let prefix = &trimmed[..colon_pos];
            let local_part = &trimmed[colon_pos + 1..];
            if let Some(namespace) = prefix_mapper.get(prefix) {
                return format!("{}{}", namespace, local_part);
            }
        }

        trimmed.to_string()
    }

    pub(crate) fn wrap_iri(&self, iri: &str, prefixes: &HashMap<String, String>) -> String {
        for (prefix, namespace) in prefixes {
            if iri.starts_with(namespace) {
                let local_part = &iri[namespace.len()..];
                return format!("{}:{}", prefix, local_part);
            }
        }
        format!("<{}>", iri)
    }
}
