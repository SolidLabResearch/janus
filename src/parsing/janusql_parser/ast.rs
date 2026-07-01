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
    /// Name of the declared source, either a live stream or historical log.
    pub source_name: String,
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
                if range > offset {
                    return None;
                }

                let historical_start = evaluation_time.saturating_sub(offset);
                let historical_end = historical_start.checked_add(range)?;
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
    pub source_windows: Vec<String>,
    pub raw_query: String,
    pub select_clause: String,
    pub where_clause: String,
    pub group_by_clause: Option<String>,
    pub having_clause: Option<String>,
    pub output_variables: Vec<String>,
    pub materialization_kind: HistoricalMaterializationKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaselineUse {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedBaselineQuery {
    pub name: String,
    pub source_window: String,
    pub source_windows: Vec<String>,
    pub sparql_query: String,
    pub output_variables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NestedSubquery {
    pub raw_query: String,
    pub select_clause: String,
    pub where_clause: String,
    pub where_windows: Vec<WhereWindowClause>,
    pub group_by_clause: Option<String>,
    pub having_clause: Option<String>,
    pub output_variables: Vec<String>,
    pub block_start: usize,
    pub block_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedWindowRef {
    pub identifier: String,
    pub window_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubqueryWindowDependencies {
    pub historical_windows: Vec<NamedWindowRef>,
    pub live_windows: Vec<NamedWindowRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubqueryExecutionMode {
    HistoricalMaterializedOnce,
    LiveOnly,
    LiveHistoricalJoin,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalSubqueryPlan {
    HistoricalMaterialized {
        windows: Vec<NamedWindowRef>,
    },
    LiveSubquery {
        windows: Vec<NamedWindowRef>,
    },
    LiveHistoricalJoin {
        live_windows: Vec<NamedWindowRef>,
        historical_windows: Vec<NamedWindowRef>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalSubqueryPlan {
    MaterializeHistoricalResult,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueryPlanningStatistics {
    pub historical_materialized_subqueries: usize,
    pub live_subqueries: usize,
    pub live_historical_joins: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubqueryPlanningDiagnostics {
    pub subquery_index: usize,
    pub execution_mode: SubqueryExecutionMode,
    pub logical_plan: LogicalSubqueryPlan,
    pub physical_plan: PhysicalSubqueryPlan,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedSubquery {
    pub id: String,
    pub query: NestedSubquery,
    pub dependencies: SubqueryWindowDependencies,
    pub execution_mode: SubqueryExecutionMode,
    pub logical_plan: LogicalSubqueryPlan,
    pub physical_plan: PhysicalSubqueryPlan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalMaterializedSubquery {
    pub id: String,
    pub materialized_name: String,
    pub dependencies: SubqueryWindowDependencies,
    pub execution_mode: SubqueryExecutionMode,
    pub query: NestedSubquery,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoricalMaterializationKind {
    ExplicitBaseline,
    NestedSubquery,
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
    pub nested_subqueries: Vec<NestedSubquery>,
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
    /// Historical materialized subqueries detected and lowered into materialized historical results.
    pub historical_materialized_subqueries: Vec<HistoricalMaterializedSubquery>,
    /// Structured nested subquery planning output for diagnostics and future execution modes.
    pub planned_subqueries: Vec<PlannedSubquery>,
    /// Human-readable diagnostics for nested subquery planning.
    pub subquery_planning_diagnostics: Vec<SubqueryPlanningDiagnostics>,
    /// Aggregated nested subquery planning statistics.
    pub planning_statistics: QueryPlanningStatistics,
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
