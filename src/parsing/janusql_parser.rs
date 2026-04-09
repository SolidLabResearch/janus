use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
/// Different types of windows supported in JanusQL.
pub enum WindowType {
    Live,
    HistoricalSliding,
    HistoricalFixed,
}

#[derive(Debug, Clone, PartialEq)]
/// Source kinds supported in JanusQL window clauses.
pub enum SourceKind {
    /// Real-time stream source.
    Stream,
    /// Historical log or store source.
    Log,
}

#[derive(Debug, Clone)]
/// Definition of a window in JanusQL which is also used for stream processing.
pub struct WindowDefinition {
    /// Name of the window
    pub window_name: String,
    /// Source kind used by the window clause.
    pub source_kind: SourceKind,
    /// Name of the stream
    pub stream_name: String,
    /// Width of the window
    pub width: u64,
    /// Slide step
    pub slide: u64,
    /// Offset for sliding windows
    pub offset: Option<u64>,
    /// Start time for fixed windows
    pub start: Option<u64>,
    /// End time for fixed windows
    pub end: Option<u64>,
    /// Type of the window
    pub window_type: WindowType,
}

/// R2S operator definition which does the relation to stream conversion by executing a SPARQL query
/// parsed from the JanusQL query on top of the defined windows to create a stream output result.
#[derive(Debug, Clone)]
pub struct R2SOperator {
    /// Operator type
    pub operator: String,
    /// Operator name
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
/// Prefix declaration captured from JanusQL.
pub struct PrefixDeclaration {
    pub prefix: String,
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq)]
/// REGISTER clause captured from JanusQL.
pub struct RegisterClause {
    pub operator: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineBootstrapMode {
    Last,
    Aggregate,
}

impl Default for BaselineBootstrapMode {
    fn default() -> Self {
        Self::Aggregate
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Structured window specification used by the AST.
pub enum WindowSpec {
    LiveSliding { range: u64, step: u64 },
    HistoricalSliding { offset: u64, range: u64, step: u64 },
    HistoricalFixed { start: u64, end: u64 },
}

#[derive(Debug, Clone, PartialEq)]
/// Structured `FROM NAMED WINDOW` clause in the AST.
pub struct WindowClause {
    pub window_name: String,
    pub source_kind: SourceKind,
    pub source_name: String,
    pub spec: WindowSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaselineClause {
    pub window_name: String,
    pub mode: BaselineBootstrapMode,
}

#[derive(Debug, Clone, PartialEq)]
/// One parsed `WINDOW foo { ... }` block from the WHERE clause.
pub struct WhereWindowClause {
    pub identifier: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
/// Abstract syntax tree for a JanusQL query.
pub struct JanusQueryAst {
    pub prefixes: Vec<PrefixDeclaration>,
    pub register: Option<RegisterClause>,
    pub baseline: Option<BaselineClause>,
    pub select_clause: String,
    pub windows: Vec<WindowClause>,
    pub where_clause: String,
    pub where_windows: Vec<WhereWindowClause>,
}

/// Parsed JanusQL query structure containing all components extracted from the query.
#[derive(Debug, Clone)]
pub struct ParsedJanusQuery {
    /// Structured AST representation of the parsed JanusQL query.
    pub ast: JanusQueryAst,
    /// Optional baseline clause selecting a historical window and bootstrap mode.
    pub baseline: Option<BaselineClause>,
    /// R2S operator if present
    pub r2s: Option<R2SOperator>,
    /// Live windows defined in the query
    pub live_windows: Vec<WindowDefinition>,
    /// Historical windows defined in the query
    pub historical_windows: Vec<WindowDefinition>,
    /// RSPQL query string
    pub rspql_query: String,
    /// SPARQL queries
    pub sparql_queries: Vec<String>,
    /// Prefix mappings
    pub prefixes: HashMap<String, String>,
    /// WHERE clause
    pub where_clause: String,
    /// SELECT clause
    pub select_clause: String,
}

/// Parser for JanusQL queries.
pub struct JanusQLParser;

impl JanusQLParser {
    /// Creates a new JanusQLParser instance.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self)
    }

    /// Parse JanusQL into an explicit AST without regex-based clause matching.
    pub fn parse_ast(&self, query: &str) -> Result<JanusQueryAst, Box<dyn std::error::Error>> {
        let mut prefixes = Vec::new();
        let mut prefix_mapper = HashMap::new();
        let mut register = None;
        let mut baseline = None;
        let mut select_clause = String::new();
        let mut windows = Vec::new();
        let mut in_where_clause = false;
        let mut where_lines: Vec<&str> = Vec::new();
        let lines = query.lines().collect::<Vec<_>>();
        let mut index = 0;

        while index < lines.len() {
            let line = lines[index];
            let trimmed_line = line.trim();

            if trimmed_line.is_empty()
                || trimmed_line.starts_with("/*")
                || trimmed_line.starts_with('*')
                || trimmed_line.starts_with("*/")
            {
                if in_where_clause && !trimmed_line.is_empty() {
                    where_lines.push(trimmed_line);
                }
                index += 1;
                continue;
            }

            if trimmed_line.starts_with("REGISTER") {
                register = Some(self.parse_register_clause(trimmed_line, &prefix_mapper)?);
            } else if trimmed_line.starts_with("USING BASELINE") {
                baseline = Some(self.parse_baseline_clause(trimmed_line, &prefix_mapper)?);
            } else if trimmed_line.starts_with("PREFIX") {
                let prefix = self.parse_prefix_declaration(trimmed_line)?;
                prefix_mapper.insert(prefix.prefix.clone(), prefix.namespace.clone());
                prefixes.push(prefix);
            } else if trimmed_line.starts_with("SELECT") {
                select_clause = trimmed_line.to_string();
            } else if trimmed_line.starts_with("FROM NAMED WINDOW") {
                let mut clause = trimmed_line.to_string();
                while !clause.contains(']') && index + 1 < lines.len() {
                    index += 1;
                    clause.push(' ');
                    clause.push_str(lines[index].trim());
                }
                windows.push(self.parse_window_clause(&clause, &prefix_mapper)?);
            } else if trimmed_line.starts_with("WHERE") {
                in_where_clause = true;
                where_lines.push(line);
            } else if in_where_clause {
                where_lines.push(line);
            }

            index += 1;
        }

        let where_clause = where_lines.join("\n");
        let where_windows = self.extract_where_windows(&where_clause);

        Ok(JanusQueryAst {
            prefixes,
            register,
            baseline,
            select_clause,
            windows,
            where_clause,
            where_windows,
        })
    }

    /// Parses a JanusQL query string.
    pub fn parse(&self, query: &str) -> Result<ParsedJanusQuery, Box<dyn std::error::Error>> {
        let ast = self.parse_ast(query)?;
        let prefixes = ast
            .prefixes
            .iter()
            .map(|prefix| (prefix.prefix.clone(), prefix.namespace.clone()))
            .collect::<HashMap<_, _>>();
        let prefix_lines = ast
            .prefixes
            .iter()
            .map(|prefix| format!("PREFIX {}: <{}>", prefix.prefix, prefix.namespace))
            .collect::<Vec<_>>();

        let mut live_windows = Vec::new();
        let mut historical_windows = Vec::new();

        for window in &ast.windows {
            let definition = self.lower_window_clause(window);
            match definition.window_type {
                WindowType::Live => live_windows.push(definition),
                WindowType::HistoricalSliding | WindowType::HistoricalFixed => {
                    historical_windows.push(definition);
                }
            }
        }

        let r2s = ast
            .register
            .clone()
            .map(|register| R2SOperator { operator: register.operator, name: register.name });

        if let Some(baseline) = &ast.baseline {
            let has_matching_historical_window = historical_windows
                .iter()
                .any(|window| window.window_name == baseline.window_name);
            if !has_matching_historical_window {
                return Err(self.parse_error(format!(
                    "USING BASELINE references unknown historical window '{}'",
                    baseline.window_name
                )));
            }
        }

        let mut parsed = ParsedJanusQuery {
            ast: ast.clone(),
            baseline: ast.baseline.clone(),
            r2s,
            live_windows,
            historical_windows,
            rspql_query: String::new(),
            sparql_queries: Vec::new(),
            prefixes,
            where_clause: ast.where_clause.clone(),
            select_clause: ast.select_clause.clone(),
        };

        if !parsed.live_windows.is_empty() {
            parsed.rspql_query = self.generate_rspql_query(&parsed, &prefix_lines);
        }
        parsed.sparql_queries = self.generate_sparql_queries(&parsed, &prefix_lines);

        Ok(parsed)
    }

    fn parse_baseline_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<BaselineClause, Box<dyn std::error::Error>> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "USING" || parts[1] != "BASELINE" {
            return Err(self.parse_error(format!("Invalid USING BASELINE clause: {line}")));
        }

        let mode = match parts[3] {
            "LAST" => BaselineBootstrapMode::Last,
            "AGGREGATE" => BaselineBootstrapMode::Aggregate,
            other => {
                return Err(self.parse_error(format!(
                    "Unsupported baseline mode '{other}'. Use LAST or AGGREGATE"
                )))
            }
        };

        Ok(BaselineClause { window_name: self.unwrap_iri(parts[2], prefix_mapper), mode })
    }

    fn parse_register_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<RegisterClause, Box<dyn std::error::Error>> {
        let rest = line
            .strip_prefix("REGISTER")
            .ok_or_else(|| self.parse_error("REGISTER clause must start with REGISTER"))?
            .trim();
        let parts = rest.split_whitespace().collect::<Vec<_>>();

        if parts.len() != 3 || parts[2] != "AS" {
            return Err(self.parse_error(format!("Invalid REGISTER clause: {line}")));
        }

        Ok(RegisterClause {
            operator: parts[0].to_string(),
            name: self.unwrap_iri(parts[1], prefix_mapper),
        })
    }

    fn parse_prefix_declaration(
        &self,
        line: &str,
    ) -> Result<PrefixDeclaration, Box<dyn std::error::Error>> {
        let rest = line
            .strip_prefix("PREFIX")
            .ok_or_else(|| self.parse_error("PREFIX clause must start with PREFIX"))?
            .trim();
        let (prefix, namespace) = rest
            .split_once(':')
            .ok_or_else(|| self.parse_error(format!("Invalid PREFIX clause: {line}")))?;
        let namespace = namespace.trim();

        if !namespace.starts_with('<') || !namespace.ends_with('>') {
            return Err(self.parse_error(format!(
                "PREFIX namespace must be enclosed in angle brackets: {line}"
            )));
        }

        Ok(PrefixDeclaration {
            prefix: prefix.trim().to_string(),
            namespace: namespace[1..namespace.len() - 1].to_string(),
        })
    }

    fn parse_window_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<WindowClause, Box<dyn std::error::Error>> {
        let (header, spec) = line
            .split_once('[')
            .ok_or_else(|| self.parse_error(format!("Missing window spec in clause: {line}")))?;
        let spec = spec
            .trim()
            .strip_suffix(']')
            .ok_or_else(|| self.parse_error(format!("Window spec must end with ']': {line}")))?;
        let header_parts = header.split_whitespace().collect::<Vec<_>>();

        if header_parts.len() != 7
            || header_parts[0] != "FROM"
            || header_parts[1] != "NAMED"
            || header_parts[2] != "WINDOW"
            || header_parts[4] != "ON"
        {
            return Err(self.parse_error(format!("Invalid window clause header: {line}")));
        }

        let source_kind = self.parse_source_kind(header_parts[5])?;
        let window_name = self.unwrap_iri(header_parts[3], prefix_mapper);
        let source_name = self.unwrap_iri(header_parts[6], prefix_mapper);
        let spec_parts = spec.split_whitespace().collect::<Vec<_>>();
        let spec = match spec_parts.as_slice() {
            ["RANGE", range, "STEP", step] => {
                if source_kind != SourceKind::Stream {
                    return Err(self.parse_error(
                        "Live RANGE/STEP windows are only supported on STREAM sources",
                    ));
                }
                WindowSpec::LiveSliding { range: range.parse()?, step: step.parse()? }
            }
            ["OFFSET", offset, "RANGE", range, "STEP", step] => WindowSpec::HistoricalSliding {
                offset: offset.parse()?,
                range: range.parse()?,
                step: step.parse()?,
            },
            ["START", start, "END", end] => {
                WindowSpec::HistoricalFixed { start: start.parse()?, end: end.parse()? }
            }
            _ => {
                return Err(self.parse_error(format!("Unsupported window specification: [{spec}]")));
            }
        };

        Ok(WindowClause { window_name, source_kind, source_name, spec })
    }

    fn parse_source_kind(&self, raw: &str) -> Result<SourceKind, Box<dyn std::error::Error>> {
        match raw {
            "STREAM" => Ok(SourceKind::Stream),
            "LOG" => Ok(SourceKind::Log),
            _ => Err(self.parse_error(format!("Unsupported source kind: {raw}"))),
        }
    }

    fn lower_window_clause(&self, window: &WindowClause) -> WindowDefinition {
        match window.spec {
            WindowSpec::LiveSliding { range, step } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                stream_name: window.source_name.clone(),
                width: range,
                slide: step,
                offset: None,
                start: None,
                end: None,
                window_type: WindowType::Live,
            },
            WindowSpec::HistoricalSliding { offset, range, step } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                stream_name: window.source_name.clone(),
                width: range,
                slide: step,
                offset: Some(offset),
                start: None,
                end: None,
                window_type: WindowType::HistoricalSliding,
            },
            WindowSpec::HistoricalFixed { start, end } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                stream_name: window.source_name.clone(),
                width: 0,
                slide: 0,
                offset: None,
                start: Some(start),
                end: Some(end),
                window_type: WindowType::HistoricalFixed,
            },
        }
    }

    fn generate_rspql_query(&self, parsed: &ParsedJanusQuery, prefix_lines: &[String]) -> String {
        let mut lines: Vec<String> = Vec::new();

        for prefix in prefix_lines {
            lines.push(prefix.clone());
        }

        lines.push(String::new());

        if let Some(ref r2s) = parsed.r2s {
            let wrapped_name = self.wrap_iri(&r2s.name, &parsed.prefixes);
            lines.push(format!("REGISTER {} {} AS", r2s.operator, wrapped_name));
        }

        if !parsed.select_clause.is_empty() {
            lines.push(parsed.select_clause.clone());
        }

        lines.push(String::new());

        for window in &parsed.live_windows {
            let wrapped_window_name = self.wrap_iri(&window.window_name, &parsed.prefixes);
            let wrapped_stream_name = self.wrap_iri(&window.stream_name, &parsed.prefixes);

            lines.push(format!(
                "FROM NAMED WINDOW {} ON STREAM {} [RANGE {} STEP {}]",
                wrapped_window_name, wrapped_stream_name, window.width, window.slide
            ));
        }

        if !parsed.where_clause.is_empty() {
            let adapted_where = self.adapt_where_clause_for_live(
                &parsed.ast.where_windows,
                &parsed.where_clause,
                &parsed.live_windows,
                &parsed.prefixes,
            );
            lines.push(adapted_where);
        }

        lines.join("\n")
    }

    fn generate_sparql_queries(
        &self,
        parsed: &ParsedJanusQuery,
        prefix_lines: &[String],
    ) -> Vec<String> {
        let mut queries = Vec::new();

        for window in &parsed.historical_windows {
            let mut lines: Vec<String> = Vec::new();

            for prefix in prefix_lines {
                lines.push(prefix.clone());
            }

            lines.push(String::new());

            let (where_clause, bound_vars) = self.generate_where_and_extract_vars(
                &parsed.ast.where_windows,
                &parsed.where_clause,
                window,
                &parsed.prefixes,
            );

            if !parsed.select_clause.is_empty() {
                let clean_select = self.filter_select_clause(&parsed.select_clause, &bound_vars);
                lines.push(clean_select);
            }

            lines.push(String::new());
            lines.push(where_clause);
            queries.push(lines.join("\n"));
        }

        queries
    }

    fn generate_where_and_extract_vars(
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

    fn filter_select_clause(&self, select_clause: &str, allowed_vars: &HashSet<String>) -> String {
        if allowed_vars.is_empty() {
            return select_clause.to_string();
        }

        let trimmed = select_clause.trim();
        if !trimmed.to_uppercase().starts_with("SELECT") {
            return select_clause.to_string();
        }

        let content = trimmed[6..].trim();
        let projection_items = self.extract_projection_items(content);
        let mut kept_items = Vec::new();

        for item in projection_items {
            let vars_in_item = self.extract_variables(&item);
            if vars_in_item.iter().any(|var| allowed_vars.contains(var)) {
                kept_items.push(item);
            }
        }

        if kept_items.is_empty() {
            return select_clause.to_string();
        }

        format!("SELECT {}", kept_items.join(" "))
    }

    fn adapt_where_clause_for_live(
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

    fn extract_non_window_where_patterns(&self, where_clause: &str) -> String {
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

    fn extract_where_inner(&self, where_clause: &str) -> String {
        let trimmed = where_clause.trim();
        let without_where = trimmed
            .strip_prefix("WHERE")
            .or_else(|| trimmed.strip_prefix("where"))
            .map_or(trimmed, str::trim);

        if without_where.starts_with('{') {
            if let Some(end) = self.find_matching_brace(without_where, 0) {
                if end == without_where.len() - 1 {
                    return without_where[1..end].trim().to_string();
                }
            }
        }

        without_where.to_string()
    }

    fn find_window_body<'a>(
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

    fn extract_where_windows(&self, where_clause: &str) -> Vec<WhereWindowClause> {
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

    fn find_matching_brace(&self, input: &str, open_brace_index: usize) -> Option<usize> {
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

    fn extract_variables(&self, input: &str) -> Vec<String> {
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

    fn extract_projection_items(&self, input: &str) -> Vec<String> {
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

    fn local_name<'a>(&self, iri: &'a str) -> Option<&'a str> {
        iri.rsplit(['#', '/']).next().filter(|local| !local.is_empty())
    }

    fn parse_error(&self, message: impl Into<String>) -> Box<dyn std::error::Error> {
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into()))
    }

    fn unwrap_iri(&self, prefixed_iri: &str, prefix_mapper: &HashMap<String, String>) -> String {
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

    fn wrap_iri(&self, iri: &str, prefixes: &HashMap<String, String>) -> String {
        for (prefix, namespace) in prefixes {
            if iri.starts_with(namespace) {
                let local_part = &iri[namespace.len()..];
                return format!("{}:{}", prefix, local_part);
            }
        }
        format!("<{}>", iri)
    }
}

impl Default for JanusQLParser {
    fn default() -> Self {
        Self::new().expect("Failed to create JanusQLParser")
    }
}
