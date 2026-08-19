use super::data_gen::prepare_coordination_workload;
use super::helpers::{
    canonical_result_hash, collect_live_results, event_payloads, historical_input_hash,
    join_live_with_baseline, live_input_hash, live_only_rspql,
    materialize_bindings_as_static_baseline, materialized_baseline_rows_from_bindings, now_ms,
    publish_live_events,
};
use super::io::write_h1_debug_artifacts;
use super::types::{
    CoordinationPair, CoordinationRow, CoordinationRunConfig, CoordinationSystem, ExecutionMode,
    LIVE_STREAM_URI,
};
use crate::{
    execution::HistoricalExecutor, parsing::janusql_parser::JanusQLParser,
    querying::oxigraph_adapter::OxigraphAdapter,
    stream::live_stream_processing::LiveStreamProcessing,
};
use std::sync::Arc;
use std::time::Duration;

pub fn run_coordination_pair(
    config: CoordinationRunConfig<'_>,
) -> Result<CoordinationPair, Box<dyn std::error::Error>> {
    let unified = run_coordination_system(CoordinationSystem::JanusUnified, &config)?;
    let decomposed = run_coordination_system(CoordinationSystem::DecomposedOxigraph, &config)?;

    let historical_equivalent = unified.historical_result_hash == decomposed.historical_result_hash;
    let hybrid_equivalent = unified.hybrid_result_hash == decomposed.hybrid_result_hash
        && unified.hybrid_result_count == decomposed.hybrid_result_count;

    let mut unified = unified;
    let mut decomposed = decomposed;

    unified.historical_equivalent_to_baseline = Some(historical_equivalent);
    unified.hybrid_equivalent_to_baseline = Some(hybrid_equivalent);
    unified.equivalent_to_baseline = Some(hybrid_equivalent); // hybrid is primary equivalence target

    decomposed.historical_equivalent_to_baseline = None;
    decomposed.hybrid_equivalent_to_baseline = None;
    decomposed.equivalent_to_baseline = None;

    if let Some(debug_output_dir) = config.debug_output_dir {
        write_h1_debug_artifacts(debug_output_dir, &unified, &decomposed, &config)?;
    }

    Ok(CoordinationPair { unified, decomposed })
}

pub fn run_coordination_system(
    system: CoordinationSystem,
    config: &CoordinationRunConfig<'_>,
) -> Result<CoordinationRow, Box<dyn std::error::Error>> {
    let owned_workload;
    let workload = if config.mode == ExecutionMode::Warm {
        config.warm_workload.ok_or("warm mode requires prepared workload")?
    } else {
        owned_workload =
            prepare_coordination_workload(config.historical_events, config.live_events)?;
        &owned_workload
    };

    let client_start = now_ms();
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&workload.hybrid_query)?;
    let query_registered = now_ms();

    let historical_start = now_ms();
    let executor =
        HistoricalExecutor::new(Arc::clone(&workload.historical_storage), OxigraphAdapter::new());
    let baseline_bindings = executor.execute_fixed_window(
        parsed.historical_windows.first().ok_or("missing historical window")?,
        &workload.historical_sparql_query,
    )?;
    let historical_done = now_ms();

    let live_events_published = workload.live_events.len();
    let live_events_processed = workload.live_events.len();
    let live_window_size = 10000;
    let live_window_slide = 1000;
    let live_first_event_at = workload.live_events.first().map(|e| e.timestamp).unwrap_or(0);

    match system {
        CoordinationSystem::JanusUnified => {
            let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
            processor.register_stream(LIVE_STREAM_URI)?;
            materialize_bindings_as_static_baseline(&mut processor, &baseline_bindings)?;
            processor.start_processing()?;
            let live_ready = now_ms();
            publish_live_events(&processor, &workload.live_events)?;
            let first_event_published = now_ms();
            let live_collection = if workload.live_events.is_empty() {
                super::helpers::LiveCollectionResult {
                    first_result_engine_ms: now_ms(),
                    all_rows: Vec::new(),
                }
            } else {
                collect_live_results(
                    &processor,
                    Duration::from_millis(500),
                    Duration::from_millis(10),
                )?
            };
            let result_rows = live_collection.all_rows;
            let first_result_engine = live_collection.first_result_engine_ms;
            let first_result_client = now_ms();

            let estimated_useful_engine_work_ms = (historical_done - historical_start) as f64
                + (first_result_engine - first_event_published) as f64;
            let e2e_latency_ms = (first_result_client - client_start) as f64;

            let live_stream_processing_latency_ms =
                (first_result_engine as f64 - first_event_published as f64).max(0.0);
            let external_join_latency_ms = 0.0;
            let first_hybrid_result_latency_ms = e2e_latency_ms;

            let historical_result_count = baseline_bindings.len();
            let historical_result_hash = canonical_result_hash(&baseline_bindings)?;
            let live_result_count = 0;
            let live_result_hash = None;
            let hybrid_result_count = result_rows.len();
            let hybrid_result_hash = canonical_result_hash(&result_rows)?;

            Ok(CoordinationRow {
                system: system.as_str().to_string(),
                mode: config.mode.as_str().to_string(),
                is_warmup: config.is_warmup,
                run_index: config.run_index,
                historical_events: config.historical_events,
                live_events: config.live_events,
                client_start,
                query_registered,
                historical_start,
                historical_done,
                live_ready,
                first_event_published,
                first_result_engine,
                first_result_client,
                e2e_latency_ms,
                estimated_useful_engine_work_ms,
                estimated_coordination_overhead_ms: (e2e_latency_ms
                    - estimated_useful_engine_work_ms)
                    .max(0.0),
                historical_intermediate_bytes: serde_json::to_vec(&baseline_bindings)?.len(),
                live_intermediate_bytes: serde_json::to_vec(&event_payloads(
                    &workload.live_events,
                ))?
                .len(),
                estimated_external_transfer_bytes: 0,
                final_result_bytes: serde_json::to_vec(&result_rows)?.len(),
                components: 1,
                process_boundaries: 0,
                serialization_steps: 1,
                result_count: result_rows.len(),
                historical_input_hash: historical_input_hash(&workload.historical_rdf_events)?,
                live_input_hash: live_input_hash(&workload.live_events)?,
                result_hash: canonical_result_hash(&result_rows)?,
                equivalent_to_baseline: None,
                metadata: config.metadata.clone(),

                live_events_published,
                live_events_processed,
                live_window_size,
                live_window_slide,
                live_query_registered_at: query_registered,
                live_first_event_at,
                live_first_window_result_at: first_result_engine,
                live_stream_processing_latency_ms,
                external_join_latency_ms,
                first_hybrid_result_latency_ms,
                historical_result_count,
                historical_result_hash,
                live_result_count,
                live_result_hash,
                hybrid_result_count,
                hybrid_result_hash,
                historical_equivalent_to_baseline: None,
                hybrid_equivalent_to_baseline: None,
            })
        }
        CoordinationSystem::DecomposedOxigraph => {
            let external_bindings = config.adapter.execute_bindings_query(
                &workload.historical_sparql_query,
                &workload.historical_rdf_events,
            )?;
            let materialized_baseline_rows = materialized_baseline_rows_from_bindings(
                &external_bindings,
                "historicalAvgCongestion",
            );
            let historical_done = now_ms();
            let mut processor = LiveStreamProcessing::new(live_only_rspql())?;
            processor.register_stream(LIVE_STREAM_URI)?;
            processor.start_processing()?;
            let live_ready = now_ms();
            publish_live_events(&processor, &workload.live_events)?;
            let first_event_published = now_ms();
            let live_collection = if workload.live_events.is_empty() {
                super::helpers::LiveCollectionResult {
                    first_result_engine_ms: now_ms(),
                    all_rows: Vec::new(),
                }
            } else {
                collect_live_results(
                    &processor,
                    Duration::from_millis(500),
                    Duration::from_millis(10),
                )?
            };
            let live_rows = live_collection.all_rows;
            let joined_rows = join_live_with_baseline(&live_rows, &materialized_baseline_rows);
            let first_result_engine = live_collection.first_result_engine_ms;
            let first_result_client = now_ms();

            let estimated_useful_engine_work_ms = (historical_done - historical_start) as f64
                + (first_result_engine - first_event_published) as f64;
            let e2e_latency_ms = (first_result_client - client_start) as f64;

            let live_stream_processing_latency_ms =
                (first_result_engine as f64 - first_event_published as f64).max(0.0);
            let external_join_latency_ms =
                (first_result_client as f64 - first_result_engine as f64).max(0.0);
            let first_hybrid_result_latency_ms = e2e_latency_ms;

            let historical_result_count = external_bindings.len();
            let historical_result_hash = canonical_result_hash(&external_bindings)?;
            let live_result_count = live_rows.len();
            let live_result_hash = Some(canonical_result_hash(&live_rows)?);
            let hybrid_result_count = joined_rows.len();
            let hybrid_result_hash = canonical_result_hash(&joined_rows)?;

            let historical_intermediate_bytes = serde_json::to_vec(&external_bindings)?.len();
            let live_intermediate_bytes =
                serde_json::to_vec(&event_payloads(&workload.live_events))?.len();
            Ok(CoordinationRow {
                system: system.as_str().to_string(),
                mode: config.mode.as_str().to_string(),
                is_warmup: config.is_warmup,
                run_index: config.run_index,
                historical_events: config.historical_events,
                live_events: config.live_events,
                client_start,
                query_registered,
                historical_start,
                historical_done,
                live_ready,
                first_event_published,
                first_result_engine,
                first_result_client,
                e2e_latency_ms,
                estimated_useful_engine_work_ms,
                estimated_coordination_overhead_ms: (e2e_latency_ms
                    - estimated_useful_engine_work_ms)
                    .max(0.0),
                historical_intermediate_bytes,
                live_intermediate_bytes,
                estimated_external_transfer_bytes: historical_intermediate_bytes
                    + live_intermediate_bytes,
                final_result_bytes: serde_json::to_vec(&joined_rows)?.len(),
                components: 4,
                process_boundaries: 3,
                serialization_steps: 4,
                result_count: joined_rows.len(),
                historical_input_hash: historical_input_hash(&workload.historical_rdf_events)?,
                live_input_hash: live_input_hash(&workload.live_events)?,
                result_hash: canonical_result_hash(&joined_rows)?,
                equivalent_to_baseline: None,
                metadata: config.metadata.clone(),

                live_events_published,
                live_events_processed,
                live_window_size,
                live_window_slide,
                live_query_registered_at: query_registered,
                live_first_event_at,
                live_first_window_result_at: first_result_engine,
                live_stream_processing_latency_ms,
                external_join_latency_ms,
                first_hybrid_result_latency_ms,
                historical_result_count,
                historical_result_hash,
                live_result_count,
                live_result_hash,
                hybrid_result_count,
                hybrid_result_hash,
                historical_equivalent_to_baseline: None,
                hybrid_equivalent_to_baseline: None,
            })
        }
    }
}
