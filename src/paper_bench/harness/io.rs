use super::helpers::{
    canonical_result_rows, canonical_window_hash, collect_live_results, event_payload_rows,
    historical_baseline_sparql_query, join_live_with_baseline_detailed, live_only_rspql,
    materialize_bindings_as_static_baseline, materialized_baseline_rows_from_bindings,
    publish_live_events,
};
use super::types::{
    CoordinationRow, CoordinationRunConfig, CoordinationSummaryRow, EquivalenceReport,
    ScalingFitRow, ScalingSummaryRow, SustainedRunConfig, SustainedSummaryRow,
    SustainedSystemOutput, LIVE_STREAM_URI,
};
use crate::{
    core::RDFEvent, execution::HistoricalExecutor, parsing::janusql_parser::JanusQLParser,
    querying::oxigraph_adapter::OxigraphAdapter,
    stream::live_stream_processing::LiveStreamProcessing,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Write,
    path::Path,
    sync::Arc,
    time::Duration,
};

pub fn ensure_output_dir(base: &Path) -> std::io::Result<()> {
    fs::create_dir_all(base)
}

pub fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        writeln!(file)?;
    }
    Ok(())
}

pub fn write_coordination_summary_csv(
    path: &Path,
    rows: &[CoordinationSummaryRow],
) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "system,mode,runs,components,process_boundaries,serialization_steps,p50_e2e_latency_ms,p95_e2e_latency_ms,avg_useful_engine_work_ms,avg_coordination_overhead_ms,avg_external_transfer_bytes,avg_final_result_bytes,avg_result_count,avg_live_events_published,avg_live_events_processed,avg_live_stream_processing_latency_ms,avg_external_join_latency_ms,avg_first_hybrid_result_latency_ms,avg_historical_result_count,avg_live_result_count,avg_hybrid_result_count,historical_equivalence_rate,hybrid_equivalence_rate"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
            row.system,
            row.mode,
            row.runs,
            row.components,
            row.process_boundaries,
            row.serialization_steps,
            row.p50_e2e_latency_ms,
            row.p95_e2e_latency_ms,
            row.avg_useful_engine_work_ms,
            row.avg_coordination_overhead_ms,
            row.avg_external_transfer_bytes,
            row.avg_final_result_bytes,
            row.avg_result_count,
            row.avg_live_events_published,
            row.avg_live_events_processed,
            row.avg_live_stream_processing_latency_ms,
            row.avg_external_join_latency_ms,
            row.avg_first_hybrid_result_latency_ms,
            row.avg_historical_result_count,
            row.avg_live_result_count,
            row.avg_hybrid_result_count,
            row.historical_equivalence_rate,
            row.hybrid_equivalence_rate
        )?;
    }
    Ok(())
}

pub fn write_scaling_summary_csv(path: &Path, rows: &[ScalingSummaryRow]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "dataset_size_quads,query_type,mode,runs,logical_quads_scanned,selectivity,result_count,p50_latency_ms,p95_latency_ms,avg_latency_ms,avg_throughput_quads_per_sec,max_peak_rss_mb"
    )?;
    for row in rows {
        let peak_rss = row.max_peak_rss_mb.map_or_else(String::new, |value| format!("{value:.3}"));
        writeln!(
            file,
            "{},{},{},{},{},{:.6},{},{:.3},{:.3},{:.3},{:.3},{}",
            row.dataset_size_quads,
            row.query_type,
            row.mode,
            row.runs,
            row.logical_quads_scanned,
            row.selectivity,
            row.result_count,
            row.p50_latency_ms,
            row.p95_latency_ms,
            row.avg_latency_ms,
            row.avg_throughput_quads_per_sec,
            peak_rss
        )?;
    }
    Ok(())
}

pub fn write_scaling_fit_csv(path: &Path, rows: &[ScalingFitRow]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "query_type,mode,slope_ms_per_100k_quads,intercept_ms,r_squared,number_of_points"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{:.6},{:.6},{:.6},{}",
            row.query_type,
            row.mode,
            row.slope_ms_per_100k_quads,
            row.intercept_ms,
            row.r_squared,
            row.number_of_points
        )?;
    }
    Ok(())
}

pub fn write_trig_events(
    path: &Path,
    events: &[RDFEvent],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;
    let mut grouped = HashMap::<String, Vec<&RDFEvent>>::new();
    for event in events {
        grouped.entry(event.graph.clone()).or_default().push(event);
    }
    let mut graphs = grouped.into_iter().collect::<Vec<_>>();
    graphs.sort_by(|left, right| left.0.cmp(&right.0));
    for (graph, rows) in graphs {
        writeln!(file, "<{}> {{", graph)?;
        for event in rows {
            writeln!(
                file,
                "  <{}> <{}> \"{}\" . # ts={}",
                event.subject, event.predicate, event.object, event.timestamp
            )?;
        }
        writeln!(file, "}}")?;
    }
    Ok(())
}

pub fn write_h1_debug_artifacts(
    base_dir: &Path,
    unified: &CoordinationRow,
    decomposed: &CoordinationRow,
    config: &CoordinationRunConfig<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let run_dir = base_dir.join("h1_equivalence_debug").join(format!(
        "run_{:03}_{}",
        config.run_index,
        if config.is_warmup {
            "warmup"
        } else {
            "measured"
        }
    ));
    fs::create_dir_all(&run_dir)?;

    let workload = config
        .warm_workload
        .ok_or("debug equivalence artifacts currently require warm workload")?;
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&workload.hybrid_query)?;
    let executor =
        HistoricalExecutor::new(Arc::clone(&workload.historical_storage), OxigraphAdapter::new());
    let janus_historical_results = executor.execute_fixed_window(
        parsed.historical_windows.first().ok_or("missing historical window")?,
        &workload.historical_sparql_query,
    )?;
    let oxigraph_historical_results = config.adapter.execute_bindings_query(
        &workload.historical_sparql_query,
        &workload.historical_rdf_events,
    )?;
    let oxigraph_materialized_baseline = materialized_baseline_rows_from_bindings(
        &oxigraph_historical_results,
        "historicalAvgCongestion",
    );

    let mut live_processor = LiveStreamProcessing::new(live_only_rspql())?;
    live_processor.register_stream(LIVE_STREAM_URI)?;
    live_processor.start_processing()?;
    publish_live_events(&live_processor, &workload.live_events)?;
    let live_rows =
        collect_live_results(&live_processor, Duration::from_secs(10), Duration::from_millis(10))?
            .all_rows;
    let (decomposed_join_results, join_trace) =
        join_live_with_baseline_detailed(&live_rows, &oxigraph_materialized_baseline);

    let mut janus_processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    janus_processor.register_stream(LIVE_STREAM_URI)?;
    materialize_bindings_as_static_baseline(&mut janus_processor, &janus_historical_results)?;
    janus_processor.start_processing()?;
    publish_live_events(&janus_processor, &workload.live_events)?;
    let janus_results =
        collect_live_results(&janus_processor, Duration::from_secs(10), Duration::from_millis(10))?
            .all_rows;

    write_trig_events(
        &run_dir.join("janus_input_historical.trig"),
        &workload.historical_rdf_events,
    )?;
    write_trig_events(
        &run_dir.join("oxigraph_input_historical.trig"),
        &workload.historical_rdf_events,
    )?;
    write_jsonl(&run_dir.join("live_events.jsonl"), &event_payload_rows(&workload.live_events))?;
    write_jsonl(&run_dir.join("janus_results.jsonl"), &janus_results)?;
    write_jsonl(&run_dir.join("oxigraph_historical_results.jsonl"), &oxigraph_historical_results)?;
    write_jsonl(
        &run_dir.join("oxigraph_materialized_baseline.jsonl"),
        &oxigraph_materialized_baseline,
    )?;
    write_jsonl(&run_dir.join("decomposed_join_results.jsonl"), &decomposed_join_results)?;
    write_jsonl(
        &run_dir.join("canonical_janus_results.jsonl"),
        &canonical_result_rows(&janus_results),
    )?;
    write_jsonl(
        &run_dir.join("canonical_decomposed_results.jsonl"),
        &canonical_result_rows(&decomposed_join_results),
    )?;
    write_jsonl(&run_dir.join("join_trace.jsonl"), &join_trace)?;
    fs::write(
        run_dir.join("oxigraph_query.rq"),
        format!("{}\n", workload.historical_sparql_query),
    )?;

    let report = EquivalenceReport {
        system_pair: "janus_unified_vs_decomposed_oxigraph".to_string(),
        run_index: config.run_index,
        mode: config.mode.as_str().to_string(),
        historical_input_hash: unified.historical_input_hash.clone(),
        live_input_hash: unified.live_input_hash.clone(),
        janus_result_count: unified.result_count,
        decomposed_result_count: decomposed.result_count,
        janus_result_hash: unified.result_hash.clone(),
        decomposed_result_hash: decomposed.result_hash.clone(),
        equivalent: unified.equivalent_to_baseline.unwrap_or(false),
        historical_inputs_semantically_equal: unified.historical_input_hash
            == decomposed.historical_input_hash,
        live_inputs_semantically_equal: unified.live_input_hash == decomposed.live_input_hash,
        notes: vec![
            "Historical inputs are emitted from the same RDFEvent sequence for Janus and Oxigraph."
                .to_string(),
            "TriG comments preserve source timestamps for debug inspection.".to_string(),
        ],
    };
    fs::write(run_dir.join("equivalence_report.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

pub fn write_sustained_summary_csv(
    path: &Path,
    rows: &[SustainedSummaryRow],
) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "system,mode,time_mode,runs,historical_events,logical_live_duration_seconds,event_rate_hz,event_interval_ms,expected_wall_clock_duration_ms,window_size_ms,window_slide_ms,avg_events_published,avg_events_processed,avg_completed_windows_total,avg_completed_windows_in_horizon,avg_flush_windows,avg_missed_windows,p50_first_hybrid_result_latency_ms,p95_first_hybrid_result_latency_ms,p50_first_hybrid_result_wall_clock_ms,p95_first_hybrid_result_wall_clock_ms,p50_window_hybrid_latency_ms,p95_window_hybrid_latency_ms,p50_window_result_wall_clock_offset_ms,p95_window_result_wall_clock_offset_ms,avg_historical_preparation_latency_ms,avg_first_live_window_latency_ms,avg_readiness_gap_ms,avg_hybrid_wait_after_inputs_ready_ms,avg_external_join_latency_total_ms,avg_external_join_latency_ms,avg_estimated_external_transfer_bytes_total,avg_estimated_external_transfer_bytes_per_window,avg_hybrid_result_count_total,avg_wall_clock_benchmark_duration_ms,avg_wall_clock_overhead_ms,uses_virtual_event_time,equivalence_rate"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{},{},{},{:.3},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{:.3}",
            row.system,
            row.mode,
            row.time_mode,
            row.runs,
            row.historical_events,
            row.logical_live_duration_seconds,
            row.event_rate_hz,
            row.event_interval_ms,
            row.expected_wall_clock_duration_ms,
            row.window_size_ms,
            row.window_slide_ms,
            row.avg_events_published,
            row.avg_events_processed,
            row.avg_completed_windows_total,
            row.avg_completed_windows_in_horizon,
            row.avg_flush_windows,
            row.avg_missed_windows,
            row.p50_first_hybrid_result_latency_ms,
            row.p95_first_hybrid_result_latency_ms,
            row.p50_first_hybrid_result_wall_clock_ms,
            row.p95_first_hybrid_result_wall_clock_ms,
            row.p50_window_hybrid_latency_ms,
            row.p95_window_hybrid_latency_ms,
            row.p50_window_result_wall_clock_offset_ms,
            row.p95_window_result_wall_clock_offset_ms,
            row.avg_historical_preparation_latency_ms,
            row.avg_first_live_window_latency_ms,
            row.avg_readiness_gap_ms,
            row.avg_hybrid_wait_after_inputs_ready_ms,
            row.avg_external_join_latency_total_ms,
            row.avg_external_join_latency_ms,
            row.avg_estimated_external_transfer_bytes_total,
            row.avg_estimated_external_transfer_bytes_per_window,
            row.avg_hybrid_result_count_total,
            row.avg_wall_clock_benchmark_duration_ms,
            row.avg_wall_clock_overhead_ms,
            row.uses_virtual_event_time,
            row.equivalence_rate
        )?;
    }
    Ok(())
}

pub fn write_h1_2_debug_artifacts(
    base_dir: &Path,
    unified: &SustainedSystemOutput,
    decomposed: &SustainedSystemOutput,
    config: &SustainedRunConfig<'_>,
    equivalent: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let run_dir = base_dir.join("h1_2_sustained_debug").join(format!(
        "run_{:03}_{}",
        config.run_index,
        if config.is_warmup {
            "warmup"
        } else {
            "measured"
        }
    ));
    fs::create_dir_all(&run_dir)?;

    write_jsonl(&run_dir.join("live_events.jsonl"), &event_payload_rows(&unified.live_events))?;
    write_jsonl(&run_dir.join("historical_baseline.jsonl"), &unified.historical_baseline)?;

    // Write janus_window_results.jsonl
    let mut janus_rows = Vec::new();
    for (win_id, rows) in &unified.window_results {
        let parts: Vec<&str> = win_id.split('-').collect();
        let win_start_val = parts[0].parse::<u64>().unwrap_or(0);
        let win_end_val = parts[1].parse::<u64>().unwrap_or(0);
        let win_hash = canonical_window_hash(rows)?;
        janus_rows.push(serde_json::json!({
            "window_start_ms": win_start_val,
            "window_end_ms": win_end_val,
            "window_id": win_id.clone(),
            "result_count": rows.len(),
            "result_hash": win_hash,
            "results": rows
        }));
    }
    janus_rows.sort_by_key(|val| val["window_start_ms"].as_u64().unwrap_or(0));
    write_jsonl(&run_dir.join("janus_window_results.jsonl"), &janus_rows)?;

    // Write decomposed_window_results.jsonl
    let mut decomposed_rows = Vec::new();
    for (win_id, rows) in &decomposed.window_results {
        let parts: Vec<&str> = win_id.split('-').collect();
        let win_start_val = parts[0].parse::<u64>().unwrap_or(0);
        let win_end_val = parts[1].parse::<u64>().unwrap_or(0);
        let win_hash = canonical_window_hash(rows)?;
        decomposed_rows.push(serde_json::json!({
            "window_start_ms": win_start_val,
            "window_end_ms": win_end_val,
            "window_id": win_id.clone(),
            "result_count": rows.len(),
            "result_hash": win_hash,
            "results": rows
        }));
    }
    decomposed_rows.sort_by_key(|val| val["window_start_ms"].as_u64().unwrap_or(0));
    write_jsonl(&run_dir.join("decomposed_window_results.jsonl"), &decomposed_rows)?;

    // Write per_window_equivalence.jsonl
    let mut equivalence_rows = Vec::new();
    let mut per_window_summary = Vec::new();
    let mut all_keys: HashSet<String> = unified.window_results.keys().cloned().collect();
    all_keys.extend(decomposed.window_results.keys().cloned());
    let mut sorted_keys: Vec<String> = all_keys.into_iter().collect();
    sorted_keys
        .sort_by_key(|k| k.split('-').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0));

    for win_id in sorted_keys {
        let parts: Vec<&str> = win_id.split('-').collect();
        let win_start_val = parts[0].parse::<u64>().unwrap_or(0);
        let win_end_val = parts[1].parse::<u64>().unwrap_or(0);

        let u_rows = unified.window_results.get(&win_id);
        let d_rows = decomposed.window_results.get(&win_id);
        let u_len = u_rows.map(|r| r.len()).unwrap_or(0);
        let d_len = d_rows.map(|r| r.len()).unwrap_or(0);
        let u_hash =
            u_rows.map(|r| canonical_window_hash(r).unwrap_or_default()).unwrap_or_default();
        let d_hash =
            d_rows.map(|r| canonical_window_hash(r).unwrap_or_default()).unwrap_or_default();
        let equivalent = u_len == d_len && u_hash == d_hash && u_len > 0;

        equivalence_rows.push(serde_json::json!({
            "window_start_ms": win_start_val,
            "window_end_ms": win_end_val,
            "window_id": win_id.clone(),
            "janus_count": u_len,
            "decomposed_count": d_len,
            "janus_hash": u_hash,
            "decomposed_hash": d_hash,
            "equivalent": equivalent
        }));

        per_window_summary.push(serde_json::json!({
            "window_id": win_id,
            "equivalent": equivalent
        }));
    }
    write_jsonl(&run_dir.join("per_window_equivalence.jsonl"), &equivalence_rows)?;

    // Write equivalence_report.json
    let report = serde_json::json!({
        "system_pair": "janus_unified_vs_decomposed_sustained",
        "run_index": config.run_index,
        "mode": config.mode.as_str().to_string(),
        "time_mode": config.time_mode.as_str().to_string(),
        "expected_windows": config.expected_completed_windows_in_horizon(),
        "janus_completed_windows": unified.row.completed_windows_in_horizon,
        "decomposed_completed_windows": decomposed.row.completed_windows_in_horizon,
        "janus_total_hash": unified.row.hybrid_result_hash_total,
        "decomposed_total_hash": decomposed.row.hybrid_result_hash_total,
        "equivalent": equivalent,
        "per_window_equivalence": per_window_summary
    });
    fs::write(run_dir.join("equivalence_report.json"), serde_json::to_vec_pretty(&report)?)?;

    Ok(())
}
