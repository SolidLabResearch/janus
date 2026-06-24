use super::helpers::{
    historical_baseline_sparql_query, hybrid_query, sustained_hybrid_query, unique_config,
};
use super::types::{
    CoordinationWorkload, DatasetSpec, HistoricalDataset, SustainedWorkload, BASELINE_PREDICATE,
    GRAPH_URI, TRAFFIC_PREDICATE,
};
use crate::{core::RDFEvent, storage::segmented_storage::StreamingSegmentedStorage};
use std::{fs, fs::File, io::Write, path::Path, sync::Arc};

pub fn generate_citybench_dataset(
    size_quads: usize,
    output_dir: &Path,
) -> Result<HistoricalDataset, Box<dyn std::error::Error>> {
    let logs_dir = output_dir.join("logs");
    fs::create_dir_all(&logs_dir)?;
    let log_path = logs_dir.join(format!("citybench_{size_quads}.nq"));

    let mut log_file = File::create(&log_path)?;
    let storage = Arc::new(StreamingSegmentedStorage::new(unique_config("paper_scaling"))?);
    let start_ts = 1_720_000_000_000;
    let fixed_window = 1_000usize.min(size_quads.max(1));
    let range_10 = (size_quads / 10).max(1);
    let range_50 = (size_quads / 2).max(1);
    let midpoint = size_quads / 2;
    let point_ts = start_ts + midpoint as u64;
    let point_subject = format!("http://example.org/junction/{}", midpoint % 256);

    for index in 0..size_quads {
        let event = citybench_event(start_ts + index as u64, index);
        writeln!(
            log_file,
            "{} <{}> <{}> \"{}\" <{}> .",
            event.timestamp, event.subject, event.predicate, event.object, event.graph
        )?;
        storage.write_rdf_event(event)?;
    }
    storage.flush()?;

    Ok(HistoricalDataset {
        storage,
        spec: DatasetSpec {
            size_quads,
            start_ts,
            end_ts: start_ts + size_quads.saturating_sub(1) as u64,
            point_ts,
            point_subject,
            fixed_start: start_ts + midpoint.saturating_sub(fixed_window / 2) as u64,
            fixed_end: start_ts
                + midpoint.saturating_sub(fixed_window / 2) as u64
                + fixed_window as u64,
            proportional_10_end: start_ts + range_10 as u64,
            proportional_50_end: start_ts + range_50 as u64,
        },
    })
}

pub fn citybench_event(timestamp: u64, index: usize) -> RDFEvent {
    let junction = index % 256;
    let flow = 20 + (index % 80);
    RDFEvent::new(
        timestamp,
        &format!("http://example.org/junction/{junction}"),
        TRAFFIC_PREDICATE,
        &flow.to_string(),
        GRAPH_URI,
    )
}

pub fn prepare_coordination_workload(
    historical_events: usize,
    live_events: usize,
) -> Result<CoordinationWorkload, Box<dyn std::error::Error>> {
    let historical_start_ts = 1_800_000_000_000;
    let historical_storage = build_historical_storage(historical_events, historical_start_ts)?;
    let historical_rdf_events = historical_storage.query_rdf(
        historical_start_ts,
        historical_start_ts + historical_events.saturating_sub(1) as u64,
    )?;
    Ok(CoordinationWorkload {
        historical_storage,
        historical_rdf_events,
        live_events: build_live_events(live_events, 1_900_000_000_000),
        historical_start_ts,
        historical_end_ts: historical_start_ts + historical_events.saturating_sub(1) as u64,
        historical_sparql_query: historical_baseline_sparql_query()?,
        hybrid_query: hybrid_query(
            historical_start_ts,
            historical_start_ts + historical_events.saturating_sub(1) as u64,
        ),
    })
}

pub fn build_historical_storage(
    events: usize,
    start_ts: u64,
) -> Result<Arc<StreamingSegmentedStorage>, Box<dyn std::error::Error>> {
    let storage = Arc::new(StreamingSegmentedStorage::new(unique_config("paper_h1"))?);
    for index in 0..events {
        let event = RDFEvent::new(
            start_ts + index as u64,
            &format!("http://example.org/junction/{}", index % 64),
            BASELINE_PREDICATE,
            &(40 + (index % 17)).to_string(),
            GRAPH_URI,
        );
        storage.write_rdf_event(event)?;
    }
    storage.flush()?;
    Ok(storage)
}

pub fn build_live_events(events: usize, start_ts: u64) -> Vec<RDFEvent> {
    (0..events)
        .map(|index| {
            RDFEvent::new(
                start_ts + index as u64,
                &format!("http://example.org/junction/{}", index % 64),
                TRAFFIC_PREDICATE,
                &(70 + (index % 11)).to_string(),
                GRAPH_URI,
            )
        })
        .collect()
}

pub fn prepare_sustained_workload(
    historical_events: usize,
    live_duration_seconds: usize,
    event_rate_hz: usize,
    window_size_seconds: usize,
    window_slide_seconds: usize,
) -> Result<SustainedWorkload, Box<dyn std::error::Error>> {
    let historical_start_ts = 1_800_000_000_000;
    let historical_storage = build_historical_storage(historical_events, historical_start_ts)?;
    let historical_rdf_events = historical_storage.query_rdf(
        historical_start_ts,
        historical_start_ts + historical_events.saturating_sub(1) as u64,
    )?;
    let live_events =
        build_sustained_live_events(live_duration_seconds, event_rate_hz, 1_900_000_000_000);
    let window_size_ms = window_size_seconds * 1000;
    let window_slide_ms = window_slide_seconds * 1000;

    Ok(SustainedWorkload {
        historical_storage,
        historical_rdf_events,
        live_events,
        historical_start_ts,
        historical_end_ts: historical_start_ts + historical_events.saturating_sub(1) as u64,
        historical_sparql_query: historical_baseline_sparql_query()?,
        hybrid_query: sustained_hybrid_query(
            historical_start_ts,
            historical_start_ts + historical_events.saturating_sub(1) as u64,
            window_size_ms,
            window_slide_ms,
        ),
    })
}

pub fn build_sustained_live_events(
    duration_sec: usize,
    rate_hz: usize,
    start_ts: u64,
) -> Vec<RDFEvent> {
    let total_events = duration_sec * rate_hz;
    if total_events == 0 {
        return Vec::new();
    }
    let interval_ms = 1000 / rate_hz;
    (0..total_events)
        .map(|index| {
            RDFEvent::new(
                start_ts + (index * interval_ms) as u64,
                &format!("http://example.org/junction/{}", index % 64),
                TRAFFIC_PREDICATE,
                &(70 + (index % 11)).to_string(),
                GRAPH_URI,
            )
        })
        .collect()
}
