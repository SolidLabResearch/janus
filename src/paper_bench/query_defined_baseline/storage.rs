use std::convert::TryFrom;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::types::{
    HistoricalWriteStats, LiveReplayMode, PreparedStorage, QueryDefinedBaselineProfile,
    ResolvedLiveReplayConfig,
};
use super::PREFIX;
use crate::{
    core::RDFEvent,
    parsing::janusql_parser::{SourceKind, WindowDefinition, WindowType},
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};

pub fn sensor_iri(sensor_idx: usize) -> String {
    format!("{PREFIX}sensor{sensor_idx}")
}

pub fn smoke_historical_window(
    min_timestamp: u64,
    max_timestamp: u64,
) -> Result<WindowDefinition, Box<dyn std::error::Error>> {
    Ok(WindowDefinition {
        window_name: format!("{PREFIX}historyDay"),
        source_kind: SourceKind::Log,
        source_name: format!("{PREFIX}stream"),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(min_timestamp),
        end: Some(max_timestamp),
        window_type: WindowType::HistoricalFixed,
    })
}

pub fn prepare_storage(
    profile: QueryDefinedBaselineProfile,
    historical_events_count: usize,
    baseline_entities_count: usize,
    verbose: bool,
) -> Result<PreparedStorage, Box<dyn std::error::Error>> {
    if historical_events_count == 0 {
        return Err("historical_events must be at least 1".into());
    }
    if baseline_entities_count == 0 {
        return Err("baseline_entities must be at least 1".into());
    }
    if historical_events_count < baseline_entities_count {
        return Err("historical_events must be greater than or equal to baseline_entities".into());
    }

    let storage = StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: format!(
            "/tmp/janus_query_defined_baseline_{}_{}_{}",
            profile.as_str(),
            historical_events_count,
            baseline_entities_count
        ),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    })?;

    let historical_stats = write_historical_events(
        &storage,
        historical_events_count,
        baseline_entities_count,
        verbose,
    )?;
    let historical_generation_ms = historical_stats.generation_ms;
    let storage_write_ms = historical_stats.storage_write_ms;

    let live_events = generate_accelerated_live_events(baseline_entities_count);

    Ok(PreparedStorage {
        storage: Arc::new(storage),
        historical_min_timestamp: historical_stats.min_timestamp,
        historical_max_timestamp: historical_stats.max_timestamp,
        historical_generation_ms,
        storage_write_ms,
        live_events,
    })
}

pub fn write_historical_events(
    storage: &StreamingSegmentedStorage,
    historical_events_count: usize,
    baseline_entities_count: usize,
    verbose: bool,
) -> Result<HistoricalWriteStats, Box<dyn std::error::Error>> {
    let mut min_timestamp = 0u64;
    let mut max_timestamp = 0u64;
    let mut initialized = false;
    let mut timestamp = 10u64;
    let mut generation_ms = 0.0;
    let mut write_ms = 0.0;

    for event_idx in 0..historical_events_count {
        if verbose && event_idx > 0 && event_idx % 1_000_000 == 0 {
            eprintln!(
                "[query_defined_baseline] historical_events_written={event_idx}/{historical_events_count}"
            );
        }

        let event_started = Instant::now();
        let sensor_idx = event_idx % baseline_entities_count;
        let value = 10 + i64::try_from(sensor_idx).expect("sensor index fits in i64");
        let event = RDFEvent::new(
            timestamp,
            &sensor_iri(sensor_idx),
            &format!("{PREFIX}hasValue"),
            &value.to_string(),
            &format!("{PREFIX}stream"),
        );
        generation_ms += event_started.elapsed().as_secs_f64() * 1_000.0;

        let write_started = Instant::now();
        storage.write_rdf_event(event)?;
        write_ms += write_started.elapsed().as_secs_f64() * 1_000.0;

        if !initialized {
            min_timestamp = timestamp;
            max_timestamp = timestamp;
            initialized = true;
        } else {
            min_timestamp = min_timestamp.min(timestamp);
            max_timestamp = max_timestamp.max(timestamp);
        }
        timestamp += 10;
    }

    if !initialized {
        return Err("missing historical benchmark events".into());
    }

    Ok(HistoricalWriteStats {
        min_timestamp,
        max_timestamp,
        generation_ms,
        storage_write_ms: write_ms,
    })
}

pub fn generate_accelerated_live_events(baseline_entities_count: usize) -> Vec<RDFEvent> {
    let mut events = Vec::with_capacity(baseline_entities_count);
    for sensor_idx in 0..baseline_entities_count {
        let timestamp = 1000 + (sensor_idx as u64 * 10);
        let value = 20 + i64::try_from(sensor_idx).expect("sensor index fits in i64") * 10;
        events.push(RDFEvent::new(
            timestamp,
            &sensor_iri(sensor_idx),
            &format!("{PREFIX}hasValue"),
            &value.to_string(),
            &format!("{PREFIX}stream"),
        ));
    }
    events
}

pub fn generate_realtime_live_events(
    baseline_entities_count: usize,
    live_event_count: usize,
    event_interval_ms: f64,
) -> Vec<RDFEvent> {
    let mut events = Vec::with_capacity(live_event_count);
    let start_timestamp = 1_900_000_000_000u64;
    for event_idx in 0..live_event_count {
        let sensor_idx = event_idx % baseline_entities_count;
        let timestamp = start_timestamp + ((event_idx as f64) * event_interval_ms).round() as u64;
        let value = 20 + i64::try_from(sensor_idx).expect("sensor index fits in i64") * 10;
        events.push(RDFEvent::new(
            timestamp,
            &sensor_iri(sensor_idx),
            &format!("{PREFIX}hasValue"),
            &value.to_string(),
            &format!("{PREFIX}stream"),
        ));
    }
    events
}

pub fn build_live_events_for_replay(
    prepared: &PreparedStorage,
    live_replay: &ResolvedLiveReplayConfig,
    baseline_entities_count: usize,
) -> Vec<RDFEvent> {
    match live_replay.mode {
        LiveReplayMode::Accelerated => prepared.live_events.clone(),
        LiveReplayMode::Realtime => generate_realtime_live_events(
            baseline_entities_count,
            live_replay.live_event_count,
            live_replay.event_interval_ms,
        ),
    }
}

pub fn realtime_close_timestamp(
    live_events: &[RDFEvent],
    live_replay: &ResolvedLiveReplayConfig,
) -> Result<i64, Box<dyn std::error::Error>> {
    let last_timestamp = live_events.last().ok_or("missing live benchmark events")?.timestamp;
    let window_size_ms = live_replay
        .live_window_size_seconds
        .ok_or("missing realtime window size")?
        .saturating_mul(1000);
    let event_interval_ms = live_replay.event_interval_ms.ceil() as u64;
    let close_timestamp = last_timestamp
        .saturating_add(window_size_ms)
        .saturating_add(event_interval_ms)
        .saturating_add(1);
    Ok(i64::try_from(close_timestamp)?)
}
