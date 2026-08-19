use std::sync::Arc;
use std::time::{Duration, Instant};

use super::rdf::materialize_query_defined_baseline_quads;
use super::storage::{
    build_live_events_for_replay, realtime_close_timestamp, smoke_historical_window,
};
use super::types::{
    LiveReplayMode, PreparedStorage, QueryDefinedBaselineComparisonRow,
    QueryDefinedBaselineProfile, QueryDefinedBaselineVariantMetrics, ResolvedLiveReplayConfig,
    ResourceSampler, TimedBinding, VariantRunData,
};
use super::validation::{
    build_correctness_diagnostics, expected_day_averages, expected_live_averages, parse_live_rows,
    summarize_observed_windows, validate_baseline_rows, validate_live_only_rows,
};
use super::{PREFIX, RESOURCE_SAMPLE_INTERVAL, STREAM_URI};
use crate::{
    execution::HistoricalExecutor, parsing::janusql_parser::JanusQLParser,
    querying::oxigraph_adapter::OxigraphAdapter,
    stream::live_stream_processing::LiveStreamProcessing,
};

#[allow(clippy::too_many_arguments)]
pub fn run_single_comparison(
    parser: &JanusQLParser,
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    profile: QueryDefinedBaselineProfile,
    historical_events: usize,
    baseline_entities: usize,
    run_index: usize,
    is_warmup: bool,
    debug_lowered_query: bool,
    verbose: bool,
) -> Result<QueryDefinedBaselineComparisonRow, Box<dyn std::error::Error>> {
    let _ = profile;
    let sampler = ResourceSampler::start(RESOURCE_SAMPLE_INTERVAL);
    let comparison_result =
        (|| -> Result<QueryDefinedBaselineComparisonRow, Box<dyn std::error::Error>> {
            let baseline = run_baseline_variant(
                parser,
                prepared,
                live_replay,
                historical_events,
                baseline_entities,
                run_index,
                is_warmup,
                debug_lowered_query,
                verbose,
            )?;
            let live_only = run_live_only_variant(
                prepared,
                live_replay,
                historical_events,
                baseline_entities,
                run_index,
                is_warmup,
                debug_lowered_query,
                verbose,
            )?;

            Ok(QueryDefinedBaselineComparisonRow {
                historical_events,
                baseline_entities,
                is_warmup,
                run_index,
                observed_baseline_rows: baseline.metrics.result_count,
                observed_live_only_rows: live_only.metrics.result_count,
                live_startup_overhead_ms: (baseline.metrics.live_startup_ms
                    - live_only.metrics.live_startup_ms)
                    .max(0.0),
                first_result_overhead_ms: (baseline.metrics.first_result_latency_ms
                    - live_only.metrics.first_result_latency_ms)
                    .max(0.0),
                baseline: baseline.metrics,
                live_only: live_only.metrics,
                peak_rss_mb: None,
                mean_rss_mb: None,
                peak_cpu_percent: None,
                mean_cpu_percent: None,
                sample_count: 0,
            })
        })();
    let resource_usage = sampler.finish();
    let mut comparison = comparison_result?;
    comparison.peak_rss_mb = resource_usage.peak_rss_mb;
    comparison.mean_rss_mb = resource_usage.mean_rss_mb;
    comparison.peak_cpu_percent = resource_usage.peak_cpu_percent;
    comparison.mean_cpu_percent = resource_usage.mean_cpu_percent;
    comparison.sample_count = resource_usage.sample_count;
    Ok(comparison)
}

#[allow(clippy::too_many_arguments)]
pub fn run_baseline_variant(
    parser: &JanusQLParser,
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    historical_events: usize,
    baseline_entities: usize,
    run_index: usize,
    is_warmup: bool,
    debug_lowered_query: bool,
    verbose: bool,
) -> Result<VariantRunData, Box<dyn std::error::Error>> {
    let window_size_ms = live_replay.live_window_size_seconds.unwrap_or(60).saturating_mul(1000);
    let window_slide_ms = live_replay.live_window_slide_seconds.unwrap_or(5).saturating_mul(1000);
    let query = query_defined_baseline_query(live_replay.mode, window_size_ms, window_slide_ms);
    log_stage(verbose, "baseline", "parse_query");
    let parsed = parser.parse(&query)?;
    log_stage(verbose, "baseline", "execute_historical_query");
    let historical_executor =
        HistoricalExecutor::new(Arc::clone(&prepared.storage), OxigraphAdapter::new());
    let historical_window = smoke_historical_window(
        prepared.historical_min_timestamp,
        prepared.historical_max_timestamp,
    )?;

    let historical_started = Instant::now();
    let bindings = historical_executor.execute_fixed_window(
        &historical_window,
        parsed
            .generated_baseline_queries
            .first()
            .ok_or("missing generated baseline query")?
            .sparql_query
            .as_str(),
    )?;
    let historical_query_ms = historical_started.elapsed().as_secs_f64() * 1_000.0;

    log_stage(verbose, "baseline", "materialize_quads");
    let materialize_started = Instant::now();
    let quads = materialize_query_defined_baseline_quads(&parsed, &bindings)?;
    let baseline_materialization_ms = materialize_started.elapsed().as_secs_f64() * 1_000.0;

    if debug_lowered_query {
        log_lowered_live_query("query_defined_baseline", &parsed.rspql_query);
    }
    log_stage(verbose, "baseline", "create_live_processor");
    let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    processor.register_stream(STREAM_URI)?;
    log_stage(verbose, "baseline", "inject_static_quads");
    let injection_started = Instant::now();
    for quad in &quads {
        processor.add_static_quad(quad.clone());
    }
    let static_graph_injection_ms = injection_started.elapsed().as_secs_f64() * 1_000.0;

    log_stage(verbose, "baseline", "start_live_processing");
    let startup_started = Instant::now();
    processor.start_processing()?;
    let live_startup_ms = startup_started.elapsed().as_secs_f64() * 1_000.0;

    let live_events = build_live_events_for_replay(prepared, live_replay, baseline_entities);
    let expected_live_averages = expected_live_averages(&live_events);
    let expected_day_averages = expected_day_averages(&bindings);
    let live_event_start = Instant::now();
    let mut collected = Vec::new();
    match live_replay.mode {
        LiveReplayMode::Accelerated => {
            log_stage(verbose, "baseline", "send_live_events");
            let first_event = live_events.first().ok_or("missing live benchmark events")?.clone();
            processor.add_event(STREAM_URI, first_event)?;
            for event in live_events.iter().skip(1) {
                processor.add_event(STREAM_URI, event.clone())?;
            }
            log_stage(verbose, "baseline", "close_stream");
            processor.close_stream(STREAM_URI, 10_000)?;
            std::thread::sleep(Duration::from_millis(25));
            log_stage(verbose, "baseline", "collect_results");
            collected = collect_live_results(&processor, live_event_start, baseline_entities)?;
        }
        LiveReplayMode::Realtime => {
            log_stage(verbose, "baseline", "send_live_events");
            for (event_index, event) in live_events.iter().enumerate() {
                wait_for_live_event_schedule(live_event_start, event_index, live_replay.rate_hz);
                processor.add_event(STREAM_URI, event.clone())?;
                drain_available_live_results(&processor, live_event_start, &mut collected)?;
            }
            log_stage(verbose, "baseline", "close_stream");
            let close_timestamp = realtime_close_timestamp(&live_events, live_replay)?;
            processor.close_stream(STREAM_URI, close_timestamp)?;
            log_stage(verbose, "baseline", "collect_results");
            collect_realtime_live_results(&processor, live_event_start, &mut collected)?;
        }
    }
    let observed_rows = parse_live_rows(&collected)?;
    log_stage(verbose, "baseline", "done");
    let mut window_semantics_note = if live_replay.mode == LiveReplayMode::Realtime {
        Some(
            "Realtime replay reports emitted windows separately from full windows; the first emission is the initial warm-up window and full windows follow at the logical duration horizon."
                .to_string(),
        )
    } else {
        None
    };
    let window_summaries = summarize_observed_windows(&observed_rows);
    let first_result_latency_ms = observed_rows
        .first()
        .map(|row| row.received_after_first_event_ms)
        .unwrap_or(0.0);
    let window_result_latencies_ms = observed_rows
        .iter()
        .map(|row| row.received_after_first_event_ms)
        .collect::<Vec<_>>();
    let completed_window_latencies_ms = window_summaries
        .iter()
        .map(|window| window.first_result_latency_ms)
        .collect::<Vec<_>>();
    let completed_window_result_counts =
        window_summaries.iter().map(|window| window.result_count).collect::<Vec<_>>();
    let observed_emitted_windows = window_summaries.len();
    let live_event_count = live_events.len();
    if live_replay.mode == LiveReplayMode::Realtime
        && observed_emitted_windows != live_replay.expected_emitted_windows
    {
        let boundary_note = format!(
            "Observed {} emitted windows vs expected {}; the difference is typically caused by inclusive/exclusive window boundary behavior on a window that lands exactly on the replay horizon.",
            observed_emitted_windows, live_replay.expected_emitted_windows
        );
        window_semantics_note = Some(match window_semantics_note {
            Some(existing) => format!("{existing} {boundary_note}"),
            None => boundary_note,
        });
    }
    let correctness_result = validate_baseline_rows(
        &observed_rows,
        &expected_live_averages,
        &expected_day_averages,
        baseline_entities,
        live_replay,
        observed_emitted_windows,
        observed_rows.len(),
        live_event_count,
    );
    let correctness_ok = correctness_result.is_ok();
    let correctness_diagnostics = correctness_result.err().map(|reason| {
        build_correctness_diagnostics(
            "baseline",
            baseline_entities,
            &observed_rows,
            Some(live_replay.expected_emitted_windows),
            observed_emitted_windows,
            vec![
                "sensor".to_string(),
                "minuteAvgValue".to_string(),
                "dayAvgValue".to_string(),
                "difference".to_string(),
            ],
            reason,
        )
    });
    if verbose {
        if let Some(diagnostics) = &correctness_diagnostics {
            eprintln!("[baseline] correctness failed: {}", diagnostics.reason);
            eprintln!(
                "[baseline] expected_result_count={} observed_result_count={} expected_emitted_windows={:?} observed_emitted_windows={} expected_variables={:?} observed_variables={:?}",
                diagnostics.expected_result_count,
                diagnostics.observed_result_count,
                diagnostics.expected_emitted_windows,
                diagnostics.observed_emitted_windows,
                diagnostics.expected_variables,
                diagnostics.observed_variables
            );
            eprintln!("[baseline] first_observed_rows={:?}", diagnostics.first_observed_rows);
        }
    }
    let metrics = QueryDefinedBaselineVariantMetrics {
        variant: "baseline".to_string(),
        run_index,
        historical_events,
        baseline_entities,
        live_replay_mode: live_replay.mode.as_str().to_string(),
        live_rate_hz: live_replay.rate_hz,
        live_duration_seconds: live_replay.live_duration_seconds,
        live_window_size_seconds: live_replay.live_window_size_seconds,
        live_window_slide_seconds: live_replay.live_window_slide_seconds,
        live_event_count,
        expected_emitted_windows: live_replay.expected_emitted_windows,
        expected_full_windows: live_replay.expected_full_windows,
        warmup_window_count: live_replay.warmup_window_count,
        observed_emitted_windows,
        window_semantics_note,
        historical_generation_ms: Some(prepared.historical_generation_ms),
        storage_write_ms: Some(prepared.storage_write_ms),
        baseline_eval_ms: Some(historical_query_ms),
        materialization_ms: Some(baseline_materialization_ms),
        static_injection_ms: Some(static_graph_injection_ms),
        historical_query_ms: Some(historical_query_ms),
        baseline_materialization_ms: Some(baseline_materialization_ms),
        static_graph_injection_ms: Some(static_graph_injection_ms),
        live_startup_ms,
        first_result_latency_ms,
        peak_rss_mb: None,
        mean_rss_mb: None,
        peak_cpu_percent: None,
        mean_cpu_percent: None,
        sample_count: 0,
        result_count: observed_rows.len(),
        correctness_ok,
        correctness_diagnostics,
        materialized_quad_count: Some(quads.len()),
        baseline_binding_count: Some(bindings.len()),
        window_result_latencies_ms,
        completed_window_latencies_ms,
        completed_window_result_counts,
        observed_rows,
    };

    let _ = is_warmup;
    Ok(VariantRunData { metrics })
}

#[allow(clippy::too_many_arguments)]
pub fn run_live_only_variant(
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    historical_events: usize,
    baseline_entities: usize,
    run_index: usize,
    is_warmup: bool,
    debug_lowered_query: bool,
    verbose: bool,
) -> Result<VariantRunData, Box<dyn std::error::Error>> {
    let _ = (run_index, is_warmup);
    let window_size_ms = live_replay.live_window_size_seconds.unwrap_or(60).saturating_mul(1000);
    let window_slide_ms = live_replay.live_window_slide_seconds.unwrap_or(5).saturating_mul(1000);
    let live_query = live_only_query(live_replay.mode, window_size_ms, window_slide_ms);
    log_stage(verbose, "live_only", "parse_query");
    if debug_lowered_query {
        log_lowered_live_query("live_only", &live_query);
    }
    log_stage(verbose, "live_only", "create_live_processor");
    let mut processor = LiveStreamProcessing::new(live_query)?;
    processor.register_stream(STREAM_URI)?;
    log_stage(verbose, "live_only", "start_live_processing");
    let startup_started = Instant::now();
    processor.start_processing()?;
    let live_startup_ms = startup_started.elapsed().as_secs_f64() * 1_000.0;

    let live_events = build_live_events_for_replay(prepared, live_replay, baseline_entities);
    let expected_live_averages = expected_live_averages(&live_events);
    let live_event_start = Instant::now();
    let mut collected = Vec::new();
    match live_replay.mode {
        LiveReplayMode::Accelerated => {
            log_stage(verbose, "live_only", "send_live_events");
            let first_event = live_events.first().ok_or("missing live benchmark events")?.clone();
            processor.add_event(STREAM_URI, first_event)?;
            for event in live_events.iter().skip(1) {
                processor.add_event(STREAM_URI, event.clone())?;
            }
            log_stage(verbose, "live_only", "close_stream");
            processor.close_stream(STREAM_URI, 10_000)?;
            std::thread::sleep(Duration::from_millis(25));
            log_stage(verbose, "live_only", "collect_results");
            collected = collect_live_results(&processor, live_event_start, baseline_entities)?;
        }
        LiveReplayMode::Realtime => {
            log_stage(verbose, "live_only", "send_live_events");
            for (event_index, event) in live_events.iter().enumerate() {
                wait_for_live_event_schedule(live_event_start, event_index, live_replay.rate_hz);
                processor.add_event(STREAM_URI, event.clone())?;
                drain_available_live_results(&processor, live_event_start, &mut collected)?;
            }
            log_stage(verbose, "live_only", "close_stream");
            let close_timestamp = realtime_close_timestamp(&live_events, live_replay)?;
            processor.close_stream(STREAM_URI, close_timestamp)?;
            log_stage(verbose, "live_only", "collect_results");
            collect_realtime_live_results(&processor, live_event_start, &mut collected)?;
        }
    }
    let observed_rows = parse_live_rows(&collected)?;
    log_stage(verbose, "live_only", "done");
    let mut window_semantics_note = if live_replay.mode == LiveReplayMode::Realtime {
        Some(
            "Realtime replay reports emitted windows separately from full windows; the first emission is the initial warm-up window and full windows follow at the logical duration horizon."
                .to_string(),
        )
    } else {
        None
    };
    let window_summaries = summarize_observed_windows(&observed_rows);
    let first_result_latency_ms = observed_rows
        .first()
        .map(|row| row.received_after_first_event_ms)
        .unwrap_or(0.0);
    let window_result_latencies_ms = observed_rows
        .iter()
        .map(|row| row.received_after_first_event_ms)
        .collect::<Vec<_>>();
    let completed_window_latencies_ms = window_summaries
        .iter()
        .map(|window| window.first_result_latency_ms)
        .collect::<Vec<_>>();
    let completed_window_result_counts =
        window_summaries.iter().map(|window| window.result_count).collect::<Vec<_>>();
    let observed_emitted_windows = window_summaries.len();
    let live_event_count = live_events.len();
    if live_replay.mode == LiveReplayMode::Realtime
        && observed_emitted_windows != live_replay.expected_emitted_windows
    {
        let boundary_note = format!(
            "Observed {} emitted windows vs expected {}; the difference is typically caused by inclusive/exclusive window boundary behavior on a window that lands exactly on the replay horizon.",
            observed_emitted_windows, live_replay.expected_emitted_windows
        );
        window_semantics_note = Some(match window_semantics_note {
            Some(existing) => format!("{existing} {boundary_note}"),
            None => boundary_note,
        });
    }
    let correctness_result = validate_live_only_rows(
        &observed_rows,
        &expected_live_averages,
        baseline_entities,
        live_replay,
        observed_emitted_windows,
        observed_rows.len(),
        live_event_count,
    );
    let correctness_ok = correctness_result.is_ok();
    let correctness_diagnostics = correctness_result.err().map(|reason| {
        build_correctness_diagnostics(
            "live_only",
            baseline_entities,
            &observed_rows,
            if live_replay.mode == LiveReplayMode::Realtime {
                Some(live_replay.expected_emitted_windows)
            } else {
                None
            },
            observed_emitted_windows,
            vec!["sensor".to_string(), "minuteAvgValue".to_string()],
            reason,
        )
    });
    if verbose {
        if let Some(diagnostics) = &correctness_diagnostics {
            eprintln!("[live_only] correctness failed: {}", diagnostics.reason);
            eprintln!(
                "[live_only] expected_result_count={} observed_result_count={} expected_emitted_windows={:?} observed_emitted_windows={} expected_variables={:?} observed_variables={:?}",
                diagnostics.expected_result_count,
                diagnostics.observed_result_count,
                diagnostics.expected_emitted_windows,
                diagnostics.observed_emitted_windows,
                diagnostics.expected_variables,
                diagnostics.observed_variables
            );
            eprintln!("[live_only] first_observed_rows={:?}", diagnostics.first_observed_rows);
        }
    }
    let metrics = QueryDefinedBaselineVariantMetrics {
        variant: "live_only".to_string(),
        run_index,
        historical_events,
        baseline_entities,
        live_replay_mode: live_replay.mode.as_str().to_string(),
        live_rate_hz: live_replay.rate_hz,
        live_duration_seconds: live_replay.live_duration_seconds,
        live_window_size_seconds: live_replay.live_window_size_seconds,
        live_window_slide_seconds: live_replay.live_window_slide_seconds,
        live_event_count,
        expected_emitted_windows: live_replay.expected_emitted_windows,
        expected_full_windows: live_replay.expected_full_windows,
        warmup_window_count: live_replay.warmup_window_count,
        observed_emitted_windows,
        window_semantics_note,
        historical_generation_ms: Some(prepared.historical_generation_ms),
        storage_write_ms: Some(prepared.storage_write_ms),
        baseline_eval_ms: None,
        materialization_ms: None,
        static_injection_ms: None,
        historical_query_ms: None,
        baseline_materialization_ms: None,
        static_graph_injection_ms: None,
        live_startup_ms,
        first_result_latency_ms,
        peak_rss_mb: None,
        mean_rss_mb: None,
        peak_cpu_percent: None,
        mean_cpu_percent: None,
        sample_count: 0,
        result_count: observed_rows.len(),
        correctness_ok,
        correctness_diagnostics,
        materialized_quad_count: None,
        baseline_binding_count: None,
        window_result_latencies_ms,
        completed_window_latencies_ms,
        completed_window_result_counts,
        observed_rows,
    };

    Ok(VariantRunData { metrics })
}

pub fn log_stage(verbose: bool, variant: &str, stage: &str) {
    if verbose {
        eprintln!("[{variant}] stage={stage}");
    }
}

pub fn log_lowered_live_query(label: &str, query: &str) {
    eprintln!("[{}] lowered live query:", label);
    for (index, line) in query.lines().enumerate() {
        eprintln!("{:>4} | {}", index + 1, line);
    }
}

pub fn wait_for_live_event_schedule(replay_start: Instant, event_index: usize, rate_hz: f64) {
    let target = replay_start + Duration::from_secs_f64(event_index as f64 / rate_hz);
    if let Some(remaining) = target.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

pub fn drain_available_live_results(
    processor: &LiveStreamProcessing,
    first_event_started: Instant,
    results: &mut Vec<TimedBinding>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut made_progress = false;
    while let Some(result) = processor.try_receive_result()? {
        let received_after_first_event_ms = first_event_started.elapsed().as_secs_f64() * 1_000.0;
        results.push(TimedBinding { result, received_after_first_event_ms });
        made_progress = true;
    }

    Ok(made_progress)
}

pub fn collect_realtime_live_results(
    processor: &LiveStreamProcessing,
    first_event_started: Instant,
    results: &mut Vec<TimedBinding>,
) -> Result<(), Box<dyn std::error::Error>> {
    let idle_timeout = Duration::from_millis(250);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_progress = Instant::now();

    loop {
        if drain_available_live_results(processor, first_event_started, results)? {
            last_progress = Instant::now();
        } else if last_progress.elapsed() >= idle_timeout {
            break;
        }

        if Instant::now() >= deadline {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    if results.is_empty() {
        return Err("timed out waiting for realtime live results".into());
    }

    Ok(())
}

pub fn collect_live_results(
    processor: &LiveStreamProcessing,
    first_event_started: Instant,
    expected_results: usize,
) -> Result<Vec<TimedBinding>, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs_f64(600.0);
    let mut timed_results = Vec::new();
    let mut last_progress = Instant::now();

    loop {
        let mut made_progress = false;
        while let Some(result) = processor.try_receive_result()? {
            let received_after_first_event_ms =
                first_event_started.elapsed().as_secs_f64() * 1_000.0;
            timed_results.push(TimedBinding { result, received_after_first_event_ms });
            if timed_results.len() >= expected_results {
                return Ok(timed_results);
            }
            made_progress = true;
            last_progress = Instant::now();
        }

        if made_progress && last_progress.elapsed() >= Duration::from_millis(50) {
            break;
        }

        if Instant::now() > deadline {
            break;
        }

        std::thread::sleep(Duration::from_millis(5));
    }

    if timed_results.is_empty() {
        return Err("timed out waiting for live results".into());
    }

    Ok(timed_results)
}

pub fn query_defined_baseline_query(
    live_replay: LiveReplayMode,
    window_size_ms: u64,
    window_slide_ms: u64,
) -> String {
    let window_clause = match live_replay {
        LiveReplayMode::Accelerated => "FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]".to_string(),
        LiveReplayMode::Realtime => format!(
            "FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE {window_size_ms} STEP {window_slide_ms}]"
        ),
    };
    format!(
        r#"
PREFIX ex: <{prefix}>

{window_clause}
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {{
  ?sensor ex:hasValue ?value .
}}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
       ((AVG(?value) - ?dayAvgValue) AS ?difference)
WHERE {{
  WINDOW ex:liveMinute {{
    ?sensor ex:hasValue ?value .
  }}
  GRAPH ex:dayBaseline {{
    ?sensor ex:dayAvgValue ?dayAvgValue .
  }}
}}
GROUP BY ?sensor ?dayAvgValue
HAVING(AVG(?value) > ?dayAvgValue)
"#,
        prefix = PREFIX,
        window_clause = window_clause
    )
}

pub fn live_only_query(
    live_replay: LiveReplayMode,
    window_size_ms: u64,
    window_slide_ms: u64,
) -> String {
    let window_clause = match live_replay {
        LiveReplayMode::Accelerated => "FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60 STEP 5]".to_string(),
        LiveReplayMode::Realtime => format!(
            "FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE {window_size_ms} STEP {window_slide_ms}]"
        ),
    };
    format!(
        r#"
PREFIX : <{prefix}>

{window_clause}

REGISTER RStream :output AS
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
WHERE {{
  WINDOW :liveMinute {{
    ?sensor :hasValue ?value .
  }}
}}
GROUP BY ?sensor
"#,
        prefix = PREFIX,
        window_clause = window_clause
    )
}
