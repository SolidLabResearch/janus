use std::collections::HashMap;
use crate::parsing::janusql_parser::{JanusQLParser, JANUS_HISTORICAL_MATERIALIZED_SUBQUERY_NS};
use crate::parsing::janusql_parser::ast::{
    JanusQueryAst, NestedSubquery, PlannedSubquery, SubqueryPlanningDiagnostics,
    QueryPlanningStatistics, SubqueryWindowDependencies, SubqueryExecutionMode,
    LogicalSubqueryPlan, PhysicalSubqueryPlan, HistoricalMaterializedSubquery,
    BaselineDefinition, BaselineUse, HistoricalMaterializationKind, NamedWindowRef,
    WindowClause, SourceKind, WindowType
};

#[derive(Debug, Clone)]
pub(crate) struct NestedSubqueryPlanningResult {
    pub(crate) planned_subqueries: Vec<PlannedSubquery>,
    pub(crate) diagnostics: Vec<SubqueryPlanningDiagnostics>,
    pub(crate) statistics: QueryPlanningStatistics,
}

impl JanusQLParser {
    pub(crate) fn plan_nested_subqueries(
        &self,
        ast: &JanusQueryAst,
        prefixes: &HashMap<String, String>,
    ) -> Result<NestedSubqueryPlanningResult, Box<dyn std::error::Error>> {
        let windows_by_name = ast
            .windows
            .iter()
            .map(|window| (window.window_name.clone(), window))
            .collect::<HashMap<_, _>>();
        let mut planned_subqueries = Vec::new();
        let mut diagnostics = Vec::new();
        let mut statistics = QueryPlanningStatistics::default();

        for (index, subquery) in ast.nested_subqueries.iter().enumerate() {
            let dependencies =
                self.analyze_subquery_window_dependencies(subquery, &windows_by_name, prefixes)?;
            let execution_mode = self.classify_subquery_execution_mode(&dependencies);
            let logical_plan = self.build_logical_subquery_plan(&dependencies, execution_mode);
            let physical_plan = self.build_physical_subquery_plan(&logical_plan);
            self.validate_subquery_plan(execution_mode, physical_plan)?;
            let id = format!("__hist_mat_subquery_{index}");
            let planned = PlannedSubquery {
                id: id.clone(),
                query: subquery.clone(),
                dependencies: dependencies.clone(),
                execution_mode,
                logical_plan: logical_plan.clone(),
                physical_plan,
            };
            self.record_planning_statistics(&mut statistics, execution_mode);
            diagnostics.push(SubqueryPlanningDiagnostics {
                subquery_index: index,
                execution_mode,
                logical_plan: logical_plan.clone(),
                physical_plan,
                summary: self.format_subquery_planning_diagnostics(
                    index,
                    execution_mode,
                    &logical_plan,
                    physical_plan,
                ),
            });
            planned_subqueries.push(planned);
        }

        Ok(NestedSubqueryPlanningResult { planned_subqueries, diagnostics, statistics })
    }

    pub(crate) fn build_historical_materialized_subqueries(
        &self,
        planned_subqueries: &[PlannedSubquery],
    ) -> Vec<HistoricalMaterializedSubquery> {
        planned_subqueries
            .iter()
            .filter(|planned| {
                planned.physical_plan == PhysicalSubqueryPlan::MaterializeHistoricalResult
            })
            .map(|planned| HistoricalMaterializedSubquery {
                id: planned.id.clone(),
                materialized_name: format!(
                    "{JANUS_HISTORICAL_MATERIALIZED_SUBQUERY_NS}{}",
                    planned.id
                ),
                dependencies: planned.dependencies.clone(),
                execution_mode: planned.execution_mode,
                query: planned.query.clone(),
            })
            .collect()
    }

    pub(crate) fn lower_nested_subqueries(
        &self,
        ast: &JanusQueryAst,
        planning: &NestedSubqueryPlanningResult,
        prefixes: &HashMap<String, String>,
    ) -> Result<JanusQueryAst, Box<dyn std::error::Error>> {
        if ast.nested_subqueries.is_empty() {
            return Ok(ast.clone());
        }

        let mut lowered = ast.clone();
        let mut rewritten_where = String::with_capacity(ast.where_clause.len());
        let mut cursor = 0usize;

        for planned in &planning.planned_subqueries {
            let block_start = planned.query.block_start;
            let block_end = planned.query.block_end;
            rewritten_where.push_str(&ast.where_clause[cursor..block_start]);
            let (graph_pattern, baseline_definition, baseline_use) =
                self.lower_physical_subquery_plan(planned, ast, prefixes)?;
            rewritten_where.push_str(&graph_pattern);
            cursor = block_end;
            lowered.baseline_definitions.push(baseline_definition);
            lowered.baseline_uses.push(baseline_use);
        }

        rewritten_where.push_str(&ast.where_clause[cursor..]);
        lowered.where_clause = rewritten_where;
        lowered.where_windows = self.extract_where_windows(&lowered.where_clause);
        lowered.nested_subqueries = ast.nested_subqueries.clone();
        lowered.baseline_graph_templates =
            self.extract_baseline_graph_templates(&lowered.where_clause, prefixes)?;

        Ok(lowered)
    }

    pub(crate) fn lower_physical_subquery_plan(
        &self,
        planned: &PlannedSubquery,
        ast: &JanusQueryAst,
        prefixes: &HashMap<String, String>,
    ) -> Result<(String, BaselineDefinition, BaselineUse), Box<dyn std::error::Error>> {
        match planned.physical_plan {
            PhysicalSubqueryPlan::MaterializeHistoricalResult => {
                let analysis = HistoricalMaterializedSubquery {
                    id: planned.id.clone(),
                    materialized_name: format!(
                        "{JANUS_HISTORICAL_MATERIALIZED_SUBQUERY_NS}{}",
                        planned.id
                    ),
                    dependencies: planned.dependencies.clone(),
                    execution_mode: planned.execution_mode,
                    query: planned.query.clone(),
                };
                let lowered_definition =
                    self.build_historical_materialized_definition(&analysis, ast, prefixes)?;
                let graph_pattern = self.build_historical_materialized_graph_pattern(
                    &analysis.materialized_name,
                    &analysis.query.output_variables,
                )?;
                Ok((
                    graph_pattern,
                    lowered_definition,
                    BaselineUse { name: analysis.materialized_name.clone() },
                ))
            }
            PhysicalSubqueryPlan::Unsupported => Err(self.parse_error(
                "Nested subquery physical plan is unsupported and cannot be lowered.",
            )),
        }
    }

    pub(crate) fn build_historical_materialized_definition(
        &self,
        analysis: &HistoricalMaterializedSubquery,
        ast: &JanusQueryAst,
        prefixes: &HashMap<String, String>,
    ) -> Result<BaselineDefinition, Box<dyn std::error::Error>> {
        let windows_by_name = ast
            .windows
            .iter()
            .map(|window| (window.window_name.clone(), window))
            .collect::<HashMap<_, _>>();
        let source_windows = analysis
            .dependencies
            .historical_windows
            .iter()
            .map(|window| window.window_name.clone())
            .collect::<Vec<_>>();
        let source_window = source_windows.first().cloned().ok_or_else(|| {
            self.parse_error(
                "Historical materialized subquery requires at least one historical window",
            )
        })?;
        for window_name in &source_windows {
            if !windows_by_name.contains_key(window_name) {
                return Err(self.parse_error(format!(
                    "Nested historical subquery references unknown window '{}'",
                    window_name
                )));
            }
        }
        let where_clause = self.rewrite_nested_subquery_where_for_materialization(
            &analysis.query.where_clause,
            prefixes,
        )?;

        Ok(BaselineDefinition {
            name: analysis.materialized_name.clone(),
            source_window,
            source_windows,
            raw_query: analysis.query.raw_query.clone(),
            select_clause: analysis.query.select_clause.clone(),
            where_clause,
            group_by_clause: analysis.query.group_by_clause.clone(),
            having_clause: analysis.query.having_clause.clone(),
            output_variables: analysis.query.output_variables.clone(),
            materialization_kind: HistoricalMaterializationKind::NestedSubquery,
        })
    }

    pub(crate) fn build_historical_materialized_graph_pattern(
        &self,
        materialized_name: &str,
        output_variables: &[String],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let anchor_var =
            self.select_baseline_anchor_variable(output_variables).ok_or_else(|| {
                self.parse_error(
                    "Historical materialized subquery requires at least one projected variable",
                )
            })?;
        let projected = output_variables
            .iter()
            .filter(|variable| *variable != &anchor_var)
            .map(|variable| {
                let local_name = variable.trim_start_matches('?');
                let predicate = format!("<{}{local_name}>", materialized_name);
                format!("    {} {} {} .", anchor_var, predicate, variable)
            })
            .collect::<Vec<_>>();

        if projected.is_empty() {
            return Err(self.parse_error(
                "Historical materialized subquery must project at least one join value besides the anchor variable",
            ));
        }

        Ok(format!(
            "{{\n  GRAPH <{}> {{\n{}\n  }}\n}}",
            materialized_name,
            projected.join("\n")
        ))
    }

    pub(crate) fn analyze_subquery_window_dependencies(
        &self,
        subquery: &NestedSubquery,
        windows_by_name: &HashMap<String, &WindowClause>,
        prefixes: &HashMap<String, String>,
    ) -> Result<SubqueryWindowDependencies, Box<dyn std::error::Error>> {
        let mut historical_windows = Vec::new();
        let mut live_windows = Vec::new();

        for where_window in &subquery.where_windows {
            let window_name = self
                .resolve_window_identifier(&where_window.identifier, windows_by_name, prefixes)
                .ok_or_else(|| {
                    self.parse_error(format!(
                        "Nested subquery references unknown window '{}'",
                        where_window.identifier
                    ))
                })?;
            let window = windows_by_name.get(&window_name).ok_or_else(|| {
                self.parse_error(format!(
                    "Nested subquery references unknown window '{}'",
                    window_name
                ))
            })?;
            let named_ref = NamedWindowRef {
                identifier: where_window.identifier.clone(),
                window_name: window_name.clone(),
            };
            let lowered = self.lower_window_clause(window);
            if window.source_kind == SourceKind::Log && lowered.window_type != WindowType::Live {
                historical_windows.push(named_ref);
            } else {
                live_windows.push(named_ref);
            }
        }

        historical_windows.sort_by(|left, right| left.window_name.cmp(&right.window_name));
        historical_windows.dedup_by(|left, right| left.window_name == right.window_name);
        live_windows.sort_by(|left, right| left.window_name.cmp(&right.window_name));
        live_windows.dedup_by(|left, right| left.window_name == right.window_name);

        Ok(SubqueryWindowDependencies { historical_windows, live_windows })
    }

    pub(crate) fn classify_subquery_execution_mode(
        &self,
        dependencies: &SubqueryWindowDependencies,
    ) -> SubqueryExecutionMode {
        if !dependencies.historical_windows.is_empty() && dependencies.live_windows.is_empty() {
            SubqueryExecutionMode::HistoricalMaterializedOnce
        } else if !dependencies.live_windows.is_empty()
            && dependencies.historical_windows.is_empty()
        {
            SubqueryExecutionMode::LiveOnly
        } else if !dependencies.live_windows.is_empty()
            && !dependencies.historical_windows.is_empty()
        {
            SubqueryExecutionMode::LiveHistoricalJoin
        } else {
            SubqueryExecutionMode::Unsupported
        }
    }

    pub(crate) fn build_logical_subquery_plan(
        &self,
        dependencies: &SubqueryWindowDependencies,
        execution_mode: SubqueryExecutionMode,
    ) -> LogicalSubqueryPlan {
        match execution_mode {
            SubqueryExecutionMode::HistoricalMaterializedOnce => {
                LogicalSubqueryPlan::HistoricalMaterialized {
                    windows: dependencies.historical_windows.clone(),
                }
            }
            SubqueryExecutionMode::LiveOnly => {
                LogicalSubqueryPlan::LiveSubquery { windows: dependencies.live_windows.clone() }
            }
            SubqueryExecutionMode::LiveHistoricalJoin => {
                LogicalSubqueryPlan::LiveHistoricalJoin {
                    live_windows: dependencies.live_windows.clone(),
                    historical_windows: dependencies.historical_windows.clone(),
                }
            }
            SubqueryExecutionMode::Unsupported => {
                LogicalSubqueryPlan::LiveSubquery { windows: Vec::new() }
            }
        }
    }

    pub(crate) fn build_physical_subquery_plan(
        &self,
        logical_plan: &LogicalSubqueryPlan,
    ) -> PhysicalSubqueryPlan {
        match logical_plan {
            LogicalSubqueryPlan::HistoricalMaterialized { .. } => {
                PhysicalSubqueryPlan::MaterializeHistoricalResult
            }
            LogicalSubqueryPlan::LiveSubquery { .. }
            | LogicalSubqueryPlan::LiveHistoricalJoin { .. } => PhysicalSubqueryPlan::Unsupported,
        }
    }

    pub(crate) fn validate_subquery_plan(
        &self,
        execution_mode: SubqueryExecutionMode,
        physical_plan: PhysicalSubqueryPlan,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match (execution_mode, physical_plan) {
            (
                SubqueryExecutionMode::HistoricalMaterializedOnce,
                PhysicalSubqueryPlan::MaterializeHistoricalResult,
            ) => Ok(()),
            (SubqueryExecutionMode::LiveHistoricalJoin, _) => Err(self.parse_error(
                "Mixed live/historical nested subqueries require LiveHistoricalJoin planning and are not supported yet.",
            )),
            (SubqueryExecutionMode::LiveOnly, _) => Err(self.parse_error(
                "Live-only nested subqueries require LiveSubquery planning and are not supported yet.",
            )),
            (SubqueryExecutionMode::Unsupported, _) => Err(self.parse_error(
                "Nested subquery must reference at least one known WINDOW block.",
            )),
            (_, PhysicalSubqueryPlan::Unsupported) => Err(self.parse_error(
                "Nested subquery physical plan is unsupported and cannot be lowered.",
            )),
        }
    }

    pub(crate) fn record_planning_statistics(
        &self,
        statistics: &mut QueryPlanningStatistics,
        execution_mode: SubqueryExecutionMode,
    ) {
        match execution_mode {
            SubqueryExecutionMode::HistoricalMaterializedOnce => {
                statistics.historical_materialized_subqueries += 1;
            }
            SubqueryExecutionMode::LiveOnly => {
                statistics.live_subqueries += 1;
            }
            SubqueryExecutionMode::LiveHistoricalJoin => {
                statistics.live_historical_joins += 1;
            }
            SubqueryExecutionMode::Unsupported => {}
        }
    }

    pub(crate) fn format_subquery_planning_diagnostics(
        &self,
        subquery_index: usize,
        execution_mode: SubqueryExecutionMode,
        logical_plan: &LogicalSubqueryPlan,
        physical_plan: PhysicalSubqueryPlan,
    ) -> String {
        format!(
            "Nested subquery #{subquery_index}\nExecution mode: {execution_mode:?}\n\nLogical plan:\n{logical_plan:?}\n\nPhysical plan:\n{physical_plan:?}"
        )
    }

    pub(crate) fn resolve_window_identifier(
        &self,
        identifier: &str,
        windows_by_name: &HashMap<String, &WindowClause>,
        prefixes: &HashMap<String, String>,
    ) -> Option<String> {
        let resolved = self.unwrap_iri(identifier, prefixes);
        if windows_by_name.contains_key(&resolved) {
            return Some(resolved);
        }

        windows_by_name.keys().find_map(|window_name| {
            let wrapped = self.wrap_iri(window_name, prefixes);
            if wrapped == identifier || window_name == identifier {
                Some(window_name.clone())
            } else {
                None
            }
        })
    }

    pub(crate) fn rewrite_nested_subquery_where_for_materialization(
        &self,
        where_clause: &str,
        prefixes: &HashMap<String, String>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let inner = self.extract_where_inner(where_clause);
        if inner.is_empty() {
            return Ok("WHERE { }".to_string());
        }

        let mut rewritten = String::new();
        let mut offset = 0usize;

        while let Some(found) = inner[offset..].find("WINDOW") {
            let start = offset + found;
            rewritten.push_str(&inner[offset..start]);

            let after_keyword = start + "WINDOW".len();
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
                rewritten.push_str("WINDOW");
                offset = after_keyword;
                continue;
            }

            let body_start = cursor + 1;
            let Some(body_end) = self.find_matching_brace(&inner, cursor) else {
                return Err(self.parse_error("Unclosed WINDOW block in nested subquery"));
            };
            let graph_name = self.wrap_iri(&self.unwrap_iri(identifier, prefixes), prefixes);
            let body = inner[body_start..body_end].trim();
            rewritten.push_str(&format!("GRAPH {} {{\n    {}\n  }}", graph_name, body));
            offset = body_end + 1;
        }

        if offset < inner.len() {
            rewritten.push_str(&inner[offset..]);
        }

        Ok(format!("WHERE {{\n  {}\n}}", rewritten.trim()))
    }

    pub(crate) fn extract_nested_subqueries(
        &self,
        where_clause: &str,
    ) -> Result<Vec<NestedSubquery>, Box<dyn std::error::Error>> {
        let where_start = where_clause
            .find('{')
            .ok_or_else(|| self.parse_error("WHERE clause must contain an opening '{'"))?;
        let where_end = self
            .find_matching_brace(where_clause, where_start)
            .ok_or_else(|| self.parse_error("WHERE clause must contain a closing '}'"))?;
        let mut nested = Vec::new();
        let mut cursor = where_start + 1;

        while cursor < where_end {
            let remainder = &where_clause[cursor..where_end];
            let Some(relative_open) = remainder.find('{') else {
                break;
            };
            let block_start = cursor + relative_open;
            let Some(block_end) = self.find_matching_brace(where_clause, block_start) else {
                return Err(self.parse_error("Unclosed nested block in WHERE clause"));
            };
            if block_end > where_end {
                break;
            }

            let body = where_clause[block_start + 1..block_end].trim();
            if body.to_uppercase().starts_with("SELECT") {
                nested.push(self.parse_nested_subquery(where_clause, block_start, block_end)?);
            }
            cursor = block_end + 1;
        }

        Ok(nested)
    }
}

