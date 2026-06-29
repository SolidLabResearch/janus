use super::types::{
    SustainedRunConfig, TimeMode, BASELINE_NS, CONGESTION_PREDICATE, GRAPH_URI, LIVE_STREAM_URI,
};
use crate::{
    api::janus_api::JanusApiError, core::RDFEvent, storage::util::StreamingConfig,
    stream::live_stream_processing::LiveStreamProcessing,
};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub use crate::execution::result_converter::parse_rsprs_binding_string;

static CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct LiveCollectionResult {
    pub first_result_engine_ms: u64,
    pub all_rows: Vec<HashMap<String, String>>,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

pub fn unique_config(prefix: &str) -> StreamingConfig {
    let counter = CONFIG_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    StreamingConfig {
        segment_base_path: format!("/tmp/janus_{prefix}_{}_{}", now_ms(), counter),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

pub fn normalize_binding_term(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("\\\"") && trimmed.contains("\\\"^^<") {
        let without_prefix = &trimmed[2..];
        if let Some(end) = without_prefix.find("\\\"^^<") {
            return without_prefix[..end].to_string();
        }
    }
    if trimmed.starts_with('"') && trimmed.contains("\"^^<") {
        let without_prefix = &trimmed[1..];
        if let Some(end) = without_prefix.find("\"^^<") {
            return without_prefix[..end].to_string();
        }
    }
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn canonicalize_row(row: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut entries = row
        .iter()
        .map(|(key, value)| (key.clone(), normalize_binding_term(value)))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    entries
}

pub fn canonical_result_hash(
    rows: &[HashMap<String, String>],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut canonical_rows = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
    canonical_rows.sort();
    let payload = serde_json::to_vec(&canonical_rows)?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}"))
}

pub fn canonical_result_hash_sustained(
    windows: &HashMap<String, Vec<HashMap<String, String>>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut keys: Vec<String> = windows.keys().cloned().collect();
    keys.sort_unstable();
    let mut all_canonical = Vec::new();
    for key in keys {
        let rows = &windows[&key];
        let mut canonical_rows = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
        canonical_rows.sort();
        all_canonical.push((key, canonical_rows));
    }
    let payload = serde_json::to_vec(&all_canonical)?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}"))
}

pub fn canonical_window_hash(
    rows: &[HashMap<String, String>],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut canonical_rows = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
    canonical_rows.sort();
    let payload = serde_json::to_vec(&canonical_rows)?;
    let digest = Sha256::digest(payload);
    Ok(format!("{digest:x}"))
}

pub fn historical_input_hash(events: &[RDFEvent]) -> Result<String, Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec(&event_payloads(events))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

pub fn live_input_hash(events: &[RDFEvent]) -> Result<String, Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec(&event_payloads(events))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

pub fn event_payloads(events: &[RDFEvent]) -> Vec<(&str, &str, &str, &str, u64)> {
    events
        .iter()
        .map(|event| {
            (
                event.subject.as_str(),
                event.predicate.as_str(),
                event.object.as_str(),
                event.graph.as_str(),
                event.timestamp,
            )
        })
        .collect()
}

pub fn event_payload_rows(events: &[RDFEvent]) -> Vec<HashMap<String, String>> {
    events
        .iter()
        .map(|event| {
            HashMap::from([
                ("timestamp".to_string(), event.timestamp.to_string()),
                ("subject".to_string(), event.subject.clone()),
                ("predicate".to_string(), event.predicate.clone()),
                ("object".to_string(), event.object.clone()),
                ("graph".to_string(), event.graph.clone()),
            ])
        })
        .collect()
}

pub fn canonical_result_rows(rows: &[HashMap<String, String>]) -> Vec<Vec<(String, String)>> {
    let mut canonical = rows.iter().map(canonicalize_row).collect::<Vec<_>>();
    canonical.sort();
    canonical
}

pub fn extract_between(input: &str, start: &str, end: &str) -> Option<String> {
    let start_index = input.find(start)? + start.len();
    let end_index = input[start_index..].find(end)?;
    Some(input[start_index..start_index + end_index].to_string())
}

pub fn wait_for_sustained_event_schedule(
    config: &SustainedRunConfig<'_>,
    replay_start: Instant,
    event_index: usize,
) {
    if config.time_mode != TimeMode::WallClock {
        return;
    }
    let target =
        replay_start + Duration::from_secs_f64(event_index as f64 / config.event_rate_hz as f64);
    if let Some(remaining) = target.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

pub fn wait_for_sustained_replay_flush(config: &SustainedRunConfig<'_>, replay_deadline: Instant) {
    if config.time_mode != TimeMode::WallClock {
        return;
    }
    if let Some(remaining) = replay_deadline.checked_duration_since(Instant::now()) {
        std::thread::sleep(remaining);
    }
}

pub fn hybrid_query(start_ts: u64, end_ts: u64) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX baseline: <{BASELINE_NS}>

        REGISTER RStream <output> AS
        SELECT ?sensor
               (AVG(?liveCongestion) AS ?liveAvgCongestion)
               ?historicalAvgCongestion
               ((AVG(?liveCongestion) - ?historicalAvgCongestion) AS ?congestionDelta)
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {start_ts} END {end_ts}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE 10000 STEP 1000]
        USING BASELINE ex:hist AGGREGATE
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:congestionLevel ?historicalCongestion .
            }}
            WINDOW ex:live {{
                ?sensor ex:congestionLevel ?liveCongestion .
            }}
            ?sensor baseline:historicalAvgCongestion ?historicalAvgCongestion .
        }}
        GROUP BY ?sensor ?historicalAvgCongestion
        HAVING(AVG(?liveCongestion) > ?historicalAvgCongestion)
        "#
    )
}

pub fn historical_baseline_sparql_query() -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor
               (AVG(?historicalCongestion) AS ?historicalAvgCongestion)
        WHERE {{
            GRAPH ex:citybench {{
                ?sensor ex:congestionLevel ?historicalCongestion .
            }}
        }}
        GROUP BY ?sensor
        "#
    ))
}

pub fn live_only_rspql() -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor
               (AVG(?liveCongestion) AS ?liveAvgCongestion)
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE 10000 STEP 1000]
        WHERE {{
            WINDOW ex:live {{
                ?sensor ex:congestionLevel ?liveCongestion .
            }}
        }}
        GROUP BY ?sensor
        "#
    )
}

pub fn historical_lookup_query(start: u64, end: u64, subject_filter: Option<&str>) -> String {
    let subject_clause = subject_filter
        .map(|subject| format!("<{subject}> <{CONGESTION_PREDICATE}> ?trafficFlow ."))
        .unwrap_or_else(|| format!("?sensor <{CONGESTION_PREDICATE}> ?trafficFlow ."));
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        SELECT ?sensor ?trafficFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {start} END {end}]
        WHERE {{
            WINDOW ex:hist {{
                {subject_clause}
            }}
        }}
        "#
    )
}

pub fn sustained_hybrid_query(
    start_ts: u64,
    end_ts: u64,
    window_size_ms: usize,
    window_slide_ms: usize,
) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX baseline: <{BASELINE_NS}>

        REGISTER RStream <output> AS
        SELECT ?sensor
               (AVG(?liveCongestion) AS ?liveAvgCongestion)
               ?historicalAvgCongestion
               ((AVG(?liveCongestion) - ?historicalAvgCongestion) AS ?congestionDelta)
        FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {start_ts} END {end_ts}]
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        USING BASELINE ex:hist AGGREGATE
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:congestionLevel ?historicalCongestion .
            }}
            WINDOW ex:live {{
                ?sensor ex:congestionLevel ?liveCongestion .
            }}
            ?sensor baseline:historicalAvgCongestion ?historicalAvgCongestion .
        }}
        GROUP BY ?sensor ?historicalAvgCongestion
        HAVING(AVG(?liveCongestion) > ?historicalAvgCongestion)
        "#
    )
}

pub fn live_only_rspql_sustained(window_size_ms: usize, window_slide_ms: usize) -> String {
    format!(
        r#"
        PREFIX ex: <http://example.org/>

        REGISTER RStream <output> AS
        SELECT ?sensor
               (AVG(?liveCongestion) AS ?liveAvgCongestion)
        FROM NAMED WINDOW ex:live ON STREAM <{LIVE_STREAM_URI}> [RANGE {window_size_ms} STEP {window_slide_ms}]
        WHERE {{
            WINDOW ex:live {{
                ?sensor ex:congestionLevel ?liveCongestion .
            }}
        }}
        GROUP BY ?sensor
        "#
    )
}

pub fn parse_window_id(window_id: &str) -> Option<(u64, u64)> {
    let mut parts = window_id.split('-');
    let start = parts.next()?.parse().ok()?;
    let end = parts.next()?.parse().ok()?;
    Some((start, end))
}

pub fn materialize_bindings_as_static_baseline(
    processor: &mut LiveStreamProcessing,
    bindings: &[HashMap<String, String>],
) -> Result<(), JanusApiError> {
    for (subject, predicate, object) in baseline_statements_from_bindings(bindings) {
        processor
            .add_static_data(RDFEvent::new(0, &subject, &predicate, &object, ""))
            .map_err(|err| {
                JanusApiError::LiveProcessingError(format!(
                    "Failed to materialize baseline statement '{} {} {}': {}",
                    subject, predicate, object, err
                ))
            })?;
    }
    Ok(())
}

pub fn baseline_statements_from_bindings(
    bindings: &[HashMap<String, String>],
) -> Vec<(String, String, String)> {
    let mut accumulator = HashMap::<(String, String), super::types::BaselineAccumulator>::new();
    for binding in bindings {
        let Some(subject) = binding
            .get("sensor")
            .or_else(|| binding.get("s"))
            .map(|value| normalize_binding_term(value))
        else {
            continue;
        };

        let mut keys = binding.keys().cloned().collect::<Vec<_>>();
        keys.sort_unstable();
        for key in keys {
            if key == "sensor" || key == "s" {
                continue;
            }
            let Some(value) = binding.get(&key).map(|raw| normalize_binding_term(raw)) else {
                continue;
            };
            let entry = accumulator
                .entry((subject.clone(), key))
                .or_insert_with(super::types::BaselineAccumulator::new);
            entry.last_value.clone_from(&value);
            if let Ok(number) = value.parse::<f64>() {
                entry.numeric_sum += number;
                entry.numeric_count += 1;
            } else {
                entry.all_numeric = false;
            }
        }
    }

    let mut rows = accumulator.into_iter().collect::<Vec<_>>();
    rows.sort_by(|((left_subject, left_var), _), ((right_subject, right_var), _)| {
        left_subject.cmp(right_subject).then_with(|| left_var.cmp(right_var))
    });
    rows.into_iter()
        .map(|((subject, variable), acc)| {
            let object = if acc.all_numeric && acc.numeric_count > 0 {
                (acc.numeric_sum / acc.numeric_count as f64).to_string()
            } else {
                acc.last_value
            };
            (subject, format!("{BASELINE_NS}{variable}"), object)
        })
        .collect()
}

pub fn materialized_baseline_rows_from_bindings(
    bindings: &[HashMap<String, String>],
    baseline_variable: &str,
) -> Vec<HashMap<String, String>> {
    baseline_statements_from_bindings(bindings)
        .into_iter()
        .filter_map(|(subject, predicate, object)| {
            predicate
                .strip_prefix(BASELINE_NS)
                .map(|variable_name| (subject, variable_name.to_string(), object))
        })
        .filter(|(_, variable_name, _)| variable_name == baseline_variable)
        .map(|(subject, variable_name, object)| {
            HashMap::from([("sensor".to_string(), subject), (variable_name, object)])
        })
        .collect()
}

pub fn join_live_with_baseline(
    live_rows: &[HashMap<String, String>],
    baseline_rows: &[HashMap<String, String>],
) -> Vec<HashMap<String, String>> {
    join_live_with_baseline_detailed(live_rows, baseline_rows).0
}

pub fn join_live_with_baseline_detailed(
    live_rows: &[HashMap<String, String>],
    baseline_rows: &[HashMap<String, String>],
) -> (Vec<HashMap<String, String>>, Vec<super::types::JoinTraceRow>) {
    let mut baseline_by_subject = HashMap::<String, HashMap<String, String>>::new();
    for row in baseline_rows {
        let Some(subject) = row
            .get("sensor")
            .or_else(|| row.get("s"))
            .map(|value| normalize_binding_term(value))
        else {
            continue;
        };
        baseline_by_subject.insert(subject, row.clone());
    }

    let mut joined = Vec::new();
    let mut trace = Vec::new();
    for live_row in live_rows {
        let Some(subject) = live_row
            .get("sensor")
            .or_else(|| live_row.get("s"))
            .map(|value| normalize_binding_term(value))
        else {
            trace.push(super::types::JoinTraceRow {
                historical_join_key: None,
                live_join_key: None,
                accepted: false,
                rejection_reason: Some("missing_live_join_key".to_string()),
                historical_row: None,
                live_row: canonicalize_row(live_row),
                joined_row: None,
            });
            continue;
        };
        if let Some(baseline_row) = baseline_by_subject.get(&subject) {
            let mut merged = baseline_row.clone();
            for (key, value) in live_row {
                merged.insert(key.clone(), value.clone());
            }
            let live_avg = merged
                .get("liveAvgCongestion")
                .and_then(|value| normalize_binding_term(value).parse::<f64>().ok());
            let historical_avg = merged
                .get("historicalAvgCongestion")
                .and_then(|value| normalize_binding_term(value).parse::<f64>().ok());
            if let (Some(live_avg), Some(historical_avg)) = (live_avg, historical_avg) {
                merged.insert(
                    "congestionDelta".to_string(),
                    format!("{}", live_avg - historical_avg),
                );
            }
            let accepted = matches!(
                (live_avg, historical_avg),
                (Some(live_avg), Some(historical_avg)) if live_avg > historical_avg
            );
            trace.push(super::types::JoinTraceRow {
                historical_join_key: Some(subject.clone()),
                live_join_key: Some(subject),
                accepted,
                rejection_reason: if accepted {
                    None
                } else {
                    Some("live_average_not_above_historical_average".to_string())
                },
                historical_row: Some(canonicalize_row(baseline_row)),
                live_row: canonicalize_row(live_row),
                joined_row: accepted.then(|| canonicalize_row(&merged)),
            });
            if accepted {
                joined.push(merged);
            }
        } else {
            trace.push(super::types::JoinTraceRow {
                historical_join_key: None,
                live_join_key: Some(subject),
                accepted: false,
                rejection_reason: Some("no_historical_row_for_join_key".to_string()),
                historical_row: None,
                live_row: canonicalize_row(live_row),
                joined_row: None,
            });
        }
    }
    (joined, trace)
}

pub fn publish_live_events(
    processor: &LiveStreamProcessing,
    live_events: &[RDFEvent],
) -> Result<(), Box<dyn std::error::Error>> {
    if live_events.is_empty() {
        processor.close_stream(LIVE_STREAM_URI, 20_000_i64)?;
        return Ok(());
    }
    let first = live_events.first().unwrap().clone();
    processor.add_event(LIVE_STREAM_URI, first)?;
    for event in live_events.iter().skip(1) {
        processor.add_event(LIVE_STREAM_URI, event.clone())?;
    }
    let close_ts = live_events
        .last()
        .map_or(20_000_i64, |event| i64::try_from(event.timestamp).unwrap_or(i64::MAX) + 20_000);
    processor.close_stream(LIVE_STREAM_URI, close_ts)?;
    Ok(())
}

pub fn collect_live_results(
    processor: &LiveStreamProcessing,
    first_result_timeout: Duration,
    idle_timeout: Duration,
) -> Result<LiveCollectionResult, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + first_result_timeout;
    loop {
        if let Some(result) = processor.try_receive_result()? {
            let first_result_engine_ms = now_ms();
            let mut rows = vec![parse_rsprs_binding_string(&result.bindings)];
            let mut idle_deadline = Instant::now() + idle_timeout;
            loop {
                if let Some(next_result) = processor.try_receive_result()? {
                    rows.push(parse_rsprs_binding_string(&next_result.bindings));
                    idle_deadline = Instant::now() + idle_timeout;
                    continue;
                }
                if Instant::now() >= idle_deadline {
                    return Ok(LiveCollectionResult { first_result_engine_ms, all_rows: rows });
                }
                std::thread::yield_now();
            }
        }
        if Instant::now() >= deadline {
            return Ok(LiveCollectionResult {
                first_result_engine_ms: now_ms(),
                all_rows: Vec::new(),
            });
        }
        std::thread::yield_now();
    }
}
