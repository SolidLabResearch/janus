use super::data_gen::prepare_sustained_workload;
use super::helpers::{
    baseline_statements_from_bindings, canonical_result_hash_sustained, canonical_window_hash,
    join_live_with_baseline, live_only_rspql_sustained, materialize_bindings_as_static_baseline,
    materialized_baseline_rows_from_bindings, now_ms, parse_rsprs_binding_string, parse_window_id,
    wait_for_sustained_event_schedule, wait_for_sustained_replay_flush,
};
use super::io::write_h1_2_debug_artifacts;
use super::metrics::percentile;
use super::types::{
    CoordinationSystem, ExecutionMode, SustainedPair, SustainedRow, SustainedRunConfig,
    SustainedSystemOutput, TimeMode, LIVE_STREAM_URI,
};
use crate::{
    execution::HistoricalExecutor, parsing::janusql_parser::JanusQLParser,
    querying::oxigraph_adapter::OxigraphAdapter,
    stream::live_stream_processing::LiveStreamProcessing,
};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

pub fn run_sustained_pair(
    config: SustainedRunConfig<'_>,
) -> Result<SustainedPair, Box<dyn std::error::Error>> {
    let unified = run_sustained_system(CoordinationSystem::JanusUnified, &config)?;
    let decomposed = run_sustained_system(CoordinationSystem::DecomposedOxigraph, &config)?;

    let mut unified_row = unified.row.clone();
    let mut decomposed_row = decomposed.row.clone();

    let expected_completed_windows = config.expected_completed_windows_in_horizon();
    let mut equivalent = unified_row.completed_windows_in_horizon
        == decomposed_row.completed_windows_in_horizon
        && unified_row.completed_windows_in_horizon == expected_completed_windows
        && unified_row.hybrid_result_hash_total == decomposed_row.hybrid_result_hash_total
        && !unified_row.hybrid_result_hash_total.is_empty()
        && unified_row.hybrid_result_hash_total
            != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    if equivalent {
        let horizon_end_ms = i64::try_from(
            unified_row
                .first_live_event_at
                .saturating_add((config.live_duration_seconds * 1000) as u64),
        )
        .map_err(|_| "horizon end timestamp exceeds i64 range")?;
        for (win_id, u_rows) in &unified.window_results {
            let parts: Vec<&str> = win_id.split('-').collect();
            if parts.len() == 2 {
                if let Ok(win_end) = parts[1].parse::<i64>() {
                    if win_end > horizon_end_ms {
                        continue;
                    }
                }
            }
            if let Some(d_rows) = decomposed.window_results.get(win_id) {
                let u_hash = canonical_window_hash(u_rows)?;
                let d_hash = canonical_window_hash(d_rows)?;
                if u_hash != d_hash || u_rows.len() != d_rows.len() {
                    equivalent = false;
                    break;
                }
            } else {
                equivalent = false;
                break;
            }
        }
    }

    unified_row.equivalent_to_baseline = Some(equivalent);
    decomposed_row.equivalent_to_baseline = None;

    if let Some(debug_output_dir) = config.debug_output_dir {
        write_h1_2_debug_artifacts(debug_output_dir, &unified, &decomposed, &config, equivalent)?;
    }

    Ok(SustainedPair { unified: unified_row, decomposed: decomposed_row })
}

pub fn run_sustained_system(
    system: CoordinationSystem,
    config: &SustainedRunConfig<'_>,
) -> Result<SustainedSystemOutput, Box<dyn std::error::Error>> {
    let owned_workload;
    let workload = if config.mode == ExecutionMode::Warm {
        config.warm_workload.ok_or("warm mode requires prepared workload")?
    } else {
        owned_workload = prepare_sustained_workload(
            config.historical_events,
            config.live_duration_seconds,
            config.event_rate_hz,
            config.window_size_seconds,
            config.window_slide_seconds,
        )?;
        &owned_workload
    };

    let start_wall = Instant::now();
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

    let events_published = workload.live_events.len();
    let events_processed = workload.live_events.len();
    let window_size_ms = config.window_size_seconds * 1000;
    let window_slide_ms = config.window_slide_seconds * 1000;
    let expected_completed_windows = config.expected_completed_windows_in_horizon();
    let live_first_event_at = workload.live_events.first().map(|e| e.timestamp).unwrap_or(0);
    let horizon_end_ms = i64::try_from(
        live_first_event_at.saturating_add((config.live_duration_seconds * 1000) as u64),
    )
    .map_err(|_| "horizon end timestamp exceeds i64 range")?;

    let mut window_results = HashMap::<String, Vec<HashMap<String, String>>>::new();
    let mut window_hybrid_latencies = Vec::<(i64, f64)>::new();
    let mut window_wall_clock_offsets = HashMap::<String, f64>::new();
    let mut first_live_window_ready_at = 0u64;
    let mut first_hybrid_result_at = 0u64;
    let mut first_hybrid_result_wall_clock_ms = 0.0;

    let live_start_at;
    let mut external_join_latencies = Vec::<(i64, f64)>::new();
    let historical_baseline;

    let estimated_external_transfer_bytes_total;

    match system {
        CoordinationSystem::JanusUnified => {
            let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
            processor.register_stream(LIVE_STREAM_URI)?;
            materialize_bindings_as_static_baseline(&mut processor, &baseline_bindings)?;
            processor.start_processing()?;
            live_start_at = now_ms();
            let replay_start = Instant::now();
            let replay_deadline =
                replay_start + Duration::from_millis(config.expected_wall_clock_duration_ms);

            historical_baseline = baseline_statements_from_bindings(&baseline_bindings)
                .into_iter()
                .map(|(s, _, o)| {
                    HashMap::from([("sensor".to_string(), s), ("baselineFlow".to_string(), o)])
                })
                .collect();
            estimated_external_transfer_bytes_total = 0;

            for (event_index, event) in workload.live_events.iter().enumerate() {
                wait_for_sustained_event_schedule(config, replay_start, event_index);
                let start_add = Instant::now();
                processor.add_event(LIVE_STREAM_URI, event.clone())?;
                let add_duration = start_add.elapsed().as_secs_f64() * 1000.0;

                std::thread::sleep(std::time::Duration::from_millis(2));

                let mut received = Vec::new();
                while let Some(res) = processor.try_receive_result()? {
                    received.push(res);
                }

                if !received.is_empty() {
                    let now = now_ms();
                    let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                    if first_live_window_ready_at == 0 {
                        first_live_window_ready_at = now;
                        first_hybrid_result_at = now;
                        first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                    }
                    for res in received {
                        let win_start = res.timestamp_from;
                        let win_end = res.timestamp_to;
                        let win_id = format!("{win_start}-{win_end}");
                        let rows = parse_rsprs_binding_string(&res.bindings);
                        window_results.entry(win_id.clone()).or_default().push(rows);
                        window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);
                        window_hybrid_latencies.push((win_end, add_duration));
                    }
                }
            }

            wait_for_sustained_replay_flush(config, replay_deadline);
            let close_start = Instant::now();
            let close_ts = workload.live_events.last().map_or(20_000_i64, |e| {
                i64::try_from(e.timestamp).unwrap_or(i64::MAX).saturating_add(20_000)
            });
            processor.close_stream(LIVE_STREAM_URI, close_ts)?;
            let close_duration = close_start.elapsed().as_secs_f64() * 1000.0;

            std::thread::sleep(std::time::Duration::from_millis(10));

            let mut final_received = Vec::new();
            while let Some(res) = processor.try_receive_result()? {
                final_received.push(res);
            }
            if !final_received.is_empty() {
                let now = now_ms();
                let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                if first_live_window_ready_at == 0 {
                    first_live_window_ready_at = now;
                    first_hybrid_result_at = now;
                    first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                }
                for res in final_received {
                    let win_start = res.timestamp_from;
                    let win_end = res.timestamp_to;
                    let win_id = format!("{win_start}-{win_end}");
                    let rows = parse_rsprs_binding_string(&res.bindings);
                    window_results.entry(win_id.clone()).or_default().push(rows);
                    window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);
                    window_hybrid_latencies.push((win_end, close_duration));
                }
            }
        }
        CoordinationSystem::DecomposedOxigraph => {
            let external_bindings = config.adapter.execute_bindings_query(
                &workload.historical_sparql_query,
                &workload.historical_rdf_events,
            )?;
            let materialized_baseline_rows =
                materialized_baseline_rows_from_bindings(&external_bindings, "baselineFlow");
            let historical_done = now_ms();

            historical_baseline = materialized_baseline_rows.clone();
            let historical_intermediate_bytes = serde_json::to_vec(&external_bindings)?.len();

            let mut processor = LiveStreamProcessing::new(live_only_rspql_sustained(
                window_size_ms,
                window_slide_ms,
            ))?;
            processor.register_stream(LIVE_STREAM_URI)?;
            processor.start_processing()?;
            live_start_at = now_ms();
            let replay_start = Instant::now();
            let replay_deadline =
                replay_start + Duration::from_millis(config.expected_wall_clock_duration_ms);

            let mut all_live_rows = Vec::new();

            for (event_index, event) in workload.live_events.iter().enumerate() {
                wait_for_sustained_event_schedule(config, replay_start, event_index);
                let start_add = Instant::now();
                processor.add_event(LIVE_STREAM_URI, event.clone())?;
                let add_duration = start_add.elapsed().as_secs_f64() * 1000.0;

                std::thread::sleep(std::time::Duration::from_millis(2));

                let mut received = Vec::new();
                while let Some(res) = processor.try_receive_result()? {
                    received.push(res);
                }

                if !received.is_empty() {
                    let now = now_ms();
                    let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                    if first_live_window_ready_at == 0 {
                        first_live_window_ready_at = now;
                    }
                    for res in received {
                        let win_start = res.timestamp_from;
                        let win_end = res.timestamp_to;
                        let win_id = format!("{win_start}-{win_end}");
                        let rows = parse_rsprs_binding_string(&res.bindings);
                        if win_end <= horizon_end_ms {
                            all_live_rows.push(rows.clone());
                        }
                        let start_join = Instant::now();
                        let joined = join_live_with_baseline(&[rows], &materialized_baseline_rows);
                        let join_duration = start_join.elapsed().as_secs_f64() * 1000.0;
                        if first_hybrid_result_at == 0 {
                            first_hybrid_result_at = now_ms();
                            first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                        }

                        for row in joined {
                            window_results.entry(win_id.clone()).or_default().push(row);
                        }
                        window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);

                        window_hybrid_latencies.push((win_end, add_duration + join_duration));
                        external_join_latencies.push((win_end, join_duration));
                    }
                }
            }

            wait_for_sustained_replay_flush(config, replay_deadline);
            let close_start = Instant::now();
            let close_ts = workload.live_events.last().map_or(20_000_i64, |e| {
                i64::try_from(e.timestamp).unwrap_or(i64::MAX).saturating_add(20_000)
            });
            processor.close_stream(LIVE_STREAM_URI, close_ts)?;
            let close_duration = close_start.elapsed().as_secs_f64() * 1000.0;

            std::thread::sleep(std::time::Duration::from_millis(10));

            let mut final_received = Vec::new();
            while let Some(res) = processor.try_receive_result()? {
                final_received.push(res);
            }
            if !final_received.is_empty() {
                let now = now_ms();
                let wall_clock_offset_ms = replay_start.elapsed().as_secs_f64() * 1000.0;
                if first_live_window_ready_at == 0 {
                    first_live_window_ready_at = now;
                }
                for res in final_received {
                    let win_start = res.timestamp_from;
                    let win_end = res.timestamp_to;
                    let win_id = format!("{win_start}-{win_end}");
                    let rows = parse_rsprs_binding_string(&res.bindings);
                    if win_end <= horizon_end_ms {
                        all_live_rows.push(rows.clone());
                    }
                    let start_join = Instant::now();
                    let joined = join_live_with_baseline(&[rows], &materialized_baseline_rows);
                    let join_duration = start_join.elapsed().as_secs_f64() * 1000.0;
                    if first_hybrid_result_at == 0 {
                        first_hybrid_result_at = now_ms();
                        first_hybrid_result_wall_clock_ms = wall_clock_offset_ms;
                    }

                    for row in joined {
                        window_results.entry(win_id.clone()).or_default().push(row);
                    }
                    window_wall_clock_offsets.entry(win_id).or_insert(wall_clock_offset_ms);

                    window_hybrid_latencies.push((win_end, close_duration + join_duration));
                    external_join_latencies.push((win_end, join_duration));
                }
            }

            let live_intermediate_bytes = serde_json::to_vec(&all_live_rows)?.len();
            estimated_external_transfer_bytes_total =
                historical_intermediate_bytes + live_intermediate_bytes;
        }
    }

    let wall_clock_benchmark_duration_ms = start_wall.elapsed().as_millis() as u64;

    let completed_windows_total = window_results.len();
    let mut completed_windows_in_horizon = 0;
    let mut horizon_window_results = HashMap::new();
    for (win_id, rows) in &window_results {
        let parts: Vec<&str> = win_id.split('-').collect();
        if parts.len() == 2 {
            if let Ok(win_end) = parts[1].parse::<i64>() {
                if win_end <= horizon_end_ms {
                    completed_windows_in_horizon += 1;
                    horizon_window_results.insert(win_id.clone(), rows.clone());
                }
            }
        }
    }
    let flush_windows = completed_windows_total - completed_windows_in_horizon;
    let missed_windows = expected_completed_windows.saturating_sub(completed_windows_in_horizon);

    let historical_preparation_latency_ms = (historical_done - historical_start) as f64;
    let first_live_window_latency_ms = if first_live_window_ready_at > 0 {
        (first_live_window_ready_at - live_start_at) as f64
    } else {
        0.0
    };
    let first_hybrid_result_latency_ms = if first_hybrid_result_at > 0 {
        (first_hybrid_result_at - client_start) as f64
    } else {
        0.0
    };

    let readiness_gap_ms = if historical_done > 0 && first_live_window_ready_at > 0 {
        (historical_done as f64 - first_live_window_ready_at as f64).abs()
    } else {
        0.0
    };

    let hybrid_wait_after_inputs_ready_ms = if first_hybrid_result_at > 0 {
        let max_ready = historical_done.max(first_live_window_ready_at) as f64;
        (first_hybrid_result_at as f64 - max_ready).max(0.0)
    } else {
        0.0
    };

    let horizon_latencies: Vec<f64> = window_hybrid_latencies
        .iter()
        .filter(|&&(win_end, _)| win_end <= horizon_end_ms)
        .map(|&(_, lat)| lat)
        .collect();
    let p50_window_hybrid_latency_ms = percentile(&horizon_latencies, 50.0);
    let p95_window_hybrid_latency_ms = percentile(&horizon_latencies, 95.0);

    let horizon_join_latencies: Vec<f64> = external_join_latencies
        .iter()
        .filter(|&&(win_end, _)| win_end <= horizon_end_ms)
        .map(|&(_, lat)| lat)
        .collect();
    let external_join_latency_total_ms: f64 = horizon_join_latencies.iter().sum();
    let external_join_latency_avg_ms = if completed_windows_in_horizon > 0 {
        external_join_latency_total_ms / completed_windows_in_horizon as f64
    } else {
        0.0
    };

    let estimated_external_transfer_bytes_per_window = if completed_windows_in_horizon > 0 {
        estimated_external_transfer_bytes_total
            .checked_div(completed_windows_in_horizon)
            .unwrap_or(0)
    } else {
        0
    };
    let mut window_result_wall_clock_offsets_ms = window_wall_clock_offsets
        .iter()
        .filter_map(|(win_id, offset)| {
            let (_, win_end) = parse_window_id(win_id)?;
            (i64::try_from(win_end).ok()? <= horizon_end_ms).then_some(*offset)
        })
        .collect::<Vec<_>>();
    window_result_wall_clock_offsets_ms
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let p50_window_result_wall_clock_offset_ms =
        percentile(&window_result_wall_clock_offsets_ms, 50.0);
    let p95_window_result_wall_clock_offset_ms =
        percentile(&window_result_wall_clock_offsets_ms, 95.0);

    let mut hybrid_result_count_total = 0;
    for rows in horizon_window_results.values() {
        hybrid_result_count_total += rows.len();
    }
    let hybrid_result_hash_total = canonical_result_hash_sustained(&horizon_window_results)?;
    let wall_clock_overhead_ms =
        wall_clock_benchmark_duration_ms as f64 - config.expected_wall_clock_duration_ms as f64;

    let row = SustainedRow {
        system: system.as_str().to_string(),
        mode: config.mode.as_str().to_string(),
        time_mode: config.time_mode.as_str().to_string(),
        is_warmup: config.is_warmup,
        run_index: config.run_index,
        historical_events: config.historical_events,
        logical_live_duration_seconds: config.live_duration_seconds,
        event_rate_hz: config.event_rate_hz,
        event_interval_ms: config.event_interval_ms,
        expected_wall_clock_duration_ms: config.expected_wall_clock_duration_ms,
        events_published,
        events_processed,
        window_size_ms,
        window_slide_ms,
        expected_completed_windows,
        completed_windows_total,
        completed_windows_in_horizon,
        flush_windows,
        missed_windows,
        historical_start_at: historical_start,
        historical_ready_at: historical_done,
        live_start_at,
        first_live_event_at: live_first_event_at,
        first_live_window_ready_at,
        first_hybrid_result_at,
        historical_preparation_latency_ms,
        first_live_window_latency_ms,
        first_hybrid_result_latency_ms,
        first_hybrid_result_wall_clock_ms,
        readiness_gap_ms,
        hybrid_wait_after_inputs_ready_ms,
        p50_window_hybrid_latency_ms,
        p95_window_hybrid_latency_ms,
        window_result_wall_clock_offsets_ms,
        p50_window_result_wall_clock_offset_ms,
        p95_window_result_wall_clock_offset_ms,
        external_join_latency_total_ms,
        external_join_latency_avg_ms,
        estimated_external_transfer_bytes_total,
        estimated_external_transfer_bytes_per_window,
        hybrid_result_count_total,
        hybrid_result_hash_total,
        equivalent_to_baseline: None,
        metadata: config.metadata.clone(),
        wall_clock_benchmark_duration_ms,
        wall_clock_overhead_ms,
        uses_virtual_event_time: config.time_mode.uses_virtual_event_time(),
    };

    Ok(SustainedSystemOutput {
        row,
        window_results,
        live_events: workload.live_events.clone(),
        historical_baseline,
    })
}

pub fn sustained_event_interval_ms(event_rate_hz: usize) -> f64 {
    1000.0 / event_rate_hz as f64
}

pub fn sustained_expected_wall_clock_duration_ms(live_duration_seconds: usize) -> u64 {
    (live_duration_seconds as u64) * 1000
}
