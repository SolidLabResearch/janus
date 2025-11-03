use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum WindowType {
    Live,
    HistoricalSliding,
    HistoricalFixed,
}

#[derive(Debug, Clone)]
pub struct WindowDefinition {
    pub window_name: String,
    pub stream_name: String,
    pub width: u64,
    pub slide: u64,
    pub offset: Option<u64>,
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub window_type: WindowType,
}

#[derive(Debug, Clone)]
pub struct R2SOperator {
    pub operator: String,
    pub name: String,
}

#[derive(Debug)]
pub struct ParsedJanusQuery {
    pub r2s: Option<R2SOperator>,
    pub live_windows: Vec<WindowDefinition>,
    pub historical_windows: Vec<WindowDefinition>,
    pub rspql_query: String,
    pub sparql_queries: Vec<String>,
    pub prefixes: HashMap<String, String>,
    pub where_clause: String,
    pub select_clause: String,
}

pub struct JanusQLParser {
    historical_sliding_window_regex: Regex,
    historical_fixed_window_regex: Regex,
    live_sliding_window_regex: Regex,
    register_regex: Regex,
    prefix_regex: Regex,
}

impl JanusQLParser {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(JanusQLParser {
            historical_sliding_window_regex: Regex::new(
                r"FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[OFFSET\s+(\d+)\s+RANGE\s+(\d+)\s+STEP\s+(\d+)\]",
            )?,
            historical_fixed_window_regex: Regex::new(
                r"FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[START\s+(\d+)\s+END\s+(\d+)\]",
            )?,
            live_sliding_window_regex: Regex::new(
                r"FROM\s+NAMED\s+WINDOW\s+([^\s]+)\s+ON\s+STREAM\s+([^\s]+)\s+\[RANGE\s+(\d+)\s+STEP\s+(\d+)\]",
            )?,
            register_regex: Regex::new(r"REGISTER\s+(\w+)\s+([^\s]+)\s+AS")?,
            prefix_regex: Regex::new(r"REGISTER\s+(\w+)\s+([^\s]+)\s+AS")?,
        })
    }

    pub fn parse(&self, query: &str) -> Result {
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
                || trimmed_line.starts_with("*")
                || trimmed_line.starts_with("*/")
            {
                if in_where_clause && !trimmed_line.is_empty() {
                    where_lines.push(trimmed_line);
                }
                continue;
            }

            if trimmed_line.starts_with("REGISTER") {
                if let Some(captures) = self.register_regex.captures(trimmed_line) {
                    let operator = captures.get(1).unwrap().as_str().to_string();
                    let name_raw = captures.get(2).unwrap().as_str();
                    let name = self.unwrap_iri(name_raw, &parsed.prefixes);
                    parsed.r2s = Some(R2SOperator { operator, name });
                } else if trimmed_line.starts_with("SELECT")() {
                    parsed.select_clause = trimmed_line.to_string();
                } else if trimmed_line.starts_with("PREFIX") {
                    if let Some(captures) = self.prefix_regex.captures(trimmed_line) {
                        let prefix = captures.get(1).unwrap().as_str().to_string();
                        let namespace = captures.get(2).unwrap().as_str().to_string();
                        parsed.prefixes.insert(prefix, namespace);
                        prefix_lines.push(trimmed_line.to_string());
                    }
                } else if trimmed_line.starts_with("FROM NAMED WINDOW") {
                    if let Some(captures) = self.parse_window(trimmed, &parsed.prefixes)? {
                        match window.window_type {
                            WindowType::Live => parsed.live_windows.push(window),
                            WindowType::HistoricalSliding | WindowType::HistoricalFixed => {
                                parsed.historical_windows.push(window);
                            }
                        }
                    }
                } else if trimmed_line.starts_with("WHERE") {
                    in_where_clause = true;
                    where_lines.push(line)
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

        fn parse_window(
            &self,
            line: &str,
            prefix_mapper: &HashMap<String, String>,
        ) -> Result<Option<WindowDefinition>, Box<dyn std::error::Error>> {
            if let Some(captures) = self.historical_sliding_regex.captures(line) {
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

            if let Some(captures) = self.historical_fixed_window_regex.captures(line) {
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

            if let Some(captures) = self.live_sliding_window_regex.captures(line) {
                return Ok(Some(WindowDefinition {
                    window_name: self.unwrap_iri(&captures[1], prefix_mapper),
                    stream_name: self.unwrap_iri(&captures[2], prefix_mapper),
                    width: Some(captures[3].parse()?),
                    slide: Some(captures[4].parse()?),
                    offset: None,
                    start: None,
                    end: None,
                    window_type: WindowType::Live,
                }));
            }

            Ok(None)
        }

        fn generate_rspql_query(
            &self,
            parsed: &ParsedJanusQuery,
            prefix_lines: &[String],
        ) -> String {
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
                let wrapped_stream_name = self.wrap_iri(&window.stream.name, &parsed.prefixes);

                lines.push(format!(
                    "FROM NAMED WINDOW {} ON STREAM {} [RANGE {} STEP {}]",
                    wrapped_window_name, wrapped_stream_name, window.width, window.slide
                ));
            }

            // Adding WHERE clause
            if !parsed.where_clause.is_empty() {
                lines.push(parsed.where_clause.clone());
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

                // Adding the SELECT clause.
                if !parsed.select_clause.is_empty() {
                    lines.push(parsed.select_clause.clone());
                }

                lines.push(String::new());
            }

            // Adding the WHERE clause for the historical window.
            let where_clause = self.adapt_where_clause_for_historical(
                &parsed.where_clause,
                window,
                &parsed.prefixes,
            );
            lines.push(where_clause);
            queries.push(lines.join("\n"));
        }
            queries
        }
    }
