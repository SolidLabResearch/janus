//! HTTP handlers for managing stream bus replay.

use crate::{
    http::{
        error::ApiError,
        types::{AppState, ReplayState, ReplayStatusResponse, StartReplayRequest, SuccessResponse},
    },
    stream_bus::{BrokerType, MqttConfig, StreamBus, StreamBusConfig},
};
use axum::{extract::State, Json};
use std::{
    sync::{
        atomic::Ordering,
        Arc,
    },
    time::Instant,
};

fn schedule_stream_bus_drop(stream_bus: Arc<StreamBus>) {
    std::thread::spawn(move || {
        drop(stream_bus);
    });
}

/// POST /api/replay/start - Start stream bus replay
pub async fn start_replay(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartReplayRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let mut replay_state = state.replay_state.lock().unwrap();

    if replay_state.is_running {
        return Err(ApiError::BadRequest("Replay is already running".to_string()));
    }

    // Parse broker type
    let broker_type = match payload.broker_type.to_lowercase().as_str() {
        "mqtt" => BrokerType::Mqtt,
        "none" => BrokerType::None,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid broker type: {}. Use 'mqtt' or 'none'",
                payload.broker_type
            )))
        }
    };

    // Convert configs
    let mqtt_config = payload.mqtt_config.map(|cfg| MqttConfig {
        host: cfg.host,
        port: cfg.port,
        client_id: cfg.client_id,
        keep_alive_secs: cfg.keep_alive_secs,
    });

    let bus_config = StreamBusConfig {
        input_file: payload.input_file.clone(),
        broker_type,
        topics: payload.topics,
        rate_of_publishing: payload.rate_of_publishing,
        loop_file: payload.loop_file,
        add_timestamps: payload.add_timestamps,
        mqtt_config,
    };

    let storage = Arc::clone(&state.storage);
    let input_file_clone = payload.input_file.clone();

    // Create StreamBus and store it in state
    let stream_bus = Arc::new(StreamBus::new(bus_config, storage));
    let stream_bus_clone = Arc::clone(&stream_bus);

    // Clone metric counters from StreamBus
    let events_read = Arc::clone(&stream_bus.events_read);
    let events_published = Arc::clone(&stream_bus.events_published);
    let events_stored = Arc::clone(&stream_bus.events_stored);
    let publish_errors = Arc::clone(&stream_bus.publish_errors);
    let storage_errors = Arc::clone(&stream_bus.storage_errors);

    let replay_state_clone = Arc::clone(&state.replay_state);

    let old_stream_bus = replay_state.stream_bus.take();
    replay_state.is_running = true;
    replay_state.start_time = Some(Instant::now());
    replay_state.input_file = Some(input_file_clone);
    replay_state.stream_bus = Some(Arc::clone(&stream_bus));
    replay_state.events_read = events_read;
    replay_state.events_published = events_published;
    replay_state.events_stored = events_stored;
    replay_state.publish_errors = publish_errors;
    replay_state.storage_errors = storage_errors;
    drop(replay_state);

    if let Some(bus) = old_stream_bus {
        schedule_stream_bus_drop(bus);
    }

    // Spawn replay in a blocking thread to avoid runtime conflict
    std::thread::spawn(move || {
        if let Err(e) = stream_bus_clone.start() {
            eprintln!("Stream bus replay error: {}", e);
        }

        let finished_bus = replay_state_clone.lock().ok().and_then(|mut rs| {
            rs.is_running = false;
            rs.start_time = None;
            rs.input_file = None;
            println!("Stream bus replay finished");
            rs.stream_bus.take()
        });

        if let Some(bus) = finished_bus {
            schedule_stream_bus_drop(bus);
        }
    });

    Ok(Json(SuccessResponse {
        message: format!("Stream bus replay started with file: {}", payload.input_file),
    }))
}

/// POST /api/replay/stop - Stop stream bus replay
pub async fn stop_replay(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let mut replay_state = state.replay_state.lock().unwrap();

    if !replay_state.is_running {
        return Err(ApiError::BadRequest("Replay is not running".to_string()));
    }

    let stream_bus = replay_state.stream_bus.take();
    if let Some(bus) = stream_bus.as_ref() {
        bus.stop();
    }
    replay_state.is_running = false;
    replay_state.start_time = None;
    replay_state.input_file = None;
    drop(replay_state);

    if let Some(bus) = stream_bus {
        schedule_stream_bus_drop(bus);
    }

    Ok(Json(SuccessResponse { message: "Stream bus replay stopped".to_string() }))
}

/// GET /api/replay/status - Get replay status
pub async fn replay_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReplayStatusResponse>, ApiError> {
    let replay_state = state.replay_state.lock().unwrap();
    Ok(Json(replay_status_snapshot(&replay_state)))
}

pub fn replay_status_snapshot(replay_state: &ReplayState) -> ReplayStatusResponse {
    let elapsed_seconds = if replay_state.is_running {
        replay_state.start_time.map_or(0.0, |t| t.elapsed().as_secs_f64())
    } else {
        0.0
    };

    let events_read = replay_state.events_read.load(Ordering::Relaxed);
    let events_published = replay_state.events_published.load(Ordering::Relaxed);
    let events_stored = replay_state.events_stored.load(Ordering::Relaxed);
    let publish_errors = replay_state.publish_errors.load(Ordering::Relaxed);
    let storage_errors = replay_state.storage_errors.load(Ordering::Relaxed);

    let events_per_second = if elapsed_seconds > 0.0 {
        events_read as f64 / elapsed_seconds
    } else {
        0.0
    };

    ReplayStatusResponse {
        is_running: replay_state.is_running,
        events_read,
        events_published,
        events_stored,
        publish_errors,
        storage_errors,
        events_per_second,
        elapsed_seconds,
    }
}
