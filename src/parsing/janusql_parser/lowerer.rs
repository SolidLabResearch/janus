//! Typed, parallel lowering from the standalone Core AST to Janus execution
//! compatibility structures.

use super::ast::{
    BaselineBootstrapMode, BaselineClause, BaselineGraphTemplate, BaselineUse, GraphTermTemplate,
    JanusLoweredQuery, NestedSubquery, PrefixDeclaration, RegisterClause, SourceKind,
    TripleTemplate, UnionBranch, WhereWindowClause, WindowClause, WindowSpec,
};
use super::legacy_baseline::{build_baseline_definition_from_typed, TypedLegacyBaselineMetadata};
use janusql_parser::{GraphPattern, IriReference, JanusQueryAst, ProjectionItem, TermPattern};
use std::error::Error;
use std::fmt;

/// A semantically valid Core query that cannot be represented by Janus's
/// current execution compatibility structures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JanusLoweringError(pub String);

impl fmt::Display for JanusLoweringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for JanusLoweringError {}

/// Janus-owned lowering boundary for the standalone Core syntax tree.
pub struct JanusLowerer;

impl JanusLowerer {
    /// Lower only typed syntax and typed Janus legacy metadata.  It has no
    /// original-query argument: source-backed AST fragments are retained only
    /// where the existing generators require exact source spelling.
    pub fn lower(
        ast: &JanusQueryAst,
        legacy: &TypedLegacyBaselineMetadata,
    ) -> Result<JanusLoweredQuery, JanusLoweringError> {
        let prefixes = ast
            .prefixes
            .iter()
            .map(|prefix| PrefixDeclaration {
                prefix: prefix.prefix.clone(),
                namespace: prefix.namespace.clone(),
            })
            .collect();
        let register = ast.register.as_ref().map(|register| RegisterClause {
            operator: register.operator.clone(),
            name: canonical_iri(&register.name),
        });
        let windows = ast.windows.iter().map(lower_window).collect();
        let mut where_windows = Vec::new();
        let mut nested_subqueries = Vec::new();
        let mut baseline_graph_templates = Vec::new();
        lower_patterns(
            &ast.where_clause.patterns,
            &mut where_windows,
            &mut nested_subqueries,
            &mut baseline_graph_templates,
            ast.where_clause.span.start,
        )?;

        let union_branches = ast
            .union_branches
            .iter()
            .map(|branch| {
                let mut branch_windows = Vec::new();
                let mut ignored_nested = Vec::new();
                let mut ignored_graphs = Vec::new();
                lower_patterns(
                    &branch.patterns,
                    &mut branch_windows,
                    &mut ignored_nested,
                    &mut ignored_graphs,
                    ast.where_clause.span.start,
                )?;
                Ok(UnionBranch {
                    body: branch.raw.trim().to_string(),
                    where_windows: branch_windows,
                })
            })
            .collect::<Result<Vec<_>, JanusLoweringError>>()?;

        let baseline_definitions = legacy
            .definitions
            .iter()
            .map(build_baseline_definition_from_typed)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| JanusLoweringError(error.to_string()))?;
        let (baseline, baseline_uses) = lower_legacy_uses(&legacy.uses, ast)?;

        let mut where_clause = ast.where_clause.raw.clone();
        if let Some(group_by) = &ast.group_by {
            if !where_clause.is_empty() {
                where_clause.push('\n');
            }
            where_clause.push_str(&group_by.raw);
        }
        if let Some(having) = &ast.having {
            if !where_clause.is_empty() {
                where_clause.push('\n');
            }
            where_clause.push_str(&having.raw);
        }

        Ok(JanusLoweredQuery {
            prefixes,
            register,
            baseline,
            baseline_definitions,
            baseline_uses,
            select_clause: ast.select.raw.clone(),
            windows,
            // Canonical parser-preserved WHERE plus separately typed trailing
            // clauses, matching the established generator input shape.
            where_clause,
            where_windows,
            union_branches,
            nested_subqueries,
            baseline_graph_templates,
            group_by_clause: ast.group_by.as_ref().map(|group| group.raw.clone()),
            having_clause: ast.having.as_ref().map(|having| having.raw.clone()),
        })
    }
}

fn canonical_iri(iri: &IriReference) -> String {
    iri.resolved.clone().unwrap_or_else(|| iri.lexical.clone())
}

fn lower_window(window: &janusql_parser::WindowClause) -> WindowClause {
    let source_kind = match window.source_kind {
        janusql_parser::SourceKind::Stream => SourceKind::Stream,
        janusql_parser::SourceKind::Log => SourceKind::Log,
    };
    let spec = match window.spec {
        janusql_parser::WindowSpec::LiveSliding { range, step } => {
            WindowSpec::LiveSliding { range, step }
        }
        janusql_parser::WindowSpec::HistoricalSliding { offset, range, step } => {
            WindowSpec::HistoricalSliding { offset, range, step }
        }
        janusql_parser::WindowSpec::HistoricalFixed { start, end } => {
            WindowSpec::HistoricalFixed { start, end }
        }
    };
    WindowClause {
        window_name: canonical_iri(&window.name),
        source_kind,
        source_name: canonical_iri(&window.source),
        spec,
    }
}

fn lower_patterns(
    patterns: &[GraphPattern],
    where_windows: &mut Vec<WhereWindowClause>,
    nested_subqueries: &mut Vec<NestedSubquery>,
    graph_templates: &mut Vec<BaselineGraphTemplate>,
    outer_where_start: usize,
) -> Result<(), JanusLoweringError> {
    for pattern in patterns {
        match pattern {
            GraphPattern::Window { window } => {
                where_windows.push(WhereWindowClause {
                    identifier: window.window.lexical.clone(),
                    body: window.raw.trim().to_string(),
                });
                lower_patterns(
                    &window.patterns,
                    where_windows,
                    nested_subqueries,
                    graph_templates,
                    outer_where_start,
                )?;
            }
            GraphPattern::Graph { graph } => {
                graph_templates.push(BaselineGraphTemplate {
                    baseline_name: canonical_iri(&graph.graph),
                    triples: graph
                        .patterns
                        .iter()
                        .filter_map(|pattern| match pattern {
                            GraphPattern::Triple { triple } => Some(lower_triple(triple)),
                            _ => None,
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                });
                lower_patterns(
                    &graph.patterns,
                    where_windows,
                    nested_subqueries,
                    graph_templates,
                    outer_where_start,
                )?;
            }
            GraphPattern::Group { patterns, .. } => lower_patterns(
                patterns,
                where_windows,
                nested_subqueries,
                graph_templates,
                outer_where_start,
            )?,
            GraphPattern::Union { branches, .. } => {
                for branch in branches {
                    lower_patterns(
                        branch,
                        where_windows,
                        nested_subqueries,
                        graph_templates,
                        outer_where_start,
                    )?;
                }
            }
            GraphPattern::Subquery { query } => {
                nested_subqueries.push(lower_nested(query, outer_where_start)?);
            }
            GraphPattern::Triple { .. }
            | GraphPattern::Filter { .. }
            | GraphPattern::Service { .. }
            | GraphPattern::Raw { .. } => {}
        }
    }
    Ok(())
}

fn lower_nested(
    query: &janusql_parser::NestedSubquery,
    outer_where_start: usize,
) -> Result<NestedSubquery, JanusLoweringError> {
    let mut where_windows = Vec::new();
    let mut ignored_nested = Vec::new();
    let mut ignored_graphs = Vec::new();
    lower_patterns(
        &query.where_clause.patterns,
        &mut where_windows,
        &mut ignored_nested,
        &mut ignored_graphs,
        query.where_clause.span.start,
    )?;
    let raw_query = query.raw.trim().to_string();
    let raw_query = raw_query
        .strip_prefix('{')
        .and_then(|raw| raw.strip_suffix('}'))
        .map(str::trim)
        .unwrap_or(raw_query.as_str())
        .to_string();
    Ok(NestedSubquery {
        raw_query,
        select_clause: query.select.raw.clone(),
        where_clause: query.where_clause.raw.clone(),
        where_windows,
        group_by_clause: query.group_by.as_ref().map(|group| group.raw.clone()),
        having_clause: query.having.as_ref().map(|having| having.raw.clone()),
        output_variables: projected_output_variables(&query.select.items),
        block_start: query.span.start.checked_sub(outer_where_start).ok_or_else(|| {
            JanusLoweringError("nested subquery span precedes enclosing WHERE span".into())
        })?,
        block_end: query.span.end.checked_sub(outer_where_start).ok_or_else(|| {
            JanusLoweringError("nested subquery span precedes enclosing WHERE span".into())
        })?,
    })
}

fn projected_output_variables(items: &[ProjectionItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            ProjectionItem::Variable { variable, .. } => Some(variable.name.clone()),
            ProjectionItem::AggregateAlias { alias, .. }
            | ProjectionItem::ExpressionAlias { alias, .. } => Some(alias.name.clone()),
            ProjectionItem::Raw { .. } => None,
        })
        .collect()
}

fn lower_triple(
    triple: &janusql_parser::TriplePattern,
) -> Result<TripleTemplate, JanusLoweringError> {
    Ok(TripleTemplate {
        subject: lower_term(&triple.subject)?,
        predicate: lower_term(&triple.predicate)?,
        object: lower_term(&triple.object)?,
    })
}

fn lower_term(term: &TermPattern) -> Result<GraphTermTemplate, JanusLoweringError> {
    match term {
        TermPattern::Variable { variable } => {
            Ok(GraphTermTemplate::Variable(variable.name.trim_start_matches('?').to_string()))
        }
        TermPattern::Iri { iri } => Ok(GraphTermTemplate::Iri(canonical_iri(iri))),
        TermPattern::Literal { lexical, .. } => Ok(GraphTermTemplate::Literal(lexical.clone())),
        TermPattern::Raw { raw, .. } => Err(JanusLoweringError(format!(
            "unsupported GRAPH template term in Janus execution lowering: {raw}"
        ))),
    }
}

/// Legacy baseline shells are Janus-only metadata, not Core query parsing.
fn lower_legacy_uses(
    uses: &[String],
    ast: &JanusQueryAst,
) -> Result<(Option<BaselineClause>, Vec<BaselineUse>), JanusLoweringError> {
    let mut baseline = None;
    let mut baseline_uses = Vec::new();
    for raw in uses {
        let words = raw.split_whitespace().collect::<Vec<_>>();
        match words.as_slice() {
            ["USING", "BASELINE", window, mode] => {
                let mode = match *mode {
                    "LAST" => BaselineBootstrapMode::Last,
                    "AGGREGATE" => BaselineBootstrapMode::Aggregate,
                    other => {
                        return Err(JanusLoweringError(format!(
                            "unsupported baseline mode '{other}'"
                        )))
                    }
                };
                baseline = Some(BaselineClause {
                    window_name: resolve_legacy_identifier(window, ast),
                    mode,
                });
            }
            ["USING", "BASELINE", name] => {
                baseline_uses.push(BaselineUse { name: resolve_legacy_identifier(name, ast) });
            }
            _ => return Err(JanusLoweringError(format!("invalid USING BASELINE shell: {raw}"))),
        }
    }
    Ok((baseline, baseline_uses))
}

fn resolve_legacy_identifier(identifier: &str, ast: &JanusQueryAst) -> String {
    if identifier.starts_with('<') && identifier.ends_with('>') {
        return identifier[1..identifier.len() - 1].to_string();
    }
    let Some((prefix, local)) = identifier.split_once(':') else {
        return identifier.to_string();
    };
    ast.prefixes
        .iter()
        .find(|declaration| declaration.prefix == prefix)
        .map_or_else(
            || identifier.to_string(),
            |declaration| format!("{}{local}", declaration.namespace),
        )
}

#[cfg(test)]
mod tests {
    use super::JanusLowerer;
    use crate::parsing::janusql_parser::legacy_baseline::{preprocess, type_legacy_baselines};
    use crate::parsing::janusql_parser::JanusQLParser;

    fn lower_typed(source: &str) -> crate::parsing::janusql_parser::JanusLoweredQuery {
        let preprocessed = preprocess(source).expect("legacy preprocessing");
        let ast = janusql_parser::parse(&preprocessed.core_source).expect("Core AST");
        let legacy = type_legacy_baselines(&ast, &preprocessed.legacy).expect("typed legacy");
        JanusLowerer::lower(&ast, &legacy).expect("typed lowering")
    }

    fn assert_direct_lowering(source: &str, windows: usize, where_windows: usize) {
        let lowered = lower_typed(source);
        assert_eq!(lowered.windows.len(), windows);
        assert_eq!(lowered.where_windows.len(), where_windows);
        assert!(lowered.select_clause.starts_with("SELECT"));
    }

    #[test]
    fn typed_lowering_preserves_generated_rspql_and_baseline_sparql_inputs() {
        let parser = JanusQLParser::new().unwrap();
        let mut parsed = parser.parse(LIVE).unwrap();
        let expected_rspql = parsed.rspql_query.clone();
        let typed = lower_typed(LIVE);
        parsed.lowered = typed.clone();
        parsed.where_clause = typed.where_clause.clone();
        parsed.select_clause = typed.select_clause.clone();
        let prefix_lines = typed
            .prefixes
            .iter()
            .map(|prefix| format!("PREFIX {}: <{}>", prefix.prefix, prefix.namespace))
            .collect::<Vec<_>>();
        assert_eq!(parser.generate_rspql_query(&parsed, &prefix_lines), expected_rspql);

        let baseline_source = r#"
PREFIX ex: <http://example.org/>
FROM NAMED WINDOW ex:history ON LOG ex:log [START 1000 END 2000]
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
DEFINE BASELINE ex:baseline ON WINDOW ex:history AS
SELECT ?sensor
WHERE { ?sensor ex:value ?value . }
REGISTER RStream ex:out AS
USING BASELINE ex:baseline
SELECT ?sensor
WHERE { WINDOW ex:live { ?sensor ex:value ?value . } GRAPH ex:baseline { ?sensor ex:baselineValue ?value . } }
"#;
        let expected = parser.parse(baseline_source).unwrap().generated_baseline_queries;
        let typed = lower_typed(baseline_source);
        assert_eq!(
            parser.generate_baseline_queries(&typed.baseline_definitions, &prefix_lines),
            expected
        );
    }

    const LIVE: &str = r#"
PREFIX ex: <http://example.org/>
REGISTER RStream ex:out AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
WHERE { WINDOW ex:live { ?sensor ex:value ?value . } }
"#;

    #[test]
    fn typed_lowerer_lowers_live_fixed_sliding_hybrid_and_aggregate() {
        assert_direct_lowering(LIVE, 1, 1);
        assert_direct_lowering(
            r#"
PREFIX ex: <http://example.org/>
SELECT ?sensor
FROM NAMED WINDOW ex:history ON LOG ex:log [START 1000 END 2000]
WHERE { WINDOW ex:history { ?sensor ex:value ?value . } }
"#,
            1,
            1,
        );
        assert_direct_lowering(
            r#"
PREFIX ex: <http://example.org/>
SELECT ?sensor
FROM NAMED WINDOW ex:history ON LOG ex:log [OFFSET 3000 RANGE 1000 STEP 250]
WHERE { WINDOW ex:history { ?sensor ex:value ?value . } }
"#,
            1,
            1,
        );
        assert_direct_lowering(
            r#"
PREFIX ex: <http://example.org/>
REGISTER RStream ex:out AS
SELECT ?sensor (AVG(?value) AS ?average)
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
FROM NAMED WINDOW ex:history ON LOG ex:log [START 1000 END 2000]
WHERE {
  WINDOW ex:live { ?sensor ex:value ?value . }
  WINDOW ex:history { ?sensor ex:old ?old . }
}
GROUP BY ?sensor
HAVING(AVG(?value) > 1)
"#,
            2,
            2,
        );
    }

    #[test]
    fn typed_lowerer_lowers_windows_before_select_and_union() {
        assert_direct_lowering(
            r#"
PREFIX ex: <http://example.org/>
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
REGISTER RStream ex:out AS
SELECT ?sensor
WHERE { WINDOW ex:live { ?sensor ex:value ?value . } }
"#,
            1,
            1,
        );
        assert_direct_lowering(
            r#"
PREFIX ex: <http://example.org/>
REGISTER RStream ex:out AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:one ON STREAM ex:oneStream [RANGE 500 STEP 100]
FROM NAMED WINDOW ex:two ON STREAM ex:twoStream [RANGE 500 STEP 100]
WHERE {
  { WINDOW ex:one { ?sensor ex:value ?value . } }
  UNION
  { WINDOW ex:two { ?sensor ex:value ?value . } }
}
"#,
            2,
            2,
        );
    }

    #[test]
    fn typed_lowerer_reuses_typed_legacy_baselines_without_core_source() {
        let source = String::from(
            r#"
PREFIX ex: <http://example.org/>
FROM NAMED WINDOW ex:history ON LOG ex:log [START 1000 END 2000]
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
DEFINE BASELINE ex:baseline ON WINDOW ex:history AS
SELECT ?sensor
WHERE { ?sensor ex:value ?value . }
REGISTER RStream ex:out AS
USING BASELINE ex:baseline
SELECT ?sensor
WHERE { WINDOW ex:live { ?sensor ex:value ?value . } GRAPH ex:baseline { ?sensor ex:baselineValue ?value . } }
"#,
        );
        let preprocessed = preprocess(&source).unwrap();
        let ast = janusql_parser::parse(&preprocessed.core_source).unwrap();
        let legacy = type_legacy_baselines(&ast, &preprocessed.legacy).unwrap();
        drop(source);
        let lowered = JanusLowerer::lower(&ast, &legacy).unwrap();
        assert_eq!(lowered.baseline_definitions.len(), 1);
    }

    #[test]
    fn typed_lowerer_nested_historical_subquery_matches_planning_and_materialization() {
        let source = r#"
PREFIX : <http://example.org/>
FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW :historyDay ON LOG :stream [START 0 END 86400000]
REGISTER RStream :output AS
SELECT ?sensor (AVG(?liveValue) AS ?minuteAvgValue) ?dayAvgValue
WHERE {
  WINDOW :liveMinute { ?sensor :hasValue ?liveValue . }
  {
    SELECT ?sensor (AVG(?histValue) AS ?dayAvgValue)
    WHERE { WINDOW :historyDay { ?sensor :hasValue ?histValue . } }
    GROUP BY ?sensor
    HAVING(AVG(?histValue) > 0)
  }
}
GROUP BY ?sensor ?dayAvgValue
"#;
        let parser = JanusQLParser::new().unwrap();
        let typed = lower_typed(source);
        let prefixes = typed
            .prefixes
            .iter()
            .map(|prefix| (prefix.prefix.clone(), prefix.namespace.clone()))
            .collect();
        let planning = parser.plan_nested_subqueries(&typed, &prefixes).unwrap();
        let lowered = parser.lower_nested_subqueries(&typed, &planning, &prefixes).unwrap();
        assert_eq!(planning.planned_subqueries.len(), 1);
        let new_plan = &planning.planned_subqueries[0];
        assert_eq!(new_plan.dependencies.historical_windows.len(), 1);
        assert_eq!(
            new_plan.execution_mode,
            crate::parsing::janusql_parser::SubqueryExecutionMode::HistoricalMaterializedOnce
        );
        assert_eq!(
            new_plan.physical_plan,
            crate::parsing::janusql_parser::PhysicalSubqueryPlan::MaterializeHistoricalResult
        );
        assert_eq!(new_plan.query.output_variables, vec!["?sensor", "?dayAvgValue"]);
        assert_eq!(planning.statistics.historical_materialized_subqueries, 1);
        assert_eq!(lowered.baseline_definitions.len(), 1);
        assert_eq!(lowered.baseline_uses.len(), 1);
        let prefix_lines = typed
            .prefixes
            .iter()
            .map(|prefix| format!("PREFIX {}: <{}>", prefix.prefix, prefix.namespace))
            .collect::<Vec<_>>();
        let generated =
            parser.generate_baseline_queries(&lowered.baseline_definitions, &prefix_lines);
        assert_eq!(generated.len(), 1);
        assert!(generated[0].sparql_query.contains("SELECT ?sensor"));
        assert!(generated[0].sparql_query.contains("?histValue"));
    }
}
