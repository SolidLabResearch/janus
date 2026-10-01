use std::collections::HashMap;

pub mod ast;
pub mod clauses;
pub mod generation;
pub mod graph;
pub mod legacy_baseline;
pub mod lowerer;
pub mod subquery;
pub mod utils;
pub mod where_clause;

pub use ast::{
    BaselineBootstrapMode, BaselineClause, BaselineDefinition, BaselineGraphTemplate, BaselineUse,
    GeneratedBaselineQuery, GraphTermTemplate, HistoricalMaterializationKind,
    HistoricalMaterializedSubquery, HistoricalWindowSpec, JanusLoweredQuery, LogicalSubqueryPlan,
    NamedWindowRef, NestedSubquery, ParsedJanusQuery, PhysicalSubqueryPlan, PlannedSubquery,
    PrefixDeclaration, QueryPlanningStatistics, R2SOperator, RegisterClause, SourceKind,
    SubqueryExecutionMode, SubqueryPlanningDiagnostics, SubqueryWindowDependencies, TripleTemplate,
    UnionBranch, WhereWindowClause, WindowClause, WindowDefinition, WindowSpec, WindowType,
};
pub use janusql_parser::JanusQueryAst;
pub use legacy_baseline::{TypedLegacyBaselineDefinition, TypedLegacyBaselineMetadata};
pub use lowerer::{JanusLowerer, JanusLoweringError};

pub struct JanusQLParser;

pub(crate) const JANUS_HISTORICAL_MATERIALIZED_SUBQUERY_NS: &str =
    "https://janus.rs/materialized-history/";

impl JanusQLParser {
    /// Creates a new JanusQLParser instance.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self)
    }

    /// Parse a Core Janus-QL query through the standalone language parser.
    pub fn parse_ast(&self, query: &str) -> Result<JanusQueryAst, Box<dyn std::error::Error>> {
        janusql_parser::parse(query).map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
    }

    /// Parses a JanusQL query string.
    pub fn parse(&self, query: &str) -> Result<ParsedJanusQuery, Box<dyn std::error::Error>> {
        let preprocessed = legacy_baseline::preprocess(query)?;
        let ast = janusql_parser::parse(&preprocessed.core_source)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
        let typed_legacy = legacy_baseline::type_legacy_baselines(&ast, &preprocessed.legacy)?;
        let mut lowered = JanusLowerer::lower(&ast, &typed_legacy)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
        self.validate_window_declarations(&lowered.windows)?;
        let prefixes = lowered
            .prefixes
            .iter()
            .map(|prefix| (prefix.prefix.clone(), prefix.namespace.clone()))
            .collect::<HashMap<_, _>>();
        self.validate_where_window_references(&lowered.where_windows, &lowered.windows, &prefixes)?;
        self.validate_window_bodies(&lowered.where_windows)?;
        let planning = self.plan_nested_subqueries(&lowered, &prefixes)?;
        lowered = self.lower_nested_subqueries(&lowered, &planning, &prefixes)?;
        let prefix_lines = lowered
            .prefixes
            .iter()
            .map(|prefix| format!("PREFIX {}: <{}>", prefix.prefix, prefix.namespace))
            .collect::<Vec<_>>();

        let mut live_windows = Vec::new();
        let mut historical_windows = Vec::new();

        for window in &lowered.windows {
            let definition = self.lower_window_clause(window);
            match definition.window_type {
                WindowType::Live => live_windows.push(definition),
                WindowType::HistoricalSliding | WindowType::HistoricalFixed => {
                    historical_windows.push(definition);
                }
            }
        }

        self.validate_historical_windows(&historical_windows)?;

        let r2s = lowered
            .register
            .clone()
            .map(|register| R2SOperator { operator: register.operator, name: register.name });

        if let Some(baseline) = &lowered.baseline {
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

        let window_map = lowered
            .windows
            .iter()
            .map(|window| (window.window_name.clone(), window))
            .collect::<HashMap<_, _>>();

        for definition in &lowered.baseline_definitions {
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

        for baseline_use in &lowered.baseline_uses {
            let exists = lowered
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
            ast,
            lowered: lowered.clone(),
            baseline: lowered.baseline.clone(),
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
            baseline_graph_templates: lowered.baseline_graph_templates.clone(),
            group_by_clause: lowered.group_by_clause.clone(),
            having_clause: lowered.having_clause.clone(),
            prefixes,
            where_clause: lowered.where_clause.clone(),
            select_clause: lowered.select_clause.clone(),
        };

        if !parsed.live_windows.is_empty() {
            parsed.rspql_query = self.generate_rspql_query(&parsed, &prefix_lines);
        }
        parsed.sparql_queries = self.generate_sparql_queries(&parsed, &prefix_lines);
        parsed.generated_baseline_queries =
            self.generate_baseline_queries(&parsed.lowered.baseline_definitions, &prefix_lines);

        Ok(parsed)
    }
}

impl Default for JanusQLParser {
    fn default() -> Self {
        Self::new().expect("Failed to create JanusQLParser")
    }
}

impl JanusQLParser {
    fn validate_historical_windows(
        &self,
        windows: &[WindowDefinition],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for window in windows {
            match window.window_type {
                WindowType::HistoricalSliding => {
                    let offset = window.offset.ok_or_else(|| {
                        self.parse_error(format!(
                            "Historical sliding window '{}' is missing OFFSET",
                            window.window_name
                        ))
                    })?;
                    if window.width > offset {
                        return Err(self.parse_error(format!(
                            "Historical sliding window '{}' has RANGE {} greater than OFFSET {}; the historical window would extend beyond the evaluation time",
                            window.window_name, window.width, offset
                        )));
                    }
                }
                WindowType::HistoricalFixed => {
                    let start = window.start.ok_or_else(|| {
                        self.parse_error(format!(
                            "Historical fixed window '{}' is missing START",
                            window.window_name
                        ))
                    })?;
                    let end = window.end.ok_or_else(|| {
                        self.parse_error(format!(
                            "Historical fixed window '{}' is missing END",
                            window.window_name
                        ))
                    })?;
                    if start >= end {
                        return Err(self.parse_error(format!(
                            "Historical fixed window '{}' must use START less than END",
                            window.window_name
                        )));
                    }
                }
                WindowType::Live => {}
            }
        }

        Ok(())
    }
}
