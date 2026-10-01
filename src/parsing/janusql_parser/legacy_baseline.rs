//! Janus-only preprocessing for historical baseline compatibility syntax.
//!
//! This module recognizes only `DEFINE BASELINE` and `USING BASELINE`.  It
//! leaves all Core text, including GRAPH, UNION, WINDOW and nested SELECT
//! syntax, unchanged for `janusql-parser`.

use crate::parsing::janusql_parser::ast::{BaselineDefinition, HistoricalMaterializationKind};
use janusql_parser::{GraphPattern, IriReference, JanusQueryAst, ProjectionItem};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LegacyBaselineMetadata {
    pub(crate) definitions: Vec<String>,
    pub(crate) uses: Vec<String>,
    pub(crate) raw_definitions: Vec<RawLegacyBaselineDefinition>,
}

/// Janus-only DEFINE shell; `raw_body` is deliberately not interpreted here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawLegacyBaselineDefinition {
    pub(crate) name: String,
    pub(crate) source_window: String,
    pub(crate) raw_body: String,
    pub(crate) body_span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedLegacyBaselineDefinition {
    pub name: String,
    pub source_window: String,
    pub raw_body: String,
    pub query: janusql_parser::JanusQueryAst,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypedLegacyBaselineMetadata {
    pub definitions: Vec<TypedLegacyBaselineDefinition>,
    pub uses: Vec<String>,
}

/// Build Janus's execution-facing baseline definition from the legacy shell
/// and the already parsed standalone Core AST.
///
/// The retained syntax strings are compatibility/generation inputs only. All
/// semantic extraction (projections and window dependencies) is structural.
pub(crate) fn build_baseline_definition_from_typed(
    typed: &TypedLegacyBaselineDefinition,
) -> Result<BaselineDefinition, Box<dyn std::error::Error>> {
    let name = resolve_legacy_identifier(&typed.name, &typed.query);
    let source_window = resolve_legacy_identifier(&typed.source_window, &typed.query);
    let mut source_windows = vec![source_window.clone()];
    collect_window_dependencies(&typed.query.where_clause.patterns, &mut source_windows);

    Ok(BaselineDefinition {
        name,
        source_window,
        source_windows,
        // The legacy body is retained for diagnostics and source preservation;
        // it is deliberately not used for semantic extraction above.
        raw_query: typed.raw_body.clone(),
        select_clause: typed.query.select.raw.clone(),
        where_clause: typed.query.where_clause.raw.clone(),
        group_by_clause: typed.query.group_by.as_ref().map(|group| group.raw.clone()),
        having_clause: typed.query.having.as_ref().map(|having| having.raw.clone()),
        output_variables: projected_output_variables(&typed.query),
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    })
}

fn resolve_legacy_identifier(identifier: &str, query: &JanusQueryAst) -> String {
    if identifier.starts_with('<') && identifier.ends_with('>') {
        return identifier[1..identifier.len() - 1].to_string();
    }
    let Some((prefix, local)) = identifier.split_once(':') else {
        return identifier.to_string();
    };
    query
        .prefixes
        .iter()
        .find(|declaration| declaration.prefix == prefix)
        .map_or_else(
            || identifier.to_string(),
            |declaration| format!("{}{local}", declaration.namespace),
        )
}

fn canonical_iri(iri: &IriReference) -> String {
    iri.resolved.clone().unwrap_or_else(|| iri.lexical.clone())
}

fn collect_window_dependencies(patterns: &[GraphPattern], output: &mut Vec<String>) {
    for pattern in patterns {
        match pattern {
            GraphPattern::Window { window } => {
                let window_name = canonical_iri(&window.window);
                if !output.contains(&window_name) {
                    output.push(window_name);
                }
                collect_window_dependencies(&window.patterns, output);
            }
            GraphPattern::Graph { graph } => collect_window_dependencies(&graph.patterns, output),
            GraphPattern::Group { patterns, .. } => collect_window_dependencies(patterns, output),
            GraphPattern::Union { branches, .. } => {
                for branch in branches {
                    collect_window_dependencies(branch, output);
                }
            }
            GraphPattern::Subquery { query } => {
                collect_window_dependencies(&query.where_clause.patterns, output)
            }
            GraphPattern::Triple { .. }
            | GraphPattern::Filter { .. }
            | GraphPattern::Service { .. }
            | GraphPattern::Raw { .. } => {}
        }
    }
}

fn projected_output_variables(query: &JanusQueryAst) -> Vec<String> {
    query
        .select
        .items
        .iter()
        .filter_map(|item| match item {
            ProjectionItem::Variable { variable, .. } => Some(variable.name.clone()),
            ProjectionItem::AggregateAlias { alias, .. }
            | ProjectionItem::ExpressionAlias { alias, .. } => Some(alias.name.clone()),
            // Opaque projections are not semantically interpretable and must
            // not accidentally promote expression-input variables to outputs.
            ProjectionItem::Raw { .. } => None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreprocessedJanusQuery {
    pub(crate) core_source: String,
    pub(crate) legacy: LegacyBaselineMetadata,
}

pub(crate) fn preprocess(
    source: &str,
) -> Result<PreprocessedJanusQuery, Box<dyn std::error::Error>> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut core = Vec::with_capacity(lines.len());
    let mut legacy = LegacyBaselineMetadata::default();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.starts_with("USING BASELINE") {
            legacy.uses.push(line.to_string());
            index += 1;
        } else if trimmed.starts_with("DEFINE BASELINE") {
            let mut definition = vec![line.to_string()];
            index += 1;
            let mut saw_where = false;
            let mut depth = 0usize;
            while index < lines.len() {
                let candidate = lines[index];
                let candidate_trimmed = candidate.trim();
                if saw_where
                    && depth == 0
                    && (candidate_trimmed.starts_with("REGISTER")
                        || candidate_trimmed.starts_with("DEFINE BASELINE"))
                {
                    break;
                }
                if candidate_trimmed.contains("WHERE") {
                    saw_where = true;
                }
                if saw_where {
                    depth += candidate.chars().filter(|c| *c == '{').count();
                    depth = depth
                        .checked_sub(candidate.chars().filter(|c| *c == '}').count())
                        .ok_or_else(|| {
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                "DEFINE BASELINE has an unmatched closing brace",
                            )) as Box<dyn std::error::Error>
                        })?;
                }
                definition.push(candidate.to_string());
                index += 1;
            }
            if !saw_where || depth != 0 {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "DEFINE BASELINE must contain a balanced WHERE body",
                )));
            }
            let raw_definition = definition.join("\n");
            let header = definition[0].trim().split_whitespace().collect::<Vec<_>>();
            if header.len() != 7
                || header[0] != "DEFINE"
                || header[1] != "BASELINE"
                || header[3] != "ON"
                || header[4] != "WINDOW"
                || header[6] != "AS"
            {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "invalid DEFINE BASELINE shell",
                )));
            }
            legacy.raw_definitions.push(RawLegacyBaselineDefinition {
                name: header[2].to_string(),
                source_window: header[5].to_string(),
                raw_body: definition[1..].join("\n").trim().to_string(),
                body_span: None,
            });
            legacy.definitions.push(raw_definition);
        } else {
            core.push(line);
            index += 1;
        }
    }
    Ok(PreprocessedJanusQuery { core_source: core.join("\n"), legacy })
}

pub(crate) fn type_legacy_baselines(
    main_ast: &janusql_parser::JanusQueryAst,
    raw: &LegacyBaselineMetadata,
) -> Result<TypedLegacyBaselineMetadata, Box<dyn std::error::Error>> {
    let mut typed = TypedLegacyBaselineMetadata { uses: raw.uses.clone(), ..Default::default() };
    for definition in &raw.raw_definitions {
        let _declared_source_window = main_ast
            .windows
            .iter()
            .find(|window| {
                window.name.lexical == definition.source_window
                    || window.name.resolved.as_deref() == Some(definition.source_window.as_str())
            })
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "DEFINE BASELINE references unknown source window '{}'",
                        definition.source_window
                    ),
                )) as Box<dyn std::error::Error>
            })?;
        let mut wrapper = main_ast
            .prefixes
            .iter()
            .map(|prefix| format!("PREFIX {}: <{}>", prefix.prefix, prefix.namespace))
            .collect::<Vec<_>>()
            .join("\n");
        // Validate the legacy shell's declared source, then retain every main
        // Core window in the wrapper so typed graph-pattern traversal can
        // represent any additional declared WINDOW dependencies.
        for window in &main_ast.windows {
            wrapper.push_str("\n\nFROM NAMED WINDOW ");
            wrapper.push_str(&window.name.lexical);
            wrapper.push_str(" ON ");
            wrapper.push_str(match window.source_kind {
                janusql_parser::SourceKind::Stream => "STREAM",
                janusql_parser::SourceKind::Log => "LOG",
            });
            wrapper.push(' ');
            wrapper.push_str(&window.source.lexical);
            wrapper.push_str(" [");
            wrapper.push_str(&match &window.spec {
                janusql_parser::WindowSpec::LiveSliding { range, step } => {
                    format!("RANGE {range} STEP {step}")
                }
                janusql_parser::WindowSpec::HistoricalSliding { offset, range, step } => {
                    format!("OFFSET {offset} RANGE {range} STEP {step}")
                }
                janusql_parser::WindowSpec::HistoricalFixed { start, end } => {
                    format!("START {start} END {end}")
                }
            });
            wrapper.push(']');
        }
        wrapper.push_str("\n\n");
        wrapper.push_str(&definition.raw_body);
        let query = janusql_parser::parse(&wrapper)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
        typed.definitions.push(TypedLegacyBaselineDefinition {
            name: definition.name.clone(),
            source_window: definition.source_window.clone(),
            raw_body: definition.raw_body.clone(),
            query,
        });
    }
    Ok(typed)
}

#[cfg(test)]
mod tests {
    use super::{
        build_baseline_definition_from_typed, preprocess, type_legacy_baselines,
        TypedLegacyBaselineDefinition,
    };
    use crate::parsing::janusql_parser::{ast::HistoricalMaterializationKind, JanusQLParser};

    fn typed_definition(source: &str) -> TypedLegacyBaselineDefinition {
        let preprocessed = preprocess(source).expect("legacy preprocessing");
        let main_ast = janusql_parser::parse(&preprocessed.core_source).expect("main Core query");
        let typed =
            type_legacy_baselines(&main_ast, &preprocessed.legacy).expect("typed legacy baseline");
        assert_eq!(typed.definitions.len(), 1);
        typed.definitions.into_iter().next().expect("one definition")
    }

    fn normalized_whitespace(value: &str) -> String {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn assert_parity(source: &str) {
        // The production compatibility path remains the parity oracle.
        let old = JanusQLParser::new()
            .expect("parser")
            .parse(source)
            .expect("old compatibility lowering")
            .lowered
            .baseline_definitions
            .into_iter()
            .next()
            .expect("old baseline definition");

        // The typed path deliberately follows the migration architecture and
        // never calls the old builder or compatibility lowerer.
        let typed = typed_definition(source);
        let definition = build_baseline_definition_from_typed(&typed).expect("typed builder");

        assert_eq!(definition.name, old.name);
        assert_eq!(definition.source_window, old.source_window);
        assert_eq!(definition.source_windows, old.source_windows);
        assert_eq!(definition.raw_query, old.raw_query);
        assert_eq!(
            normalized_whitespace(&definition.select_clause),
            normalized_whitespace(&old.select_clause)
        );
        // The legacy line parser retains indentation before its first WHERE
        // line; the standalone span deliberately begins at `W`. Their token
        // content is identical, while the typed definition itself keeps the
        // exact canonical source slice below.
        assert_eq!(
            normalized_whitespace(&definition.where_clause),
            normalized_whitespace(&old.where_clause)
        );
        assert_eq!(definition.group_by_clause, old.group_by_clause);
        assert_eq!(definition.having_clause, old.having_clause);
        assert_eq!(definition.output_variables, old.output_variables);
        assert_eq!(definition.materialization_kind, old.materialization_kind);
        assert_eq!(
            typed.query.where_clause.raw, definition.where_clause,
            "the typed builder must consume the whole preserved WHERE source directly"
        );
    }

    #[test]
    fn typed_baseline_builder_matches_simple_projection_compatibility_path() {
        let source = r#"
            PREFIX ex: <http://example.org/>
            FROM NAMED WINDOW ex:history ON LOG ex:store [START 0 END 10]
            DEFINE BASELINE ex:baseline ON WINDOW ex:history AS
            SELECT ?sensor ?value
            WHERE {
                WINDOW ex:history {
                    ?sensor ex:value ?value .
                }
            }
            REGISTER RStream ex:out AS
            USING BASELINE ex:baseline
            SELECT ?sensor ?value
            FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 10 STEP 10]
            WHERE { WINDOW ex:live { ?sensor ex:value ?value . } }
        "#;
        assert_parity(source);

        let definition = build_baseline_definition_from_typed(&typed_definition(source)).unwrap();
        assert_eq!(definition.output_variables, vec!["?sensor", "?value"]);
        assert_eq!(definition.source_windows, vec!["http://example.org/history"]);
    }

    #[test]
    fn typed_baseline_builder_derives_aliases_groups_having_and_dependencies() {
        let source = r#"
            PREFIX ex: <http://example.org/>
            FROM NAMED WINDOW ex:history ON LOG ex:store [START 0 END 10]
            DEFINE BASELINE ex:baseline ON WINDOW ex:history AS
            SELECT ?sensor
                   (AVG(?value) AS ?avg)
                   ((AVG(?value) - 1) AS ?difference)
            WHERE {
                GRAPH ex:archive {
                    WINDOW ex:history { ?sensor ex:value ?value . }
                }
                { SELECT ?sensor WHERE { WINDOW ex:history { ?sensor ex:oldValue ?value . } } }
            }
            GROUP BY ?sensor
            HAVING(AVG(?value) > 1)
            REGISTER RStream ex:out AS
            USING BASELINE ex:baseline
            SELECT ?sensor ?avg
            FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 10 STEP 10]
            WHERE { WINDOW ex:live { ?sensor ex:value ?value . } GRAPH ex:baseline { ?sensor ex:avg ?avg . } }
        "#;
        assert_parity(source);

        let typed = typed_definition(source);
        let definition = build_baseline_definition_from_typed(&typed).unwrap();
        assert_eq!(
            definition.output_variables,
            vec!["?sensor", "?avg", "?difference"],
            "expression inputs such as ?value are not projected outputs"
        );
        assert_eq!(definition.group_by_clause.as_deref(), Some("GROUP BY ?sensor"));
        assert_eq!(definition.having_clause.as_deref(), Some("HAVING(AVG(?value) > 1)"));
        assert_eq!(
            definition.materialization_kind,
            HistoricalMaterializationKind::ExplicitBaseline
        );
        assert!(definition.where_clause.contains("GRAPH ex:archive"));
        assert!(definition.where_clause.contains("SELECT ?sensor WHERE"));
    }

    #[test]
    fn typed_baseline_builder_collects_graph_and_nested_window_dependencies_in_order() {
        let source = r#"
            PREFIX ex: <http://example.org/>
            FROM NAMED WINDOW ex:history ON LOG ex:historyStore [START 0 END 10]
            FROM NAMED WINDOW ex:secondary ON LOG ex:secondaryStore [START 0 END 10]
            DEFINE BASELINE ex:baseline ON WINDOW ex:history AS
            SELECT ?sensor
            WHERE {
                GRAPH ex:archive { WINDOW ex:secondary { ?sensor ex:value ?value . } }
                { SELECT ?sensor WHERE { WINDOW ex:history { ?sensor ex:oldValue ?value . } } }
            }
            REGISTER RStream ex:out AS
            SELECT ?sensor
            FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 10 STEP 10]
            WHERE { WINDOW ex:live { ?sensor ex:value ?value . } }
        "#;
        let definition = build_baseline_definition_from_typed(&typed_definition(source)).unwrap();

        assert_eq!(
            definition.source_windows,
            vec![
                "http://example.org/history".to_string(),
                "http://example.org/secondary".to_string(),
            ]
        );
    }

    #[test]
    fn typed_baseline_builder_matches_parser_migration_baseline_fixture() {
        // This is the real parser-migration baseline shape from
        // `tests/janusql_parser_test.rs::legacy_baseline_isolated_before_typed_graph_lowering`.
        let source = r#"
            PREFIX ex: <http://example.org/>
            FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
            FROM NAMED WINDOW ex:history ON LOG ex:store [START 0 END 10]
            DEFINE BASELINE ex:dayBaseline ON WINDOW ex:history AS
            SELECT ?sensor
            WHERE { ?sensor ex:value ?value . }
            REGISTER RStream ex:out AS
            USING BASELINE ex:dayBaseline
            SELECT ?sensor
            WHERE {
              WINDOW ex:live { ?sensor ex:value ?value . }
              GRAPH ex:dayBaseline { ?sensor ex:dayValue ?value . }
            }
        "#;
        assert_parity(source);
    }
}
