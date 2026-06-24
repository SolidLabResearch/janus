use std::collections::HashMap;

pub mod ast;
pub mod clauses;
pub mod generation;
pub mod graph;
pub mod subquery;
pub mod utils;
pub mod where_clause;

pub use ast::{
    BaselineBootstrapMode, BaselineClause, BaselineDefinition, BaselineGraphTemplate, BaselineUse,
    GeneratedBaselineQuery, GraphTermTemplate, HistoricalMaterializationKind,
    HistoricalMaterializedSubquery, HistoricalWindowSpec, JanusQueryAst, LogicalSubqueryPlan,
    NamedWindowRef, NestedSubquery, ParsedJanusQuery, PhysicalSubqueryPlan, PlannedSubquery,
    PrefixDeclaration, QueryPlanningStatistics, R2SOperator, RegisterClause, SourceKind,
    SubqueryExecutionMode, SubqueryPlanningDiagnostics, SubqueryWindowDependencies, TripleTemplate,
    WhereWindowClause, WindowClause, WindowDefinition, WindowSpec, WindowType,
};

pub struct JanusQLParser;

pub(crate) const JANUS_HISTORICAL_MATERIALIZED_SUBQUERY_NS: &str =
    "https://janus.rs/materialized-history/";

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
        let mut where_brace_depth = 0isize;
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

            if in_where_clause {
                where_lines.push(line);
                where_brace_depth += self.brace_balance(line);
                if where_brace_depth <= 0 {
                    in_where_clause = false;
                }
                index += 1;
                continue;
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
                where_brace_depth = self.brace_balance(line);
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
        let nested_subqueries = self.extract_nested_subqueries(&where_clause)?;
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
            nested_subqueries,
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
        let planning = self.plan_nested_subqueries(&ast, &prefixes)?;
        let ast = self.lower_nested_subqueries(&ast, &planning, &prefixes)?;
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
            for source_window_name in &definition.source_windows {
                let Some(source_window) = window_map.get(source_window_name) else {
                    let label = match definition.materialization_kind {
                        HistoricalMaterializationKind::ExplicitBaseline => "DEFINE BASELINE",
                        HistoricalMaterializationKind::NestedSubquery => {
                            "Historical materialized subquery"
                        }
                    };
                    return Err(self.parse_error(format!(
                        "{label} references unknown source window '{}'",
                        source_window_name
                    )));
                };

                let lowered = self.lower_window_clause(source_window);
                if source_window.source_kind != SourceKind::Log
                    || lowered.window_type == WindowType::Live
                {
                    let label = match definition.materialization_kind {
                        HistoricalMaterializationKind::ExplicitBaseline => "DEFINE BASELINE",
                        HistoricalMaterializationKind::NestedSubquery => {
                            "Historical materialized subquery"
                        }
                    };
                    return Err(self.parse_error(format!(
                        "{label} source window '{}' must be a historical LOG window",
                        source_window_name
                    )));
                }
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
            historical_materialized_subqueries: self
                .build_historical_materialized_subqueries(&planning.planned_subqueries),
            planned_subqueries: planning.planned_subqueries.clone(),
            subquery_planning_diagnostics: planning.diagnostics.clone(),
            planning_statistics: planning.statistics.clone(),
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
}

impl Default for JanusQLParser {
    fn default() -> Self {
        Self::new().expect("Failed to create JanusQLParser")
    }
}
