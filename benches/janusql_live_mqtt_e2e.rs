mod support;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use futures_util::StreamExt;
use janus::{
    api::janus_api::JanusApi,
    http::server::{create_server_with_state, AppState},
    parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry,
    storage::segmented_storage::StreamingSegmentedStorage,
};
use reqwest::Client;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration as StdDuration};
use tokio::{net::TcpListener, runtime::Runtime, task::JoinHandle, time::Duration};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use support::{recent_base_timestamp, unique_config, BASELINE_PREDICATE, GRAPH_URI};

const DEFAULT_MQTT_HOST: &str = "127.0.0.1";
const DEFAULT_MQTT_PORT: u16 = 1883;
const DEFAULT_MQTT_TOPIC: &str = "sensors";
const BASELINE_NS: &str = "https://janus.rs/baseline#";
const HYBRID_BASELINE_START: u64 = 1_700_000_000_000;
const HYBRID_BASELINE_END: u64 = HYBRID_BASELINE_START + 5_000;
const LIVE_WINDOW_RANGE_MS: u64 = 50;
const LIVE_WINDOW_STEP_MS: u64 = 10;
const MQTT_EVENT_SPACING_MS: u64 = 15;
const MQTT_SENTINEL_ADVANCE_MS: u64 = 75;

struct BenchServer {
    base_url: String,
    ws_base_url: String,
    client: Client,
    server_task: JoinHandle<()>,
}

impl Drop for BenchServer {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

#[derive(Clone)]
struct MqttConfig {
    host: String,
    port: u16,
    topic: String,
    stream_uri: String,
}

impl MqttConfig {
    fn from_env() -> Self {
        let host = std::env::var("JANUS_BENCH_MQTT_HOST")
            .unwrap_or_else(|_| DEFAULT_MQTT_HOST.to_string());

        let port = std::env::var("JANUS_BENCH_MQTT_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(DEFAULT_MQTT_PORT);

        let topic = std::env::var("JANUS_BENCH_MQTT_TOPIC")
            .unwrap_or_else(|_| DEFAULT_MQTT_TOPIC.to_string());

        let stream_uri = std::env::var("JANUS_BENCH_MQTT_STREAM_URI")
            .unwrap_or_else(|_| format!("mqtt://{}:{}/{}", host, port, topic));

        Self { host, port, topic, stream_uri }
    }
}

async fn spawn_server() -> BenchServer {
    let storage = StreamingSegmentedStorage::new(unique_config("janusql_live_mqtt_e2e"))
        .expect("failed to create benchmark storage");
    populate_hybrid_baseline_storage(&storage);
    storage.flush().expect("failed to flush benchmark storage");
    let storage = Arc::new(storage);
    let registry = Arc::new(QueryRegistry::new());
    let janus_api = Arc::new(
        JanusApi::new(
            JanusQLParser::new().expect("failed to create parser"),
            Arc::clone(&registry),
            Arc::clone(&storage),
        )
        .expect("failed to create api"),
    );

    let (app, _state): (_, Arc<AppState>) = create_server_with_state(janus_api, registry, storage);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind benchmark listener");
    let addr = listener.local_addr().expect("failed to read benchmark listener addr");

    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("benchmark server crashed");
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    BenchServer {
        base_url: format!("http://{}", addr),
        ws_base_url: format!("ws://{}", addr),
        client: Client::new(),
        server_task,
    }
}

fn live_query(query_id: &str, stream_uri: &str) -> Value {
    json!({
        "query_id": query_id,
        "janusql": format!(
            r#"
            PREFIX ex: <http://example.org/>

            SELECT ?sensor ?temp
            FROM NAMED WINDOW ex:live ON STREAM <{stream_uri}> [RANGE {LIVE_WINDOW_RANGE_MS} STEP {LIVE_WINDOW_STEP_MS}]
            WHERE {{
                WINDOW ex:live {{
                    ?sensor ex:temperature ?temp .
                }}
            }}
            "#
        )
    })
}

fn hybrid_query(query_id: &str, stream_uri: &str) -> Value {
    json!({
        "query_id": query_id,
        "janusql": format!(
            r#"
            PREFIX ex: <http://example.org/>
            PREFIX baseline: <{BASELINE_NS}>

            SELECT ?sensor ?liveTemp ?baselineTemp
            FROM NAMED WINDOW ex:hist ON STREAM <{GRAPH_URI}> [START {HYBRID_BASELINE_START} END {HYBRID_BASELINE_END}]
            FROM NAMED WINDOW ex:live ON STREAM <{stream_uri}> [RANGE {LIVE_WINDOW_RANGE_MS} STEP {LIVE_WINDOW_STEP_MS}]
            USING BASELINE ex:hist AGGREGATE
            WHERE {{
                WINDOW ex:hist {{
                    ?sensor ex:baselineTemperature ?baselineTemp .
                }}
                WINDOW ex:live {{
                    ?sensor ex:temperature ?liveTemp .
                }}
                ?sensor baseline:baselineTemp ?baselineTemp .
            }}
            "#
        )
    })
}

fn populate_hybrid_baseline_storage(storage: &StreamingSegmentedStorage) {
    for i in 0..5u64 {
        storage
            .write_rdf(
                HYBRID_BASELINE_START + i,
                &format!("http://example.org/sensor{i}"),
                BASELINE_PREDICATE,
                &format!("{}", 20 + i),
                GRAPH_URI,
            )
            .expect("failed to write baseline benchmark data");
    }
}

async fn open_result_socket(
    ws_base_url: &str,
    query_id: &str,
) -> WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>> {
    let (socket, _) = connect_async(format!("{ws_base_url}/api/queries/{query_id}/results"))
        .await
        .expect("websocket should connect");
    socket
}

async fn wait_for_ws_result(socket: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>) {
    let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .expect("timed out waiting for websocket result")
        .expect("websocket closed unexpectedly")
        .expect("websocket message failed");

    let text = message.into_text().expect("websocket payload should be text");
    let body: Value = serde_json::from_str(&text).expect("websocket payload should be valid json");
    assert_eq!(body["type"], "result");
    assert!(
        body["bindings"].as_array().is_some_and(|bindings| !bindings.is_empty()),
        "expected at least one binding in websocket result"
    );
}

async fn wait_for_query_status(server: &BenchServer, query_id: &str, expected_status: &str) {
    for _ in 0..100 {
        let response = server
            .client
            .get(format!("{}/api/queries/{query_id}", server.base_url))
            .send()
            .await
            .expect("query status request failed");
        assert!(response.status().is_success(), "query details response should succeed");
        let body: Value = response.json().await.expect("invalid query details response");
        if body["status"] == Value::String(expected_status.to_string()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("query did not reach status '{expected_status}' within timeout");
}

async fn cleanup_query(server: &BenchServer, query_id: &str) {
    let stop_response = server
        .client
        .post(format!("{}/api/queries/{query_id}/stop", server.base_url))
        .send()
        .await
        .expect("stop request failed");
    assert!(stop_response.status().is_success(), "stop response should succeed");

    let delete_response = server
        .client
        .delete(format!("{}/api/queries/{query_id}", server.base_url))
        .send()
        .await
        .expect("delete request failed");
    assert!(delete_response.status().is_success(), "delete response should succeed");
}

fn build_mqtt_payload(ts: u64, sensor_idx: usize, temp: i64) -> String {
    format!(
        "{} <http://example.org/sensor{}> <http://example.org/temperature> \"{}\" <http://example.org/graph1> .",
        ts, sensor_idx, temp
    )
}

fn build_mqtt_sentinel_payload(ts: u64) -> String {
    format!(
        "{ts} <urn:rsp:sentinel:subject> <urn:rsp:sentinel:predicate> <urn:rsp:sentinel:object> <http://example.org/graph1> ."
    )
}

async fn publish_events(cfg: &MqttConfig, events: usize, base_ts: u64) {
    let client_id = format!("janus_bench_pub_{}_{}", std::process::id(), base_ts);
    let mut options = MqttOptions::new(client_id, cfg.host.clone(), cfg.port);
    options.set_keep_alive(Duration::from_secs(5));

    let (client, mut eventloop) = AsyncClient::new(options, 32);

    // Prime connection/sub-acks processing path.
    tokio::spawn(async move {
        // Keep polling briefly; benchmark will be short-lived.
        for _ in 0..200usize {
            let _ = tokio::time::timeout(Duration::from_millis(50), eventloop.poll()).await;
        }
    });

    // Give server subscriber a moment to subscribe before publish burst.
    tokio::time::sleep(Duration::from_millis(80)).await;

    for i in 0..events {
        let payload = build_mqtt_payload(base_ts + i as u64, i % 5, 20 + (i % 10) as i64);
        client
            .publish(cfg.topic.clone(), QoS::AtLeastOnce, false, payload)
            .await
            .expect("failed to publish mqtt event");
        tokio::time::sleep(Duration::from_millis(MQTT_EVENT_SPACING_MS)).await;
    }

    tokio::time::sleep(Duration::from_millis(MQTT_SENTINEL_ADVANCE_MS)).await;
    client
        .publish(
            cfg.topic.clone(),
            QoS::AtLeastOnce,
            false,
            build_mqtt_sentinel_payload(base_ts + events as u64 + 1),
        )
        .await
        .expect("failed to publish mqtt sentinel event");
    tokio::time::sleep(Duration::from_millis(30)).await;
}

async fn bench_live_mqtt_first_result(
    server: &BenchServer,
    mqtt_cfg: &MqttConfig,
    query_id: &str,
    publish_events_count: usize,
) -> StdDuration {
    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&live_query(query_id, &mqtt_cfg.stream_uri))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let start_response = server
        .client
        .post(format!("{}/api/queries/{query_id}/start", server.base_url))
        .send()
        .await
        .expect("start request failed");
    assert!(start_response.status().is_success());

    let started_at = std::time::Instant::now();
    let mut socket = open_result_socket(&server.ws_base_url, query_id).await;
    let publish_ts = recent_base_timestamp(0);
    publish_events(mqtt_cfg, publish_events_count, publish_ts).await;
    wait_for_ws_result(&mut socket).await;
    let elapsed = started_at.elapsed();

    cleanup_query(server, query_id).await;
    elapsed
}

async fn bench_hybrid_mqtt_first_result(
    server: &BenchServer,
    mqtt_cfg: &MqttConfig,
    query_id: &str,
    publish_events_count: usize,
) -> StdDuration {
    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&hybrid_query(query_id, &mqtt_cfg.stream_uri))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let start_response = server
        .client
        .post(format!("{}/api/queries/{query_id}/start", server.base_url))
        .send()
        .await
        .expect("start request failed");
    assert!(start_response.status().is_success());

    let started_at = std::time::Instant::now();
    wait_for_query_status(server, query_id, "Running").await;
    let mut socket = open_result_socket(&server.ws_base_url, query_id).await;
    let publish_ts = recent_base_timestamp(0);
    publish_events(mqtt_cfg, publish_events_count, publish_ts).await;
    wait_for_ws_result(&mut socket).await;
    let elapsed = started_at.elapsed();

    cleanup_query(server, query_id).await;
    elapsed
}

fn janusql_live_mqtt_e2e(c: &mut Criterion) {
    let runtime = Runtime::new().expect("failed to create benchmark runtime");
    let mqtt_cfg = MqttConfig::from_env();

    let mut group = c.benchmark_group("janusql_e2e/live_mqtt_http_first_result");
    group.sample_size(10);

    for &events in &[1usize, 10, 50] {
        group.bench_with_input(
            BenchmarkId::new("mqtt_live_first_result", events),
            &events,
            |b, &events| {
                b.iter_batched(
                    || runtime.block_on(spawn_server()),
                    |server| {
                        let query_id =
                            format!("e2e_live_mqtt_{}_{}", events, recent_base_timestamp(0));
                        black_box(runtime.block_on(bench_live_mqtt_first_result(
                            &server, &mqtt_cfg, &query_id, events,
                        )))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();

    let mut hybrid_group = c.benchmark_group("janusql_e2e/hybrid_mqtt_http_first_result");
    hybrid_group.sample_size(10);

    for &events in &[1usize, 10, 50] {
        hybrid_group.bench_with_input(
            BenchmarkId::new("mqtt_hybrid_first_result", events),
            &events,
            |b, &events| {
                b.iter_batched(
                    || runtime.block_on(spawn_server()),
                    |server| {
                        let query_id =
                            format!("e2e_hybrid_mqtt_{}_{}", events, recent_base_timestamp(0));
                        black_box(runtime.block_on(bench_hybrid_mqtt_first_result(
                            &server, &mqtt_cfg, &query_id, events,
                        )))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    hybrid_group.finish();
}

criterion_group!(benches, janusql_live_mqtt_e2e);
criterion_main!(benches);
