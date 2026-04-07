use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
/// Different types of windows supported in JanusQL.
pub enum WindowType {
    Live,
    HistoricalSliding,
    HistoricalFixed,
}

#[derive(Debug, Clone)]
/// Definition of a window in JanusQL which is also used for stream processing.
pub struct WindowDefinition {
    /// Name of the window
    pub window_name: String,
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

/// Parsed JanusQL query structure containing all components extracted from the query.
#[derive(Debug, Clone)]
pub struct ParsedJanusQuery {
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

/// Parser for JanusQL queries
pub struct JanusQLParser {
    historical_sliding_window: Regex,
    historical_fixed_window: Regex,
    live_sliding_window: Regex,
    register: Regex,
    prefix: Regex,
}

/// Implement methods for JanusQLParser struct.
impl JanusQLParser {
    /// Creates a new JanusQLParser instance.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(JanusQLParser {
            historical_sliding_window: Regex::new(
                r"FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[OFFSET\s+(\d+)\s+RANGE\s+(\d+)\s+STEP\s+(\d+)\]",
            )?,
            historical_fixed_window: Regex::new(
                r"FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[START\s+(\d+)\s+END\s+(\d+)\]",
            )?,
            live_sliding_window: Regex::new(
                r"FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[RANGE\s+(\d+)\s+STEP\s+(\d+)\]",
            )?,
            register: Regex::new(r"REGISTER\s+(\w+)\s+([^\s]+)\s+AS")?,
            prefix: Regex::new(r"PREFIX\s+([^\s]+):\s*<([^>]+)>")?,
        })
    }

    fn parse_window(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<Option<WindowDefinition>, Box<dyn std::error::Error>> {
        if let Some(captures) = self.historical_sliding_window.captures(line) {
            return Ok(Some(WindowDefinition {
                window_name: self.unwrap_iri(&captures[1], prefix_mapper),
                stream_name: self.unwrap_iri(&captures[2], prefix_mapper),
                offset: Some(captures[3].parse()?),
                width: captures[4].parse()?,
                slide: captures[5].parse()?,
                start: None,
                end: None,
                window_type: WindowType::HistoricalSliding,
            }));
        }

        if let Some(captures) = self.historical_fixed_window.captures(line) {
            return Ok(Some(WindowDefinition {
                window_name: self.unwrap_iri(&captures[1], prefix_mapper),
                stream_name: self.unwrap_iri(&captures[2], prefix_mapper),
                start: Some(captures[3].parse()?),
                end: Some(captures[4].parse()?),
                width: 0,
                slide: 0,
                offset: None,
                window_type: WindowType::HistoricalFixed,
            }));
        }

        if let Some(captures) = self.live_sliding_window.captures(line) {
            return Ok(Some(WindowDefinition {
                window_name: self.unwrap_iri(&captures[1], prefix_mapper),
                stream_name: self.unwrap_iri(&captures[2], prefix_mapper),
                width: captures[3].parse()?,
                slide: captures[4].parse()?,
                offset: None,
                start: None,
                end: None,
                window_type: WindowType::Live,
            }));
        }

        Ok(None)
    }

    /// Parses a JanusQL query string.
    pub fn parse(&self, query: &str) -> Result<ParsedJanusQuery, Box<dyn std::error::Error>> {
        let mut parsed = ParsedJanusQuery {
            r2s: None,
            live_windows: Vec::new(),
            historical_windows: Vec::new(),
            rspql_query: String::new(),
            sparql_queries: Vec::new(),
            prefixes: HashMap::new(),
            where_clause: String::new(),
            select_clause: String::new(),
        };

        let lines: Vec<&str> = query.lines().collect();
        let mut prefix_lines: Vec<String> = Vec::new();
        let mut in_where_clause = false;
        let mut where_lines: Vec<&str> = Vec::new();

        for line in lines {
            let trimmed_line = line.trim();

            if trimmed_line.is_empty()
                || trimmed_line.starts_with("/*")
                || trimmed_line.starts_with('*')
                || trimmed_line.starts_with("*/")
            {
                if in_where_clause && !trimmed_line.is_empty() {
                    where_lines.push(trimmed_line);
                }
                continue;
            }

            if trimmed_line.starts_with("REGISTER") {
                if let Some(captures) = self.register.captures(trimmed_line) {
                    let operator = captures.get(1).unwrap().as_str().to_string();
                    let name_raw = captures.get(2).unwrap().as_str();
                    let name = self.unwrap_iri(name_raw, &parsed.prefixes);
                    parsed.r2s = Some(R2SOperator { operator, name });
                }
            } else if trimmed_line.starts_with("PREFIX") {
                if let Some(captures) = self.prefix.captures(trimmed_line) {
                    let prefix = captures.get(1).unwrap().as_str().to_string();
                    let namespace = captures.get(2).unwrap().as_str().to_string();
                    parsed.prefixes.insert(prefix, namespace);
                    prefix_lines.push(trimmed_line.to_string());
                }
            } else if trimmed_line.starts_with("SELECT") {
                parsed.select_clause = trimmed_line.to_string();
            } else if trimmed_line.starts_with("FROM NAMED WINDOW") {
                if let Some(window) = self.parse_window(trimmed_line, &parsed.prefixes)? {
                    match window.window_type {
                        WindowType::Live => parsed.live_windows.push(window),
                        WindowType::HistoricalSliding | WindowType::HistoricalFixed => {
                            parsed.historical_windows.push(window);
                        }
                    }
                }
            } else if trimmed_line.starts_with("WHERE") {
                in_where_clause = true;
                where_lines.push(line);
            } else if in_where_clause {
                where_lines.push(line);
            }
        }

        parsed.where_clause = where_lines.join("\n");

        if !parsed.live_windows.is_empty() {
            parsed.rspql_query = self.generate_rspql_query(&parsed, &prefix_lines);
        }
        parsed.sparql_queries = self.generate_sparql_queries(&parsed, &prefix_lines);

        Ok(parsed)
    }

    fn generate_rspql_query(&self, parsed: &ParsedJanusQuery, prefix_lines: &[String]) -> String {
        let mut lines: Vec<String> = Vec::new();

        // Add prefixes
        for prefix in prefix_lines {
            lines.push(prefix.clone());
        }

        lines.push(String::new());

        // Adding the R2S Operator
        if let Some(ref r2s) = parsed.r2s {
            let wrapped_name = self.wrap_iri(&r2s.name, &parsed.prefixes);
            lines.push(format!("REGISTER {} {} AS", r2s.operator, wrapped_name));
        }

        if !parsed.select_clause.is_empty() {
            lines.push(parsed.select_clause.clone());
        }

        lines.push(String::new());

        // Adding live windows
        for window in &parsed.live_windows {
            let wrapped_window_name = self.wrap_iri(&window.window_name, &parsed.prefixes);
            let wrapped_stream_name = self.wrap_iri(&window.stream_name, &parsed.prefixes);

            lines.push(format!(
                "FROM NAMED WINDOW {} ON STREAM {} [RANGE {} STEP {}]",
                wrapped_window_name, wrapped_stream_name, window.width, window.slide
            ));
        }

        // Adding WHERE clause with only live window patterns
        if !parsed.where_clause.is_empty() {
            let adapted_where = self.adapt_where_clause_for_live(
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

            // Adding the prefixes.
            for prefix in prefix_lines {
                lines.push(prefix.clone());
            }

            lines.push(String::new());

            // Generate WHERE clause and extract bound variables
            let (where_clause, bound_vars) = self.generate_where_and_extract_vars(
                &parsed.where_clause,
                window,
                &parsed.prefixes,
            );

            // Clean SELECT clause based on bound variables
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
        where_clause: &str,
        window: &WindowDefinition,
        prefixes: &HashMap<String, String>,
    ) -> (String, std::collections::HashSet<String>) {
        let window_uri = &window.window_name;
        let mut prefixed_name = None;
        for (prefix, uri_base) in prefixes {
            if window_uri.starts_with(uri_base) {
                let local_name = &window_uri[uri_base.len()..];
                prefixed_name = Some(format!("{}:{}", prefix, local_name));
                break;
            }
        }

        let window_identifier = prefixed_name.as_ref().unwrap_or(window_uri);
        let window_pattern = format!("WINDOW {} {{", window_identifier);
        let mut bound_vars = std::collections::HashSet::new();

        let where_string = if let Some(start_pos) = where_clause.find(&window_pattern) {
            let after_opening = start_pos + window_pattern.len();
            let mut brace_count = 1;
            let mut end_pos = after_opening;
            let chars: Vec<char> = where_clause[after_opening..].chars().collect();

            for (i, ch) in chars.iter().enumerate() {
                if *ch == '{' {
                    brace_count += 1;
                } else if *ch == '}' {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = after_opening + i;
                        break;
                    }
                }
            }

            let inner_pattern = where_clause[after_opening..end_pos].trim();

            // Extract variables from inner pattern
            let var_regex = Regex::new(r"\?[\w]+").unwrap();
            for cap in var_regex.captures_iter(inner_pattern) {
                bound_vars.insert(cap[0].to_string());
            }

            let stream_uri = self.wrap_iri(&window.stream_name, prefixes);
            format!("WHERE {{\n  GRAPH {} {{\n    {}\n  }}\n}}", stream_uri, inner_pattern)
        } else {
            where_clause.to_string()
        };

        (where_string, bound_vars)
    }

    fn filter_select_clause(
        &self,
        select_clause: &str,
        allowed_vars: &std::collections::HashSet<String>,
    ) -> String {
        if allowed_vars.is_empty() {
            return select_clause.to_string();
        }

        let trimmed = select_clause.trim();
        if !trimmed.to_uppercase().starts_with("SELECT") {
            return select_clause.to_string();
        }

        let content = trimmed[6..].trim();

        // Regex to capture projection items:
        // 1. Aliased expressions: (expression AS ?var) - handle nested parens by matching until AS ?var)
        // 2. Simple variables: ?var
        let item_regex = Regex::new(r"(\(.*?\s+AS\s+\?[\w]+\))|(\?[\w]+)").unwrap();
        let var_regex = Regex::new(r"\?[\w]+").unwrap();

        let mut kept_items = Vec::new();

        for cap in item_regex.captures_iter(content) {
            let item = cap[0].to_string();

            // Check if item uses allowed variables
            let mut is_valid = false;

            // If it's an expression, check if input vars are allowed
            // Note: We check if ANY of the variables inside are bound.
            // For AVG(?a), if ?a is bound, we keep it.
            // If it's a simple var ?a, check if bound.

            let mut vars_in_item = Vec::new();
            for var_cap in var_regex.captures_iter(&item) {
                vars_in_item.push(var_cap[0].to_string());
            }

            // Special case: AS ?alias - the alias is a new variable, not a bound one.
            // But usually expressions are like (AVG(?a) AS ?b). ?a must be bound.
            // We only care about input variables.
            // A simple heuristic: check if at least one variable in the item (excluding the alias if possible) is bound.
            // Since parsing "AS ?alias" is hard with regex, we just check if ANY variable in the item is bound.
            // If the item is just "?alias" (output of previous), it might be tricky if this is a subquery.
            // But here we are filtering the top-level SELECT.

            for var in vars_in_item {
                if allowed_vars.contains(&var) {
                    is_valid = true;
                    break;
                }
            }

            if is_valid {
                kept_items.push(item);
            }
        }

        if kept_items.is_empty() {
            // Fallback: if nothing matches, return original (might fail) or SELECT *
            // Given the issue, returning "SELECT *" might be safer if pattern is not empty,
            // but "SELECT *" is invalid if we have GROUP BY (implied by AVG).
            // Let's return original and hope for best if filtering failed.
            return select_clause.to_string();
        }

        format!("SELECT {}", kept_items.join(" "))
    }

    fn adapt_where_clause_for_live(
        &self,
        where_clause: &str,
        live_windows: &[WindowDefinition],
        prefixes: &HashMap<String, String>,
    ) -> String {
        // Extract patterns for all live windows and combine them
        let mut where_patterns = Vec::new();

        for window in live_windows {
            // Find the window name in prefixed form
            let window_uri = &window.window_name;
            let mut prefixed_name = None;
            for (prefix, uri_base) in prefixes {
                if window_uri.starts_with(uri_base) {
                    let local_name = &window_uri[uri_base.len()..];
                    prefixed_name = Some(format!("{}:{}", prefix, local_name));
                    break;
                }
            }

            let window_identifier = prefixed_name.as_ref().unwrap_or(window_uri);
            let window_pattern = format!("WINDOW {} {{", window_identifier);

            if let Some(start_pos) = where_clause.find(&window_pattern) {
                let after_opening = start_pos + window_pattern.len();

                // Find matching closing brace
                let mut brace_count = 1;
                let mut end_pos = after_opening;
                let chars: Vec<char> = where_clause[after_opening..].chars().collect();

                for (i, ch) in chars.iter().enumerate() {
                    if *ch == '{' {
                        brace_count += 1;
                    } else if *ch == '}' {
                        brace_count -= 1;
                        if brace_count == 0 {
                            end_pos = after_opening + i;
                            break;
                        }
                    }
                }

                let inner_pattern = where_clause[after_opening..end_pos].trim();
                where_patterns
                    .push(format!("WINDOW {} {{\n    {}\n  }}", window_identifier, inner_pattern));
            }
        }

        if where_patterns.is_empty() {
            // Fallback to original
            where_clause.to_string()
        } else {
            // Combine all live window patterns
            format!("WHERE {{\n  {}\n}}", where_patterns.join("\n  "))
        }
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
