use crate::{
    api::janus_api::types::JanusApiError,
    parsing::janusql_parser::{
        BaselineDefinition, BaselineGraphTemplate, GraphTermTemplate, ParsedJanusQuery, WindowType,
    },
};
use std::collections::HashSet;

pub(crate) fn validate_query_defined_baseline_access(
    parsed: &ParsedJanusQuery,
) -> Result<(), JanusApiError> {
    for baseline_use in &parsed.ast.baseline_uses {
        let template = parsed
            .baseline_graph_templates
            .iter()
            .find(|template| template.baseline_name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE '{}' requires a matching GRAPH reference in the live query",
                    baseline_use.name
                ))
            })?;
        let definition = parsed
            .ast
            .baseline_definitions
            .iter()
            .find(|definition| definition.name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE references missing baseline definition '{}'",
                    baseline_use.name
                ))
            })?;
        validate_baseline_graph_template(definition, template)?;
    }

    Ok(())
}

pub(crate) fn validate_query_defined_baseline_step_alignment(
    parsed: &ParsedJanusQuery,
) -> Result<(), JanusApiError> {
    if parsed.live_windows.is_empty() {
        return Ok(());
    }

    let live_step = parsed.live_windows[0].slide;
    if parsed.live_windows.iter().any(|window| window.slide != live_step) {
        return Err(JanusApiError::ExecutionError(
            "Queries with multiple live STEP values are not supported with USING BASELINE"
                .to_string(),
        ));
    }

    for definition in &parsed.ast.baseline_definitions {
        for source_window_name in &definition.source_windows {
            let Some(source_window) = parsed
                .historical_windows
                .iter()
                .find(|window| window.window_name == *source_window_name)
            else {
                continue;
            };

            if source_window.window_type == WindowType::HistoricalSliding
                && source_window.slide != live_step
            {
                return Err(JanusApiError::ExecutionError(format!(
                    "Sliding historical baseline window '{}' STEP {} must match live STEP {}",
                    source_window.window_name, source_window.slide, live_step
                )));
            }
        }
    }

    Ok(())
}

pub(crate) fn validate_baseline_graph_template(
    baseline_definition: &BaselineDefinition,
    baseline_graph_template: &BaselineGraphTemplate,
) -> Result<(), JanusApiError> {
    let output_variables = baseline_definition
        .output_variables
        .iter()
        .map(|variable| variable.trim_start_matches('?'))
        .collect::<HashSet<_>>();

    for triple in &baseline_graph_template.triples {
        for term in [&triple.subject, &triple.object] {
            if let GraphTermTemplate::Variable(variable_name) = term {
                if !output_variables.contains(variable_name.as_str()) {
                    return Err(JanusApiError::ExecutionError(format!(
                        "GRAPH template for baseline '{}' references variable '?{}' that is not produced by the baseline SELECT output",
                        baseline_graph_template.baseline_name,
                        variable_name
                    )));
                }
            }
        }

        if let GraphTermTemplate::Variable(variable_name) = &triple.predicate {
            return Err(JanusApiError::ExecutionError(format!(
                "GRAPH template for baseline '{}' uses variable predicate '?{}', but predicates must be concrete IRIs for now",
                baseline_graph_template.baseline_name,
                variable_name
            )));
        }
    }

    Ok(())
}
