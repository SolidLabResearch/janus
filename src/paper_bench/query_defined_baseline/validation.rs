use std::collections::{BTreeMap, HashMap};

use super::rdf::{normalize_binding_term, parse_numeric};
use super::types::{
    LiveReplayMode, ObservedWindowSummary, QueryDefinedBaselineCorrectnessDiagnostics,
    QueryDefinedBaselineObservedRow, ResolvedLiveReplayConfig, TimedBinding,
};
use crate::{core::RDFEvent, execution::ResultConverter};

pub fn expected_live_averages(live_events: &[RDFEvent]) -> HashMap<String, f64> {
    let mut by_sensor: HashMap<String, (f64, usize)> = HashMap::new();
    for event in live_events {
        let sensor = normalize_binding_term(&event.subject.clone());
        let value = event.object.clone().trim().parse::<f64>().unwrap_or(0.0);
        let entry = by_sensor.entry(sensor).or_insert((0.0, 0));
        entry.0 += value;
        entry.1 += 1;
    }

    by_sensor
        .into_iter()
        .map(|(sensor, (sum, count))| (sensor, if count == 0 { 0.0 } else { sum / count as f64 }))
        .collect()
}

pub fn expected_day_averages(bindings: &[HashMap<String, String>]) -> HashMap<String, f64> {
    let mut expected = HashMap::new();
    for binding in bindings {
        if let (Some(sensor), Some(day_avg)) = (binding.get("sensor"), binding.get("dayAvgValue")) {
            if let Ok(value) = parse_numeric(day_avg) {
                expected.insert(normalize_binding_term(sensor), value);
            }
        }
    }

    expected
}

pub fn observed_query_variables(rows: &[QueryDefinedBaselineObservedRow]) -> Vec<String> {
    let mut vars = vec!["sensor".to_string(), "minuteAvgValue".to_string()];
    if rows.iter().any(|row| row.day_avg_value.is_some()) {
        vars.push("dayAvgValue".to_string());
    }
    if rows.iter().any(|row| row.difference.is_some()) {
        vars.push("difference".to_string());
    }
    vars
}

pub fn build_correctness_diagnostics(
    variant: &str,
    expected_result_count: usize,
    observed_rows: &[QueryDefinedBaselineObservedRow],
    expected_emitted_windows: Option<usize>,
    observed_emitted_windows: usize,
    expected_variables: Vec<String>,
    reason: String,
) -> QueryDefinedBaselineCorrectnessDiagnostics {
    QueryDefinedBaselineCorrectnessDiagnostics {
        variant: variant.to_string(),
        expected_result_count,
        observed_result_count: observed_rows.len(),
        expected_emitted_windows,
        observed_emitted_windows,
        expected_variables,
        observed_variables: observed_query_variables(observed_rows),
        first_observed_rows: observed_rows.iter().take(3).cloned().collect(),
        reason,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn validate_baseline_rows(
    rows: &[QueryDefinedBaselineObservedRow],
    expected_live_averages: &HashMap<String, f64>,
    expected_day_averages: &HashMap<String, f64>,
    expected_entities: usize,
    live_replay: &ResolvedLiveReplayConfig,
    observed_emitted_windows: usize,
    observed_row_count: usize,
    live_event_count: usize,
) -> Result<(), String> {
    if expected_day_averages.len() != expected_entities {
        return Err(format!(
            "expected {} baseline bindings but found {}",
            expected_entities,
            expected_day_averages.len()
        ));
    }
    if observed_row_count != expected_entities {
        return Err(format!(
            "expected {} baseline rows but observed {}",
            expected_entities, observed_row_count
        ));
    }
    if live_replay.mode == LiveReplayMode::Realtime {
        if live_event_count != live_replay.live_event_count {
            return Err(format!(
                "expected {} live events but observed {}",
                live_replay.live_event_count, live_event_count
            ));
        }
        if observed_emitted_windows != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} emitted windows but observed {}",
                live_replay.expected_emitted_windows, observed_emitted_windows
            ));
        }
        if summarize_observed_windows(rows).len() != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} summarized windows but observed {}",
                live_replay.expected_emitted_windows,
                summarize_observed_windows(rows).len()
            ));
        }
    }

    for row in rows {
        let expected_live = expected_live_averages
            .get(&row.sensor)
            .ok_or_else(|| format!("missing expected live aggregate for sensor {}", row.sensor))?;
        if (row.minute_avg_value - expected_live).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} minuteAvgValue {:.6} did not match expected {:.6}",
                row.sensor, row.minute_avg_value, expected_live
            ));
        }
        let expected_day = expected_day_averages
            .get(&row.sensor)
            .ok_or_else(|| format!("missing expected day aggregate for sensor {}", row.sensor))?;
        let day_avg = row
            .day_avg_value
            .ok_or_else(|| format!("sensor {} is missing dayAvgValue", row.sensor))?;
        if (day_avg - expected_day).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} dayAvgValue {:.6} did not match expected {:.6}",
                row.sensor, day_avg, expected_day
            ));
        }
        let difference = row
            .difference
            .ok_or_else(|| format!("sensor {} is missing difference", row.sensor))?;
        if (row.minute_avg_value - day_avg - difference).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} difference {:.6} was not minuteAvgValue - dayAvgValue",
                row.sensor, difference
            ));
        }
    }

    Ok(())
}

pub fn validate_live_only_rows(
    rows: &[QueryDefinedBaselineObservedRow],
    expected_live_averages: &HashMap<String, f64>,
    expected_entities: usize,
    live_replay: &ResolvedLiveReplayConfig,
    observed_emitted_windows: usize,
    observed_row_count: usize,
    live_event_count: usize,
) -> Result<(), String> {
    if observed_row_count != expected_entities {
        return Err(format!(
            "expected {} live-only rows but observed {}",
            expected_entities, observed_row_count
        ));
    }
    if live_replay.mode == LiveReplayMode::Realtime {
        if live_event_count != live_replay.live_event_count {
            return Err(format!(
                "expected {} live events but observed {}",
                live_replay.live_event_count, live_event_count
            ));
        }
        if observed_emitted_windows != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} emitted windows but observed {}",
                live_replay.expected_emitted_windows, observed_emitted_windows
            ));
        }
        if summarize_observed_windows(rows).len() != live_replay.expected_emitted_windows {
            return Err(format!(
                "expected {} summarized windows but observed {}",
                live_replay.expected_emitted_windows,
                summarize_observed_windows(rows).len()
            ));
        }
    }

    for row in rows {
        let expected_live = expected_live_averages
            .get(&row.sensor)
            .ok_or_else(|| format!("missing expected live aggregate for sensor {}", row.sensor))?;
        if (row.minute_avg_value - expected_live).abs() > 0.000_001 {
            return Err(format!(
                "sensor {} minuteAvgValue {:.6} did not match expected {:.6}",
                row.sensor, row.minute_avg_value, expected_live
            ));
        }
        if row.day_avg_value.is_some() {
            return Err(format!("sensor {} unexpectedly produced dayAvgValue", row.sensor));
        }
        if row.difference.is_some() {
            return Err(format!("sensor {} unexpectedly produced difference", row.sensor));
        }
    }

    Ok(())
}

pub fn parse_live_rows(
    results: &[TimedBinding],
) -> Result<Vec<QueryDefinedBaselineObservedRow>, Box<dyn std::error::Error>> {
    let converter = ResultConverter::new("query_defined_baseline".to_string());
    let mut rows = Vec::new();

    for result in results {
        let converted = converter.from_live_binding(result.result.clone());
        let binding = converted.bindings.first().ok_or("live result did not contain bindings")?;

        let sensor = binding.get("sensor").cloned().ok_or("live result missing sensor binding")?;
        let minute_avg_value = parse_numeric(
            binding
                .get("minuteAvgValue")
                .ok_or("live result missing minuteAvgValue binding")?,
        )?;
        let day_avg_value =
            binding.get("dayAvgValue").map(|value| parse_numeric(value)).transpose()?;
        let difference = binding.get("difference").map(|value| parse_numeric(value)).transpose()?;

        let observed = QueryDefinedBaselineObservedRow {
            sensor,
            minute_avg_value,
            day_avg_value,
            difference,
            received_after_first_event_ms: result.received_after_first_event_ms,
            timestamp_from: result.result.timestamp_from,
            timestamp_to: result.result.timestamp_to,
        };
        rows.push(observed);
    }

    rows.sort_by(|left, right| {
        left.timestamp_from
            .cmp(&right.timestamp_from)
            .then_with(|| left.timestamp_to.cmp(&right.timestamp_to))
            .then_with(|| left.sensor.cmp(&right.sensor))
    });

    Ok(rows)
}

pub fn summarize_observed_windows(
    rows: &[QueryDefinedBaselineObservedRow],
) -> Vec<ObservedWindowSummary> {
    let mut by_window: BTreeMap<(i64, i64), Vec<&QueryDefinedBaselineObservedRow>> =
        BTreeMap::new();

    for row in rows {
        by_window.entry((row.timestamp_from, row.timestamp_to)).or_default().push(row);
    }

    by_window
        .into_iter()
        .map(|((_timestamp_from, _timestamp_to), rows)| ObservedWindowSummary {
            result_count: rows.len(),
            first_result_latency_ms: rows
                .iter()
                .map(|row| row.received_after_first_event_ms)
                .fold(f64::INFINITY, f64::min),
        })
        .collect()
}
