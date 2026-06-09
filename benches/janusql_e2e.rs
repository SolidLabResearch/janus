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
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Arc, time::Duration as StdDuration};
use tempfile::TempDir;
use tokio::{net::TcpListener, runtime::Runtime, task::JoinHandle, time::Duration};
use tokio_tungstenite::connect_async;

use support::{
    populate_storage, recent_base_timestamp, unique_config, GRAPH_URI, TEMPERATURE_PREDICATE,
};

struct BenchServer {
    base_url: String,
    ws_base_url: String,
    client: Client,
    storage_dir: PathBuf,
    _temp_dir: TempDir,
    server_task: JoinHandle<()>,
}

impl Drop for BenchServer {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

async fn spawn_server(
    preload_events: usize,
    preload_start_timestamp: u64,
    preload_step_ms: u64,
) -> BenchServer {
    let temp_dir = TempDir::new().expect("failed to create benchmark temp dir");
    let storage_dir = temp_dir.path().to_path_buf();
    let storage = StreamingSegmentedStorage::new(unique_config("janusql_e2e"))
        .expect("failed to create benchmark storage");

    if preload_events > 0 {
        populate_storage(
            &storage,
            preload_events,
            preload_start_timestamp,
            preload_step_ms,
            GRAPH_URI,
        );
        storage.flush().expect("failed to flush benchmark storage");
    }

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

    tokio::time::sleep(Duration::from_millis(25)).await;

    BenchServer {
        base_url: format!("http://{}", addr),
        ws_base_url: format!("ws://{}", addr),
        client: Client::new(),
        storage_dir,
        _temp_dir: temp_dir,
        server_task,
    }
}

fn historical_sliding_query(query_id: &str, graph_uri: &str) -> Value {
    json!({
        "query_id": query_id,
        "janusql": format!(
            r#"
            PREFIX ex: <http://example.org/>

            SELECT ?sensor ?temp

            FROM NAMED WINDOW ex:hist ON STREAM <{graph_uri}> [OFFSET 12000 RANGE 2000 STEP 250]

            WHERE {{
                WINDOW ex:hist {{
                    ?sensor ex:temperature ?temp .
                }}
            }}
            "#
        )
    })
}

fn historical_fixed_query(query_id: &str, graph_uri: &str, start: u64, end: u64) -> Value {
    json!({
        "query_id": query_id,
        "janusql": format!(
            r#"
            PREFIX ex: <http://example.org/>

            SELECT ?sensor ?temp

            FROM NAMED WINDOW ex:hist ON STREAM <{graph_uri}> [START {start} END {end}]

            WHERE {{
                WINDOW ex:hist {{
                    ?sensor ex:temperature ?temp .
                }}
            }}
            "#
        )
    })
}

async fn wait_for_ws_result(ws_base_url: &str, query_id: &str) {
    let (mut socket, _) = connect_async(format!("{ws_base_url}/api/queries/{query_id}/results"))
        .await
        .expect("websocket should connect");

    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
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

async fn bench_preloaded_historical(server: &BenchServer, query_id: &str) -> StdDuration {
    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&historical_sliding_query(query_id, GRAPH_URI))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let started_at = std::time::Instant::now();
    let start_response = server
        .client
        .post(format!("{}/api/queries/{query_id}/start", server.base_url))
        .send()
        .await
        .expect("start request failed");
    assert!(start_response.status().is_success());

    wait_for_ws_result(&server.ws_base_url, query_id).await;
    let elapsed = started_at.elapsed();

    cleanup_query(server, query_id).await;
    elapsed
}

fn replay_input_contents(graph_uri: &str, start_timestamp: u64, event_count: usize) -> String {
    let mut contents = String::new();
    for i in 0..event_count as u64 {
        contents.push_str(&format!(
            "{} <http://example.org/sensor{}> <{}> \"{}\" <{}> .\n",
            start_timestamp + i,
            i % 5,
            TEMPERATURE_PREDICATE,
            20 + (i % 10),
            graph_uri
        ));
    }
    contents
}

async fn wait_for_replay_completion(server: &BenchServer) {
    for _ in 0..100 {
        let response = server
            .client
            .get(format!("{}/api/replay/status", server.base_url))
            .send()
            .await
            .expect("replay status request failed");
        assert!(response.status().is_success());
        let body: Value = response.json().await.expect("invalid replay status response");
        if body["is_running"] == Value::Bool(false) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("replay did not complete within timeout");
}

async fn bench_replay_historical(
    server: &BenchServer,
    query_id: &str,
    graph_uri: &str,
    start_timestamp: u64,
    event_count: usize,
) -> StdDuration {
    let replay_file = server.storage_dir.join(format!("{query_id}_replay.nq"));
    fs::write(&replay_file, replay_input_contents(graph_uri, start_timestamp, event_count))
        .expect("failed to write replay input file");

    let started_at = std::time::Instant::now();
    let replay_response = server
        .client
        .post(format!("{}/api/replay/start", server.base_url))
        .json(&json!({
            "input_file": replay_file.to_string_lossy().to_string(),
            "broker_type": "none",
            "topics": ["sensors"],
            "rate_of_publishing": 0,
            "loop_file": false,
            "add_timestamps": false
        }))
        .send()
        .await
        .expect("replay start request failed");
    assert!(replay_response.status().is_success());

    wait_for_replay_completion(server).await;

    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&historical_fixed_query(
            query_id,
            graph_uri,
            start_timestamp,
            start_timestamp + event_count as u64,
        ))
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

    wait_for_ws_result(&server.ws_base_url, query_id).await;
    let elapsed = started_at.elapsed();

    cleanup_query(server, query_id).await;
    elapsed
}

fn janusql_e2e(c: &mut Criterion) {
    let runtime = Runtime::new().expect("failed to create benchmark runtime");
    let mut group = c.benchmark_group("janusql_e2e/http_first_result");
    group.sample_size(20);

    for &events in &[1_000usize, 10_000] {
        group.bench_with_input(
            BenchmarkId::new("historical_preloaded", events),
            &events,
            |b, &events| {
                b.iter_batched(
                    || {
                        let start_ts = recent_base_timestamp(9_000);
                        runtime.block_on(spawn_server(events, start_ts, 1))
                    },
                    |server| {
                        let query_id = format!("e2e_hist_preloaded_{events}");
                        black_box(runtime.block_on(bench_preloaded_historical(&server, &query_id)))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    for &events in &[100usize, 1_000] {
        group.bench_with_input(
            BenchmarkId::new("historical_via_replay", events),
            &events,
            |b, &events| {
                b.iter_batched(
                    || runtime.block_on(spawn_server(0, 0, 1)),
                    |server| {
                        let start_ts = 1_000_000 + (events as u64 * 10);
                        let query_id = format!("e2e_hist_replay_{events}");
                        black_box(runtime.block_on(bench_replay_historical(
                            &server, &query_id, GRAPH_URI, start_ts, events,
                        )))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, janusql_e2e);
criterion_main!(benches);
