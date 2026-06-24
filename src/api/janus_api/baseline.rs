use crate::{
    api::janus_api::{
        rdf::{
            normalize_binding_term, normalize_iri_term, resolve_object_template_term,
            resolve_predicate_template_term, resolve_subject_template_term,
        },
        types::JanusApiError,
    },
    core::RDFEvent,
    execution::HistoricalExecutor,
    parsing::janusql_parser::{
        BaselineDefinition, BaselineGraphTemplate, HistoricalMaterializationKind, ParsedJanusQuery,
        WindowDefinition, WindowType,
    },
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::{
        baseline_registry::{BaselineRegistry, BaselineSnapshot},
        query_registry::BaselineBootstrapMode,
    },
    storage::segmented_storage::StreamingSegmentedStorage,
    stream::live_stream_processing::{
        DynamicStaticQuadProvider, LiveStreamProcessing, LiveStreamProcessingError,
    },
};
use oxigraph::model::{GraphName, NamedNode, Quad};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::{mpsc::Receiver, Arc, RwLock},
};

pub(crate) const JANUS_BASELINE_NS: &str = "https://janus.rs/baseline#";

#[derive(Debug, Clone)]
pub(crate) struct BaselineAggregate {
    last_value: String,
    numeric_sum: f64,
    numeric_count: usize,
    all_numeric: bool,
}

pub(crate) fn collect_query_baseline_statements(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    baseline_mode: BaselineBootstrapMode,
    baseline_window_name: Option<&str>,
    shutdown_rx: &Receiver<()>,
) -> Result<Vec<(String, String, String)>, JanusApiError> {
    if parsed.live_windows.is_empty() || parsed.historical_windows.is_empty() {
        return Ok(Vec::new());
    }

    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let mut statements = Vec::new();

    for (index, window) in parsed.historical_windows.iter().enumerate() {
        if shutdown_rx.try_recv().is_ok() {
            return Ok(Vec::new());
        }
        if baseline_window_name.is_some_and(|name| name != window.window_name) {
            continue;
        }

        let Some(sparql_query) = parsed.sparql_queries.get(index) else {
            // Query-defined baselines may be the only consumer of a historical window.
            // In that case there is no main historical SPARQL query to materialize here.
            continue;
        };

        match window.window_type {
            WindowType::HistoricalFixed => {
                let bindings = executor.execute_fixed_window(window, sparql_query)?;
                statements.extend(baseline_statements_from_bindings(&bindings));
            }
            WindowType::HistoricalSliding => {
                statements.extend(collect_sliding_window_baseline_statements(
                    &executor,
                    window,
                    sparql_query,
                    baseline_mode,
                    shutdown_rx,
                )?);
            }
            WindowType::Live => {}
        }
    }

    Ok(statements)
}

pub(crate) fn initialize_fixed_query_defined_baselines(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    baseline_registry: &Arc<BaselineRegistry>,
    latest_rows: &Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
) -> Result<(), JanusApiError> {
    for definition in &parsed.ast.baseline_definitions {
        let source_windows = find_baseline_source_windows(parsed, definition)?;
        if source_windows
            .iter()
            .any(|source_window| source_window.window_type != WindowType::HistoricalFixed)
        {
            continue;
        }

        let evaluation_time = source_windows
            .iter()
            .filter_map(|source_window| source_window.end)
            .max()
            .unwrap_or_default();
        let snapshot = load_or_compute_baseline_snapshot(
            storage,
            parsed,
            definition,
            evaluation_time,
            baseline_registry,
        )?;
        store_latest_baseline_rows(latest_rows, &snapshot);
    }

    Ok(())
}

pub(crate) fn build_query_defined_baseline_provider(
    storage: Arc<StreamingSegmentedStorage>,
    parsed: ParsedJanusQuery,
    baseline_registry: Arc<BaselineRegistry>,
    latest_rows: Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
) -> DynamicStaticQuadProvider {
    Arc::new(move |evaluation_time| {
        resolve_query_defined_baseline_quads_at(
            &storage,
            &parsed,
            &baseline_registry,
            &latest_rows,
            evaluation_time,
        )
        .map_err(|err| LiveStreamProcessingError::from(err.to_string()))
    })
}

pub(crate) fn resolve_query_defined_baseline_quads_at(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    baseline_registry: &Arc<BaselineRegistry>,
    latest_rows: &Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
    evaluation_time: u64,
) -> Result<Vec<Quad>, JanusApiError> {
    let mut materialized = Vec::new();
    let mut seen = HashSet::new();

    for baseline_use in &parsed.ast.baseline_uses {
        if !seen.insert(baseline_use.name.clone()) {
            continue;
        }

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
        let snapshot = load_or_compute_baseline_snapshot(
            storage,
            parsed,
            definition,
            evaluation_time,
            baseline_registry,
        )?;
        store_latest_baseline_rows(latest_rows, &snapshot);
        materialized
            .extend(materialize_baseline_snapshot_as_quads(definition, template, &snapshot)?);
    }

    Ok(materialized)
}

pub(crate) fn load_or_compute_baseline_snapshot(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    definition: &BaselineDefinition,
    evaluation_time: u64,
    baseline_registry: &Arc<BaselineRegistry>,
) -> Result<BaselineSnapshot, JanusApiError> {
    let source_windows = find_baseline_source_windows(parsed, definition)?;
    let generated_query = parsed
        .generated_baseline_queries
        .iter()
        .find(|generated| generated.name == definition.name)
        .ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "Missing generated baseline query for '{}'",
                definition.name
            ))
        })?;

    let resolved_valid_at = if source_windows
        .iter()
        .all(|source_window| source_window.window_type == WindowType::HistoricalFixed)
    {
        source_windows
            .iter()
            .filter_map(|source_window| source_window.end)
            .max()
            .unwrap_or(evaluation_time)
    } else if source_windows
        .iter()
        .all(|source_window| source_window.window_type == WindowType::HistoricalSliding)
    {
        evaluation_time
    } else {
        return Err(JanusApiError::ExecutionError(format!(
            "Historical materialization '{}' cannot mix fixed and sliding historical windows",
            definition.name
        )));
    };

    if let Some(snapshot) = baseline_registry.get_snapshot(&definition.name, resolved_valid_at) {
        return Ok(snapshot);
    }
    if source_windows
        .iter()
        .all(|source_window| source_window.window_type == WindowType::HistoricalFixed)
    {
        if let Some(snapshot) = baseline_registry.get_latest_snapshot(&definition.name) {
            return Ok(snapshot);
        }
    }

    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let rows = match definition.materialization_kind {
        HistoricalMaterializationKind::ExplicitBaseline if source_windows.len() == 1 => {
            let source_window = source_windows[0];
            let (window_start, window_end) =
                source_window.resolve_historical_bounds(evaluation_time).ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Failed to resolve historical bounds for baseline '{}' using window '{}'",
                        definition.name, source_window.window_name
                    ))
                })?;
            executor.execute_window_bounds(
                window_start,
                window_end,
                &generated_query.sparql_query,
            )?
        }
        _ => executor.execute_materialized_historical_subquery(
            &source_windows,
            &generated_query.sparql_query,
            evaluation_time,
        )?,
    };
    let (window_start, window_end) = aggregate_window_bounds(&source_windows, evaluation_time)?;
    let snapshot = BaselineSnapshot {
        baseline_id: definition.name.clone(),
        valid_at: resolved_valid_at,
        source_window: definition.source_window.clone(),
        window_start,
        window_end,
        variables: generated_query.output_variables.clone(),
        rows,
    };
    baseline_registry.insert_snapshot(snapshot.clone());
    Ok(snapshot)
}

pub(crate) fn find_baseline_source_windows<'a>(
    parsed: &'a ParsedJanusQuery,
    definition: &BaselineDefinition,
) -> Result<Vec<&'a WindowDefinition>, JanusApiError> {
    definition
        .source_windows
        .iter()
        .map(|source_window_name| {
            parsed
                .historical_windows
                .iter()
                .find(|window| window.window_name == *source_window_name)
                .ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Missing historical source window '{}' for materialization '{}'",
                        source_window_name, definition.name
                    ))
                })
        })
        .collect()
}

pub(crate) fn store_latest_baseline_rows(
    latest_rows: &Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
    snapshot: &BaselineSnapshot,
) {
    if let Ok(mut stored) = latest_rows.write() {
        stored.insert(snapshot.baseline_id.clone(), snapshot.rows.clone());
    }
}

pub(crate) fn aggregate_window_bounds(
    source_windows: &[&WindowDefinition],
    evaluation_time: u64,
) -> Result<(u64, u64), JanusApiError> {
    let mut starts = Vec::new();
    let mut ends = Vec::new();

    for source_window in source_windows {
        let (start, end) =
            source_window.resolve_historical_bounds(evaluation_time).ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "Failed to resolve historical bounds for window '{}'",
                    source_window.window_name
                ))
            })?;
        starts.push(start);
        ends.push(end);
    }

    let start = starts.into_iter().min().unwrap_or_default();
    let end = ends.into_iter().max().unwrap_or_default();
    Ok((start, end))
}

pub(crate) fn materialize_baseline_snapshot_as_quads(
    baseline_definition: &BaselineDefinition,
    baseline_graph_template: &BaselineGraphTemplate,
    snapshot: &BaselineSnapshot,
) -> Result<Vec<Quad>, JanusApiError> {
    materialize_baseline_bindings_as_quads(
        &snapshot.baseline_id,
        baseline_definition,
        baseline_graph_template,
        &snapshot.rows,
    )
}

#[allow(dead_code)]
pub(crate) fn collect_query_defined_baseline_bindings(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    shutdown_rx: &Receiver<()>,
) -> Result<HashMap<String, Vec<HashMap<String, String>>>, JanusApiError> {
    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let mut baseline_results = HashMap::new();

    for generated in &parsed.generated_baseline_queries {
        if shutdown_rx.try_recv().is_ok() {
            return Ok(HashMap::new());
        }

        let source_windows = generated
            .source_windows
            .iter()
            .map(|source_window_name| {
                parsed
                    .historical_windows
                    .iter()
                    .find(|window| window.window_name == *source_window_name)
                    .ok_or_else(|| {
                        JanusApiError::ExecutionError(format!(
                            "Missing historical source window '{}' for generated baseline '{}'",
                            source_window_name, generated.name
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let bindings = execute_generated_baseline_query(
            &executor,
            &source_windows,
            &generated.sparql_query,
            shutdown_rx,
        )?;
        baseline_results.insert(generated.name.clone(), bindings);
    }

    Ok(baseline_results)
}

#[allow(dead_code)]
pub(crate) fn evaluate_and_materialize_query_defined_baselines(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    shutdown_rx: &Receiver<()>,
) -> Result<(HashMap<String, Vec<HashMap<String, String>>>, Vec<Quad>), JanusApiError> {
    let bindings_by_name = collect_query_defined_baseline_bindings(storage, parsed, shutdown_rx)?;
    let quads = materialize_query_defined_baseline_quads(parsed, &bindings_by_name)?;
    Ok((bindings_by_name, quads))
}

#[allow(dead_code)]
pub(crate) fn execute_generated_baseline_query(
    executor: &HistoricalExecutor,
    windows: &[&WindowDefinition],
    sparql_query: &str,
    shutdown_rx: &Receiver<()>,
) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
    if windows.is_empty() {
        return Err(JanusApiError::ExecutionError(
            "Generated historical materialization query requires at least one source window"
                .to_string(),
        ));
    }

    if windows.len() > 1 {
        let evaluation_time =
            windows.iter().filter_map(|window| window.end).max().unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            });
        return executor.execute_materialized_historical_subquery(
            windows,
            sparql_query,
            evaluation_time,
        );
    }

    let window = windows[0];
    match window.window_type {
        WindowType::HistoricalFixed => executor.execute_fixed_window(window, sparql_query),
        WindowType::HistoricalSliding => {
            let mut latest_bindings = Vec::new();

            for window_result in executor.execute_sliding_windows(window, sparql_query) {
                if shutdown_rx.try_recv().is_ok() {
                    return Ok(Vec::new());
                }
                latest_bindings = window_result?;
            }

            Ok(latest_bindings)
        }
        WindowType::Live => Err(JanusApiError::ExecutionError(format!(
            "Generated baseline query cannot execute on live window '{}'",
            window.window_name
        ))),
    }
}

pub(crate) fn collect_sliding_window_baseline_statements(
    executor: &HistoricalExecutor,
    window: &WindowDefinition,
    sparql_query: &str,
    mode: BaselineBootstrapMode,
    shutdown_rx: &Receiver<()>,
) -> Result<Vec<(String, String, String)>, JanusApiError> {
    let mut accumulator = HashMap::new();
    let mut saw_window = false;

    for window_result in executor.execute_sliding_windows(window, sparql_query) {
        if shutdown_rx.try_recv().is_ok() {
            return Ok(Vec::new());
        }
        let bindings = window_result?;
        saw_window = true;

        if mode == BaselineBootstrapMode::Last {
            accumulator.clear();
        }

        accumulate_bindings_into_baseline(&mut accumulator, &bindings);
    }

    if !saw_window {
        return Ok(Vec::new());
    }

    Ok(baseline_statements_from_accumulator(&accumulator))
}

#[allow(dead_code)]
pub(crate) fn materialize_query_defined_baseline_quads(
    parsed: &ParsedJanusQuery,
    bindings_by_name: &HashMap<String, Vec<HashMap<String, String>>>,
) -> Result<Vec<Quad>, JanusApiError> {
    let mut materialized = Vec::new();
    let mut seen = HashSet::new();

    for baseline_use in &parsed.ast.baseline_uses {
        if !seen.insert(baseline_use.name.clone()) {
            continue;
        }

        // The GRAPH template is the materialization contract. We use it instead of
        // SELECT alias heuristics because the template explicitly states the RDF
        // shape that should be injected into the live static store.
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
        let bindings = bindings_by_name.get(&baseline_use.name).ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "USING BASELINE references missing evaluated baseline '{}'",
                baseline_use.name
            ))
        })?;
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
        materialized.extend(materialize_baseline_bindings_as_quads(
            &baseline_use.name,
            definition,
            template,
            bindings,
        )?);
    }

    Ok(materialized)
}

#[cfg(test)]
pub(crate) fn materialize_bindings_as_static_baseline(
    processor: &mut LiveStreamProcessing,
    bindings: &[HashMap<String, String>],
) -> Result<(), JanusApiError> {
    let statements = baseline_statements_from_bindings(bindings);
    materialize_static_baseline_statements(processor, &statements)
}

#[allow(dead_code)]
pub(crate) fn materialize_static_quads(
    processor: &mut LiveStreamProcessing,
    quads: &[Quad],
) -> Result<(), JanusApiError> {
    for quad in quads {
        processor.add_static_quad(quad.clone());
    }
    Ok(())
}

pub(crate) fn materialize_static_baseline_statements(
    processor: &mut LiveStreamProcessing,
    statements: &[(String, String, String)],
) -> Result<(), JanusApiError> {
    for (subject, predicate, object) in statements {
        processor
            .add_static_data(RDFEvent::new(0, subject, predicate, object, ""))
            .map_err(|e| {
                JanusApiError::LiveProcessingError(format!(
                    "Failed to materialize baseline statement '{} {} {}': {}",
                    subject, predicate, object, e
                ))
            })?;
    }
    Ok(())
}

pub(crate) fn materialize_baseline_bindings_as_quads(
    baseline_name: &str,
    baseline_definition: &BaselineDefinition,
    baseline_graph_template: &BaselineGraphTemplate,
    bindings: &[HashMap<String, String>],
) -> Result<Vec<Quad>, JanusApiError> {
    let _ = baseline_definition;
    let graph_name = GraphName::NamedNode(NamedNode::new(baseline_name).map_err(|e| {
        JanusApiError::ExecutionError(format!(
            "Invalid baseline graph name '{}': {}",
            baseline_name, e
        ))
    })?);

    let mut quads = Vec::new();
    for binding in bindings {
        for triple in &baseline_graph_template.triples {
            let subject = resolve_subject_template_term(baseline_name, triple, binding)?;
            let predicate = resolve_predicate_template_term(baseline_name, triple, binding)?;
            let object = resolve_object_template_term(baseline_name, triple, binding)?;
            quads.push(Quad::new(subject, predicate, object, graph_name.clone()));
        }
    }

    Ok(quads)
}

pub(crate) fn baseline_statements_from_bindings(
    bindings: &[HashMap<String, String>],
) -> Vec<(String, String, String)> {
    let mut accumulator = HashMap::new();
    accumulate_bindings_into_baseline(&mut accumulator, bindings);
    baseline_statements_from_accumulator(&accumulator)
}

pub(crate) fn accumulate_bindings_into_baseline(
    accumulator: &mut HashMap<(String, String), BaselineAggregate>,
    bindings: &[HashMap<String, String>],
) {
    for binding in bindings {
        let Some((anchor_var, anchor_subject)) = select_binding_anchor(binding) else {
            continue;
        };

        let mut variables = binding.keys().cloned().collect::<Vec<_>>();
        variables.sort_unstable();

        for var in variables {
            if var == anchor_var {
                continue;
            }

            let Some(raw_value) = binding.get(&var) else {
                continue;
            };

            let normalized = normalize_binding_term(raw_value);
            let key = (anchor_subject.clone(), var);
            let entry = accumulator.entry(key).or_insert_with(|| BaselineAggregate {
                last_value: normalized.clone(),
                numeric_sum: 0.0,
                numeric_count: 0,
                all_numeric: true,
            });

            entry.last_value.clone_from(&normalized);
            if let Ok(value) = normalized.parse::<f64>() {
                entry.numeric_sum += value;
                entry.numeric_count += 1;
            } else {
                entry.all_numeric = false;
            }
        }
    }
}

pub(crate) fn baseline_statements_from_accumulator(
    accumulator: &HashMap<(String, String), BaselineAggregate>,
) -> Vec<(String, String, String)> {
    let mut entries = accumulator.iter().collect::<Vec<_>>();
    entries.sort_by(|((left_subject, left_var), _), ((right_subject, right_var), _)| {
        match left_subject.cmp(right_subject) {
            Ordering::Equal => left_var.cmp(right_var),
            other => other,
        }
    });

    entries
        .into_iter()
        .map(|((subject, var), aggregate)| {
            let predicate = format!("{JANUS_BASELINE_NS}{var}");
            let object = if aggregate.all_numeric && aggregate.numeric_count > 0 {
                (aggregate.numeric_sum / aggregate.numeric_count as f64).to_string()
            } else {
                aggregate.last_value.clone()
            };
            (subject.clone(), predicate, object)
        })
        .collect()
}

pub(crate) fn select_binding_anchor(binding: &HashMap<String, String>) -> Option<(String, String)> {
    for preferred in ["sensor", "subject", "entity", "s"] {
        if let Some(value) = binding.get(preferred).and_then(|raw| normalize_iri_term(raw)) {
            return Some((preferred.to_string(), value));
        }
    }

    let mut entries = binding.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_name, _), (right_name, _)| {
        if left_name == right_name {
            Ordering::Equal
        } else {
            left_name.cmp(right_name)
        }
    });

    entries
        .into_iter()
        .find_map(|(name, raw)| normalize_iri_term(raw).map(|value| (name.clone(), value)))
}
