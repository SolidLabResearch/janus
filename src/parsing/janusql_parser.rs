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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Explicit historical window specification for deterministic bound resolution.
pub enum HistoricalWindowSpec {
    /// Absolute historical interval reused for every live evaluation.
    Fixed { start: u64, end: u64 },
    /// Historical interval resolved relative to a live evaluation timestamp.
    Sliding { offset: u64, range: u64, step: u64 },
}

impl WindowDefinition {
    /// Returns the historical window specification for historical windows.
    pub fn historical_window_spec(&self) -> Option<HistoricalWindowSpec> {
        match self.window_type {
            WindowType::HistoricalFixed => {
                Some(HistoricalWindowSpec::Fixed { start: self.start?, end: self.end? })
            }
            WindowType::HistoricalSliding => Some(HistoricalWindowSpec::Sliding {
                offset: self.offset?,
                range: self.width,
                step: self.slide,
            }),
            WindowType::Live => None,
        }
    }

    /// Resolves the historical bounds for this window at the provided evaluation timestamp.
    pub fn resolve_historical_bounds(&self, evaluation_time: u64) -> Option<(u64, u64)> {
        match self.historical_window_spec()? {
            HistoricalWindowSpec::Fixed { start, end } => Some((start, end)),
            HistoricalWindowSpec::Sliding { offset, range, .. } => {
                let historical_end = evaluation_time.saturating_sub(offset);
                let historical_start = historical_end.saturating_sub(range);
                Some((historical_start, historical_end))
            }
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaselineBootstrapMode {
    Last,
    #[default]
    Aggregate,
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
pub struct BaselineDefinition {
    pub name: String,
    pub source_window: String,
    pub raw_query: String,
    pub select_clause: String,
    pub where_clause: String,
    pub group_by_clause: Option<String>,
    pub output_variables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaselineUse {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedBaselineQuery {
    pub name: String,
    pub source_window: String,
    pub sparql_query: String,
    pub output_variables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
/// One parsed `WINDOW foo { ... }` block from the WHERE clause.
pub struct WhereWindowClause {
    pub identifier: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphTermTemplate {
    Variable(String),
    Iri(String),
    Literal(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TripleTemplate {
    pub subject: GraphTermTemplate,
    pub predicate: GraphTermTemplate,
    pub object: GraphTermTemplate,
}

#[derive(Debug, Clone, PartialEq)]
/// Explicit `GRAPH :baselineName { ... }` template extracted from the live query.
pub struct BaselineGraphTemplate {
    /// Named graph IRI for the baseline materialization target.
    pub baseline_name: String,
    pub triples: Vec<TripleTemplate>,
}

#[derive(Debug, Clone, PartialEq)]
/// Abstract syntax tree for a JanusQL query.
pub struct JanusQueryAst {
    pub prefixes: Vec<PrefixDeclaration>,
    pub register: Option<RegisterClause>,
    pub baseline: Option<BaselineClause>,
    pub baseline_definitions: Vec<BaselineDefinition>,
    pub baseline_uses: Vec<BaselineUse>,
    pub select_clause: String,
    pub windows: Vec<WindowClause>,
    pub where_clause: String,
    pub where_windows: Vec<WhereWindowClause>,
    pub baseline_graph_templates: Vec<BaselineGraphTemplate>,
    pub group_by_clause: Option<String>,
    pub having_clause: Option<String>,
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
    /// Query-defined baseline SPARQL queries
    pub generated_baseline_queries: Vec<GeneratedBaselineQuery>,
    /// Explicit materialization templates extracted from `GRAPH :baselineName { ... }` blocks.
    pub baseline_graph_templates: Vec<BaselineGraphTemplate>,
    /// Top-level `GROUP BY` clause captured from the live query, if present.
    pub group_by_clause: Option<String>,
    /// Top-level `HAVING` clause captured from the live query, if present.
    pub having_clause: Option<String>,
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
        let mut baseline_definitions = Vec::new();
        let mut baseline_uses = Vec::new();
        let mut select_clause = String::new();
        let mut main_group_by_clause = None;
        let mut main_having_clause = None;
        let mut in_main_select = false;
        let mut windows = Vec::new();
        let mut in_where_clause = false;
        let mut where_lines: Vec<&str> = Vec::new();
        let lines = query.lines().collect::<Vec<_>>();
        let mut index = 0;
        let mut current_baseline_header: Option<(String, String)> = None;
        let mut current_baseline_lines: Vec<String> = Vec::new();

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

            if current_baseline_header.is_some()
                && (trimmed_line.starts_with("REGISTER")
                    || trimmed_line.starts_with("DEFINE BASELINE"))
            {
                let (name, source_window) =
                    current_baseline_header.take().expect("baseline header");
                baseline_definitions.push(self.build_baseline_definition(
                    name,
                    source_window,
                    &current_baseline_lines,
                )?);
                current_baseline_lines.clear();
            }

            if trimmed_line.starts_with("REGISTER") {
                in_where_clause = false;
                in_main_select = false;
                register = Some(self.parse_register_clause(trimmed_line, &prefix_mapper)?);
            } else if trimmed_line.starts_with("DEFINE BASELINE") {
                in_where_clause = false;
                in_main_select = false;
                current_baseline_header =
                    Some(self.parse_baseline_definition_header(trimmed_line, &prefix_mapper)?);
            } else if trimmed_line.starts_with("USING BASELINE") {
                in_where_clause = false;
                in_main_select = false;
                if self.is_legacy_baseline_clause(trimmed_line) {
                    baseline = Some(self.parse_baseline_clause(trimmed_line, &prefix_mapper)?);
                } else {
                    baseline_uses
                        .push(self.parse_baseline_use_clause(trimmed_line, &prefix_mapper)?);
                }
            } else if trimmed_line.starts_with("PREFIX") {
                let prefix = self.parse_prefix_declaration(trimmed_line)?;
                prefix_mapper.insert(prefix.prefix.clone(), prefix.namespace.clone());
                prefixes.push(prefix);
            } else if current_baseline_header.is_some() {
                current_baseline_lines.push(line.to_string());
            } else if trimmed_line.starts_with("SELECT") {
                select_clause = trimmed_line.to_string();
                in_main_select = true;
            } else if trimmed_line.starts_with("FROM NAMED WINDOW") {
                in_main_select = false;
                let mut clause = trimmed_line.to_string();
                while !clause.contains(']') && index + 1 < lines.len() {
                    index += 1;
                    clause.push(' ');
                    clause.push_str(lines[index].trim());
                }
                windows.push(self.parse_window_clause(&clause, &prefix_mapper)?);
            } else if trimmed_line.starts_with("WHERE") {
                in_where_clause = true;
                in_main_select = false;
                where_lines.push(line);
            } else if trimmed_line.starts_with("GROUP BY") {
                in_main_select = false;
                if register.is_some() {
                    main_group_by_clause = Some(trimmed_line.to_string());
                }
            } else if trimmed_line.starts_with("HAVING") {
                in_main_select = false;
                if register.is_some() {
                    main_having_clause = Some(trimmed_line.to_string());
                }
            } else if in_main_select {
                select_clause.push(' ');
                select_clause.push_str(trimmed_line);
            } else if in_where_clause {
                where_lines.push(line);
            }

            index += 1;
        }

        if let Some((name, source_window)) = current_baseline_header.take() {
            baseline_definitions.push(self.build_baseline_definition(
                name,
                source_window,
                &current_baseline_lines,
            )?);
        }

        let mut where_clause = where_lines.join("\n");
        if let Some(group_by_clause) = &main_group_by_clause {
            if !where_clause.is_empty() {
                where_clause.push('\n');
            }
            where_clause.push_str(group_by_clause);
        }
        if let Some(having_clause) = &main_having_clause {
            if !where_clause.is_empty() {
                where_clause.push('\n');
            }
            where_clause.push_str(having_clause);
        }
        let where_windows = self.extract_where_windows(&where_clause);
        let baseline_graph_templates =
            self.extract_baseline_graph_templates(&where_clause, &prefix_mapper)?;

        Ok(JanusQueryAst {
            prefixes,
            register,
            baseline,
            baseline_definitions,
            baseline_uses,
            select_clause,
            windows,
            where_clause,
            where_windows,
            baseline_graph_templates,
            group_by_clause: main_group_by_clause,
            having_clause: main_having_clause,
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

        let window_map = ast
            .windows
            .iter()
            .map(|window| (window.window_name.clone(), window))
            .collect::<HashMap<_, _>>();

        for definition in &ast.baseline_definitions {
            let Some(source_window) = window_map.get(&definition.source_window) else {
                return Err(self.parse_error(format!(
                    "DEFINE BASELINE references unknown source window '{}'",
                    definition.source_window
                )));
            };

            let lowered = self.lower_window_clause(source_window);
            if source_window.source_kind != SourceKind::Log
                || lowered.window_type == WindowType::Live
            {
                return Err(self.parse_error(format!(
                    "DEFINE BASELINE source window '{}' must be a historical LOG window",
                    definition.source_window
                )));
            }
        }

        for baseline_use in &ast.baseline_uses {
            let exists = ast
                .baseline_definitions
                .iter()
                .any(|definition| definition.name == baseline_use.name);
            if !exists {
                return Err(self.parse_error(format!(
                    "USING BASELINE references undefined baseline '{}'",
                    baseline_use.name
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
            generated_baseline_queries: Vec::new(),
            baseline_graph_templates: ast.baseline_graph_templates.clone(),
            group_by_clause: ast.group_by_clause.clone(),
            having_clause: ast.having_clause.clone(),
            prefixes,
            where_clause: ast.where_clause.clone(),
            select_clause: ast.select_clause.clone(),
        };

        if !parsed.live_windows.is_empty() {
            parsed.rspql_query = self.generate_rspql_query(&parsed, &prefix_lines);
        }
        parsed.sparql_queries = self.generate_sparql_queries(&parsed, &prefix_lines);
        parsed.generated_baseline_queries =
            self.generate_baseline_queries(&parsed.ast.baseline_definitions, &prefix_lines);

        Ok(parsed)
    }

    fn is_legacy_baseline_clause(&self, line: &str) -> bool {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        parts.len() == 4 && parts[0] == "USING" && parts[1] == "BASELINE"
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

    fn parse_baseline_use_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<BaselineUse, Box<dyn std::error::Error>> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 3 || parts[0] != "USING" || parts[1] != "BASELINE" {
            return Err(self.parse_error(format!("Invalid USING BASELINE clause: {line}")));
        }

        Ok(BaselineUse { name: self.unwrap_iri(parts[2], prefix_mapper) })
    }

    fn parse_baseline_definition_header(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 7
            || parts[0] != "DEFINE"
            || parts[1] != "BASELINE"
            || parts[3] != "ON"
            || parts[4] != "WINDOW"
            || parts[6] != "AS"
        {
            return Err(self.parse_error(format!("Invalid DEFINE BASELINE clause: {line}")));
        }

        Ok((
            self.unwrap_iri(parts[2], prefix_mapper),
            self.unwrap_iri(parts[5], prefix_mapper),
        ))
    }

    fn build_baseline_definition(
        &self,
        name: String,
        source_window: String,
        raw_lines: &[String],
    ) -> Result<BaselineDefinition, Box<dyn std::error::Error>> {
        let raw_query = raw_lines.join("\n").trim().to_string();
        if raw_query.is_empty() {
            return Err(
                self.parse_error(format!("DEFINE BASELINE '{}' must include a SELECT query", name))
            );
        }

        let mut select_clause = String::new();
        let mut where_lines = Vec::new();
        let mut group_by_clause = None;
        let mut in_where = false;

        for line in raw_query.lines().map(str::to_string).collect::<Vec<_>>() {
            let trimmed = line.trim();
            if trimmed.starts_with("SELECT") {
                select_clause = trimmed.to_string();
                in_where = false;
            } else if trimmed.starts_with("WHERE") {
                in_where = true;
                where_lines.push(line);
            } else if trimmed.starts_with("GROUP BY") {
                group_by_clause = Some(trimmed.to_string());
                in_where = false;
            } else if in_where {
                where_lines.push(line);
            } else if !select_clause.is_empty() {
                select_clause.push(' ');
                select_clause.push_str(trimmed);
            }
        }

        if select_clause.is_empty() || where_lines.is_empty() {
            return Err(self.parse_error(format!(
                "DEFINE BASELINE '{}' must include SELECT and WHERE clauses",
                name
            )));
        }

        let output_variables = self.extract_output_variables(&select_clause);

        Ok(BaselineDefinition {
            name,
            source_window,
            raw_query,
            select_clause,
            where_clause: where_lines.join("\n"),
            group_by_clause,
            output_variables,
        })
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

        if let Some(group_by_clause) = &parsed.group_by_clause {
            lines.push(group_by_clause.clone());
        }

        if let Some(having_clause) = &parsed.having_clause {
            lines.push(having_clause.clone());
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

            if self
                .find_window_body(&parsed.ast.where_windows, window, &parsed.prefixes)
                .is_none()
            {
                continue;
            }

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

    fn generate_baseline_queries(
        &self,
        baseline_definitions: &[BaselineDefinition],
        prefix_lines: &[String],
    ) -> Vec<GeneratedBaselineQuery> {
        baseline_definitions
            .iter()
            .map(|definition| {
                let mut lines = prefix_lines.to_vec();
                lines.push(String::new());
                lines.push(definition.select_clause.clone());
                lines.push(String::new());
                lines.push(self.wrap_baseline_where_clause(&definition.where_clause));
                if let Some(group_by_clause) = &definition.group_by_clause {
                    lines.push(group_by_clause.clone());
                }

                GeneratedBaselineQuery {
                    name: definition.name.clone(),
                    source_window: definition.source_window.clone(),
                    sparql_query: lines.join("\n"),
                    output_variables: definition.output_variables.clone(),
                }
            })
            .collect()
    }

    fn wrap_baseline_where_clause(&self, where_clause: &str) -> String {
        let inner = self.extract_where_inner(where_clause);
        format!("WHERE {{\n  GRAPH ?__janus_log_graph {{\n    {}\n  }}\n}}", inner)
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
                return without_where[1..end].trim().to_string();
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

    fn extract_baseline_graph_templates(
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

    fn parse_graph_template_body(
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

    fn split_graph_template_statements(&self, body: &str) -> Vec<String> {
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

    fn tokenize_graph_template_statement(&self, statement: &str) -> Vec<String> {
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

    fn parse_graph_term_template(
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

    fn extract_output_variables(&self, select_clause: &str) -> Vec<String> {
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
