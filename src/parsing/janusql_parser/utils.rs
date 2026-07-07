use crate::parsing::janusql_parser::JanusQLParser;
use crate::parsing::janusql_parser::{WhereWindowClause, WindowClause, WindowSpec, WindowType};
use std::collections::HashMap;
use std::collections::HashSet;

impl JanusQLParser {
    pub(crate) fn find_matching_brace(
        &self,
        input: &str,
        open_brace_index: usize,
    ) -> Option<usize> {
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

    pub(crate) fn select_baseline_anchor_variable(
        &self,
        output_variables: &[String],
    ) -> Option<String> {
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

    pub(crate) fn unwrap_iri(
        &self,
        prefixed_iri: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> String {
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

    pub(crate) fn validate_window_declarations(
        &self,
        windows: &[WindowClause],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut seen = HashSet::new();

        for window in windows {
            if !seen.insert(window.window_name.clone()) {
                return Err(self.parse_error(format!(
                    "Window '{}' is declared more than once in the same query",
                    window.window_name
                )));
            }

            match window.spec {
                WindowSpec::LiveSliding { range, step } => {
                    if range == 0 {
                        return Err(self.parse_error(format!(
                            "Live window '{}' must use RANGE greater than 0",
                            window.window_name
                        )));
                    }
                    if step == 0 {
                        return Err(self.parse_error(format!(
                            "Live window '{}' must use STEP greater than 0",
                            window.window_name
                        )));
                    }
                }
                WindowSpec::HistoricalFixed { start, end } => {
                    if start >= end {
                        return Err(self.parse_error(format!(
                            "Historical fixed window '{}' must use START less than END",
                            window.window_name
                        )));
                    }
                }
                WindowSpec::HistoricalSliding { range, step, .. } => {
                    if range == 0 {
                        return Err(self.parse_error(format!(
                            "Historical sliding window '{}' must use RANGE greater than 0",
                            window.window_name
                        )));
                    }
                    if step == 0 {
                        return Err(self.parse_error(format!(
                            "Historical sliding window '{}' must use STEP greater than 0",
                            window.window_name
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) fn validate_where_window_references(
        &self,
        where_windows: &[WhereWindowClause],
        windows: &[WindowClause],
        prefixes: &HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let windows_by_name = windows
            .iter()
            .map(|window| (window.window_name.clone(), window))
            .collect::<HashMap<_, _>>();

        for where_window in where_windows {
            let resolved = self.resolve_window_identifier(
                &where_window.identifier,
                &windows_by_name,
                prefixes,
            );
            if resolved.is_none() {
                return Err(self.parse_error(format!(
                    "WINDOW '{}' references undeclared window '{}'",
                    where_window.identifier, where_window.identifier
                )));
            }
        }

        Ok(())
    }

    pub(crate) fn validate_window_bodies(
        &self,
        where_windows: &[WhereWindowClause],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for where_window in where_windows {
            self.validate_window_body_core_fragment(&where_window.identifier, &where_window.body)?;
        }

        Ok(())
    }

    pub(crate) fn validate_window_body_core_fragment(
        &self,
        window_identifier: &str,
        body: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let uppercase = trimmed.to_ascii_uppercase();
            if uppercase.starts_with("SERVICE ")
                || uppercase.starts_with("SERVICE\t")
                || uppercase.starts_with("SERVICE<")
            {
                return Err(self.parse_error(format!(
                    "WINDOW '{}' uses unsupported SERVICE syntax; Janus-QL Core does not support SERVICE",
                    window_identifier
                )));
            }

            if self.line_uses_property_path(trimmed) {
                return Err(self.parse_error(format!(
                    "WINDOW '{}' uses unsupported property path syntax; Janus-QL Core does not support property paths",
                    window_identifier
                )));
            }
        }

        Ok(())
    }

    fn line_uses_property_path(&self, line: &str) -> bool {
        if line.starts_with("FILTER")
            || line.starts_with('{')
            || line.starts_with('}')
            || line.starts_with('#')
        {
            return false;
        }

        let stripped = line.trim_end_matches('.').trim();
        let tokens = stripped.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 3 {
            return false;
        }

        self.is_property_path_token(tokens[1])
    }

    fn is_property_path_token(&self, token: &str) -> bool {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')');

        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            return false;
        }

        if trimmed.starts_with('?') || trimmed.starts_with('$') {
            return false;
        }

        trimmed.starts_with('^')
            || trimmed.contains('|')
            || trimmed.contains('/')
            || trimmed.ends_with('*')
            || trimmed.ends_with('+')
            || trimmed.ends_with('?')
    }
}
