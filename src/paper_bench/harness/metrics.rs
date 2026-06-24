use super::helpers::canonical_result_hash;
use super::types::{
    CoordinationRow, CoordinationSummaryRow, ScalingFitRow, ScalingRow, ScalingSummaryRow,
    SustainedRow, SustainedSummaryRow,
};
use std::{cmp::Ordering, collections::HashMap};

pub fn summarize_coordination(rows: &[CoordinationRow]) -> Vec<CoordinationSummaryRow> {
    let mut grouped = HashMap::<(String, String), Vec<&CoordinationRow>>::new();
    for row in rows {
        grouped.entry((row.system.clone(), row.mode.clone())).or_default().push(row);
    }

    let mut summary = grouped
        .into_iter()
        .map(|((system, mode), items)| {
            let runs = items.len();
            let historical_equiv_count = items
                .iter()
                .filter(|row| row.historical_equivalent_to_baseline == Some(true))
                .count();
            let hybrid_equiv_count = items
                .iter()
                .filter(|row| row.hybrid_equivalent_to_baseline == Some(true))
                .count();

            CoordinationSummaryRow {
                system,
                mode,
                runs,
                components: items.first().map_or(0, |row| row.components),
                process_boundaries: items.first().map_or(0, |row| row.process_boundaries),
                serialization_steps: items.first().map_or(0, |row| row.serialization_steps),
                p50_e2e_latency_ms: percentile(
                    &items.iter().map(|row| row.e2e_latency_ms).collect::<Vec<_>>(),
                    50.0,
                ),
                p95_e2e_latency_ms: percentile(
                    &items.iter().map(|row| row.e2e_latency_ms).collect::<Vec<_>>(),
                    95.0,
                ),
                avg_useful_engine_work_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.estimated_useful_engine_work_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_coordination_overhead_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.estimated_coordination_overhead_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_external_transfer_bytes: mean_usize(
                    &items
                        .iter()
                        .map(|row| row.estimated_external_transfer_bytes)
                        .collect::<Vec<_>>(),
                ),
                avg_final_result_bytes: mean_usize(
                    &items.iter().map(|row| row.final_result_bytes).collect::<Vec<_>>(),
                ),
                avg_result_count: mean_usize(
                    &items.iter().map(|row| row.result_count).collect::<Vec<_>>(),
                ),
                avg_live_events_published: mean_usize(
                    &items.iter().map(|row| row.live_events_published).collect::<Vec<_>>(),
                ),
                avg_live_events_processed: mean_usize(
                    &items.iter().map(|row| row.live_events_processed).collect::<Vec<_>>(),
                ),
                avg_live_stream_processing_latency_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.live_stream_processing_latency_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_external_join_latency_ms: mean(
                    &items.iter().map(|row| row.external_join_latency_ms).collect::<Vec<_>>(),
                ),
                avg_first_hybrid_result_latency_ms: mean(
                    &items.iter().map(|row| row.first_hybrid_result_latency_ms).collect::<Vec<_>>(),
                ),
                avg_historical_result_count: mean_usize(
                    &items.iter().map(|row| row.historical_result_count).collect::<Vec<_>>(),
                ),
                avg_live_result_count: mean_usize(
                    &items.iter().map(|row| row.live_result_count).collect::<Vec<_>>(),
                ),
                avg_hybrid_result_count: mean_usize(
                    &items.iter().map(|row| row.hybrid_result_count).collect::<Vec<_>>(),
                ),
                historical_equivalence_rate: if runs > 0 {
                    historical_equiv_count as f64 / runs as f64
                } else {
                    0.0
                },
                hybrid_equivalence_rate: if runs > 0 {
                    hybrid_equiv_count as f64 / runs as f64
                } else {
                    0.0
                },
            }
        })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        left.system.cmp(&right.system).then_with(|| left.mode.cmp(&right.mode))
    });
    summary
}

pub fn summarize_scaling(rows: &[ScalingRow]) -> Vec<ScalingSummaryRow> {
    let mut grouped = HashMap::<(usize, String, String), Vec<&ScalingRow>>::new();
    for row in rows {
        grouped
            .entry((row.dataset_size_quads, row.query_type.clone(), row.mode.clone()))
            .or_default()
            .push(row);
    }

    let mut summary = grouped
        .into_iter()
        .map(|((dataset_size_quads, query_type, mode), items)| ScalingSummaryRow {
            dataset_size_quads,
            query_type,
            mode,
            runs: items.len(),
            logical_quads_scanned: items.first().map_or(0, |row| row.logical_quads_scanned),
            selectivity: items.first().map_or(0.0, |row| row.selectivity),
            result_count: items.first().map_or(0, |row| row.result_count),
            p50_latency_ms: percentile(
                &items.iter().map(|row| row.latency_ms).collect::<Vec<_>>(),
                50.0,
            ),
            p95_latency_ms: percentile(
                &items.iter().map(|row| row.latency_ms).collect::<Vec<_>>(),
                95.0,
            ),
            avg_latency_ms: mean(&items.iter().map(|row| row.latency_ms).collect::<Vec<_>>()),
            avg_throughput_quads_per_sec: mean(
                &items.iter().map(|row| row.throughput_quads_per_sec).collect::<Vec<_>>(),
            ),
            max_peak_rss_mb: items
                .iter()
                .filter_map(|row| row.peak_rss_mb)
                .max_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal)),
        })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        left.dataset_size_quads
            .cmp(&right.dataset_size_quads)
            .then_with(|| left.query_type.cmp(&right.query_type))
            .then_with(|| left.mode.cmp(&right.mode))
    });
    summary
}

pub fn summarize_scaling_fit(rows: &[ScalingRow]) -> Vec<ScalingFitRow> {
    let mut grouped = HashMap::<(String, String), Vec<&ScalingRow>>::new();
    for row in rows {
        grouped.entry((row.query_type.clone(), row.mode.clone())).or_default().push(row);
    }

    let mut fit_rows = grouped
        .into_iter()
        .map(|((query_type, mode), items)| {
            let mut points = HashMap::<usize, Vec<f64>>::new();
            for row in items {
                points.entry(row.dataset_size_quads).or_default().push(row.latency_ms);
            }
            let mut sorted_points = points
                .into_iter()
                .map(|(size, values)| (size as f64, mean(&values)))
                .collect::<Vec<_>>();
            sorted_points
                .sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(Ordering::Equal));
            linear_fit_row(&query_type, &mode, &sorted_points)
        })
        .collect::<Vec<_>>();
    fit_rows.sort_by(|left, right| {
        left.query_type.cmp(&right.query_type).then_with(|| left.mode.cmp(&right.mode))
    });
    fit_rows
}

pub fn fill_scaling_percentiles(rows: &mut [ScalingRow]) {
    let mut grouped = HashMap::<(usize, String, String), Vec<usize>>::new();
    for (index, row) in rows.iter().enumerate() {
        grouped
            .entry((row.dataset_size_quads, row.query_type.clone(), row.mode.clone()))
            .or_default()
            .push(index);
    }

    for indexes in grouped.values() {
        let latencies = indexes.iter().map(|index| rows[*index].latency_ms).collect::<Vec<_>>();
        let p50 = percentile(&latencies, 50.0);
        let p95 = percentile(&latencies, 95.0);
        for index in indexes {
            rows[*index].p50_latency_ms = p50;
            rows[*index].p95_latency_ms = p95;
        }
    }
}

pub fn summarize_sustained(rows: &[SustainedRow]) -> Vec<SustainedSummaryRow> {
    let mut grouped = HashMap::<(String, String, String), Vec<&SustainedRow>>::new();
    for row in rows {
        grouped
            .entry((row.system.clone(), row.mode.clone(), row.time_mode.clone()))
            .or_default()
            .push(row);
    }

    let mut summary = grouped
        .into_iter()
        .map(|((system, mode, time_mode), items)| {
            let runs = items.len();
            let equiv_count =
                items.iter().filter(|row| row.equivalent_to_baseline == Some(true)).count();

            SustainedSummaryRow {
                system,
                mode,
                time_mode,
                runs,
                historical_events: items.first().map_or(0, |row| row.historical_events),
                logical_live_duration_seconds: items
                    .first()
                    .map_or(0, |row| row.logical_live_duration_seconds),
                event_rate_hz: items.first().map_or(0, |row| row.event_rate_hz),
                event_interval_ms: items.first().map_or(0.0, |row| row.event_interval_ms),
                expected_wall_clock_duration_ms: items
                    .first()
                    .map_or(0, |row| row.expected_wall_clock_duration_ms),
                window_size_ms: items.first().map_or(0, |row| row.window_size_ms),
                window_slide_ms: items.first().map_or(0, |row| row.window_slide_ms),
                avg_events_published: mean_usize(
                    &items.iter().map(|row| row.events_published).collect::<Vec<_>>(),
                ),
                avg_events_processed: mean_usize(
                    &items.iter().map(|row| row.events_processed).collect::<Vec<_>>(),
                ),
                avg_completed_windows_total: mean_usize(
                    &items.iter().map(|row| row.completed_windows_total).collect::<Vec<_>>(),
                ),
                avg_completed_windows_in_horizon: mean_usize(
                    &items.iter().map(|row| row.completed_windows_in_horizon).collect::<Vec<_>>(),
                ),
                avg_flush_windows: mean_usize(
                    &items.iter().map(|row| row.flush_windows).collect::<Vec<_>>(),
                ),
                avg_missed_windows: mean_usize(
                    &items.iter().map(|row| row.missed_windows).collect::<Vec<_>>(),
                ),
                p50_first_hybrid_result_latency_ms: percentile(
                    &items.iter().map(|row| row.first_hybrid_result_latency_ms).collect::<Vec<_>>(),
                    50.0,
                ),
                p95_first_hybrid_result_latency_ms: percentile(
                    &items.iter().map(|row| row.first_hybrid_result_latency_ms).collect::<Vec<_>>(),
                    95.0,
                ),
                p50_first_hybrid_result_wall_clock_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.first_hybrid_result_wall_clock_ms)
                        .collect::<Vec<_>>(),
                    50.0,
                ),
                p95_first_hybrid_result_wall_clock_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.first_hybrid_result_wall_clock_ms)
                        .collect::<Vec<_>>(),
                    95.0,
                ),
                p50_window_hybrid_latency_ms: percentile(
                    &items.iter().map(|row| row.p50_window_hybrid_latency_ms).collect::<Vec<_>>(),
                    50.0,
                ),
                p95_window_hybrid_latency_ms: percentile(
                    &items.iter().map(|row| row.p95_window_hybrid_latency_ms).collect::<Vec<_>>(),
                    95.0,
                ),
                p50_window_result_wall_clock_offset_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.p50_window_result_wall_clock_offset_ms)
                        .collect::<Vec<_>>(),
                    50.0,
                ),
                p95_window_result_wall_clock_offset_ms: percentile(
                    &items
                        .iter()
                        .map(|row| row.p95_window_result_wall_clock_offset_ms)
                        .collect::<Vec<_>>(),
                    95.0,
                ),
                avg_historical_preparation_latency_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.historical_preparation_latency_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_first_live_window_latency_ms: mean(
                    &items.iter().map(|row| row.first_live_window_latency_ms).collect::<Vec<_>>(),
                ),
                avg_readiness_gap_ms: mean(
                    &items.iter().map(|row| row.readiness_gap_ms).collect::<Vec<_>>(),
                ),
                avg_hybrid_wait_after_inputs_ready_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.hybrid_wait_after_inputs_ready_ms)
                        .collect::<Vec<_>>(),
                ),
                avg_external_join_latency_total_ms: mean(
                    &items.iter().map(|row| row.external_join_latency_total_ms).collect::<Vec<_>>(),
                ),
                avg_external_join_latency_ms: mean(
                    &items.iter().map(|row| row.external_join_latency_avg_ms).collect::<Vec<_>>(),
                ),
                avg_estimated_external_transfer_bytes_total: mean_usize(
                    &items
                        .iter()
                        .map(|row| row.estimated_external_transfer_bytes_total)
                        .collect::<Vec<_>>(),
                ),
                avg_estimated_external_transfer_bytes_per_window: mean_usize(
                    &items
                        .iter()
                        .map(|row| row.estimated_external_transfer_bytes_per_window)
                        .collect::<Vec<_>>(),
                ),
                avg_hybrid_result_count_total: mean_usize(
                    &items.iter().map(|row| row.hybrid_result_count_total).collect::<Vec<_>>(),
                ),
                avg_wall_clock_benchmark_duration_ms: mean(
                    &items
                        .iter()
                        .map(|row| row.wall_clock_benchmark_duration_ms as f64)
                        .collect::<Vec<_>>(),
                ),
                avg_wall_clock_overhead_ms: mean(
                    &items.iter().map(|row| row.wall_clock_overhead_ms).collect::<Vec<_>>(),
                ),
                uses_virtual_event_time: items
                    .first()
                    .map_or(true, |row| row.uses_virtual_event_time),
                equivalence_rate: if runs > 0 {
                    equiv_count as f64 / runs as f64
                } else {
                    0.0
                },
            }
        })
        .collect::<Vec<_>>();
    summary.sort_by(|left, right| {
        left.system
            .cmp(&right.system)
            .then_with(|| left.mode.cmp(&right.mode))
            .then_with(|| left.time_mode.cmp(&right.time_mode))
    });
    summary
}

pub fn linear_fit_row(query_type: &str, mode: &str, points: &[(f64, f64)]) -> ScalingFitRow {
    if points.is_empty() {
        return ScalingFitRow {
            query_type: query_type.to_string(),
            mode: mode.to_string(),
            slope_ms_per_100k_quads: 0.0,
            intercept_ms: 0.0,
            r_squared: 0.0,
            number_of_points: 0,
        };
    }

    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let numerator = points.iter().map(|(x, y)| (x - mean_x) * (y - mean_y)).sum::<f64>();
    let denominator = points.iter().map(|(x, _)| (x - mean_x).powi(2)).sum::<f64>();
    let slope = if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    };
    let intercept = mean_y - slope * mean_x;

    let ss_tot = points.iter().map(|(_, y)| (y - mean_y).powi(2)).sum::<f64>();
    let ss_res = points
        .iter()
        .map(|(x, y)| {
            let predicted = intercept + slope * x;
            (y - predicted).powi(2)
        })
        .sum::<f64>();
    let r_squared = if ss_tot == 0.0 {
        1.0
    } else {
        1.0 - (ss_res / ss_tot)
    };

    ScalingFitRow {
        query_type: query_type.to_string(),
        mode: mode.to_string(),
        slope_ms_per_100k_quads: slope * 100_000.0,
        intercept_ms: intercept,
        r_squared,
        number_of_points: points.len(),
    }
}

pub fn percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let rank = ((pct / 100.0) * (sorted.len().saturating_sub(1) as f64)).round() as usize;
    sorted[rank]
}

pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

pub fn mean_usize(values: &[usize]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<usize>() as f64 / values.len() as f64
    }
}
