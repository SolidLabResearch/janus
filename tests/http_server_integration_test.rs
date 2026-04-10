use futures_util::StreamExt;
use janus::{
    api::janus_api::{JanusApi, QueryResult, ResultSource},
    http::server::{create_server_with_state, AppState, QueryResultBroadcast},
    parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use reqwest::Client;
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tokio::{
    net::TcpListener,
    sync::broadcast,
    task::JoinHandle,
    time::{sleep, Duration},
};
use tokio_tungstenite::{connect_async, tungstenite::Error as WsError};

struct TestServer {
    base_url: String,
    ws_base_url: String,
    client: Client,
    state: Arc<AppState>,
    storage_dir: PathBuf,
    _temp_dir: TempDir,
    server_task: JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

fn historical_query(query_id: &str) -> Value {
    json!({
        "query_id": query_id,
        "janusql": r#"
            PREFIX ex: <http://example.org/>

            SELECT ?sensor ?temp

            FROM NAMED WINDOW ex:hist ON LOG ex:historicalAccl [START 1000 END 2000]

            WHERE {
                WINDOW ex:hist { ?sensor ex:temperature ?temp }
            }
        "#
    })
}

async fn spawn_test_server() -> TestServer {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let storage_dir = temp_dir.path().to_path_buf();
    let mut storage = StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: storage_dir.to_string_lossy().into_owned(),
        max_batch_events: 10,
        max_batch_age_seconds: 60,
        max_batch_bytes: 1024 * 1024,
        sparse_interval: 10,
        entries_per_index_block: 100,
    })
    .expect("failed to create storage");
    storage.start_background_flushing();
    storage
        .write_rdf(
            1_000,
            "http://example.org/sensor1",
            "http://example.org/temperature",
            "21",
            "http://example.org/sensors",
        )
        .expect("failed to write rdf");
    storage.flush().expect("failed to flush storage");

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

    let (app, state) = create_server_with_state(janus_api, registry, storage);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("failed to bind listener");
    let addr = listener.local_addr().expect("failed to read local addr");
    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test server crashed");
    });

    sleep(Duration::from_millis(50)).await;

    TestServer {
        base_url: format!("http://{}", addr),
        ws_base_url: format!("ws://{}", addr),
        client: Client::new(),
        state,
        storage_dir,
        _temp_dir: temp_dir,
        server_task,
    }
}

#[tokio::test]
async fn test_health_endpoint() {
    let server = spawn_test_server().await;

    let response = server
        .client
        .get(format!("{}/health", server.base_url))
        .send()
        .await
        .expect("health request failed");

    assert!(response.status().is_success());
    let body: Value = response.json().await.expect("invalid health response");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["message"], "Janus HTTP API is running");
    assert_eq!(body["storage_status"], "ok");
    assert_eq!(body["storage_error"], Value::Null);
}

#[tokio::test]
async fn test_health_endpoint_reports_storage_degradation() {
    let server = spawn_test_server().await;

    fs::remove_dir_all(&server.storage_dir).expect("failed to remove storage directory");
    for timestamp in 2_000..2_010 {
        server
            .state
            .storage
            .write_rdf(
                timestamp,
                "http://example.org/sensor2",
                "http://example.org/temperature",
                "22",
                "http://example.org/sensors",
            )
            .expect("initial writes should succeed before background failure is observed");
    }

    sleep(Duration::from_millis(250)).await;

    let response = server
        .client
        .get(format!("{}/health", server.base_url))
        .send()
        .await
        .expect("health request failed");

    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json().await.expect("invalid health response");
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["storage_status"], "error");
    assert!(body["storage_error"]
        .as_str()
        .expect("storage error should be present")
        .contains("Background flush failed"));
}

#[tokio::test]
async fn test_query_lifecycle_register_list_get_delete() {
    let server = spawn_test_server().await;

    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&historical_query("http_lifecycle"))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let list_response = server
        .client
        .get(format!("{}/api/queries", server.base_url))
        .send()
        .await
        .expect("list request failed");
    assert!(list_response.status().is_success());
    let list_body: Value = list_response.json().await.expect("invalid list response");
    assert_eq!(list_body["total"], 1);
    assert_eq!(list_body["queries"][0], "http_lifecycle");

    let get_response = server
        .client
        .get(format!("{}/api/queries/http_lifecycle", server.base_url))
        .send()
        .await
        .expect("get request failed");
    assert!(get_response.status().is_success());
    let get_body: Value = get_response.json().await.expect("invalid query response");
    assert_eq!(get_body["query_id"], "http_lifecycle");
    assert_eq!(get_body["is_running"], false);
    assert_eq!(get_body["status"], "Registered");

    let delete_response = server
        .client
        .delete(format!("{}/api/queries/http_lifecycle", server.base_url))
        .send()
        .await
        .expect("delete request failed");
    assert!(delete_response.status().is_success());

    let missing_response = server
        .client
        .get(format!("{}/api/queries/http_lifecycle", server.base_url))
        .send()
        .await
        .expect("missing get request failed");
    assert_eq!(missing_response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_stop_route_stops_running_query_and_delete_requires_stop() {
    let server = spawn_test_server().await;

    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&historical_query("stop_delete"))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let start_response = server
        .client
        .post(format!("{}/api/queries/stop_delete/start", server.base_url))
        .send()
        .await
        .expect("start request failed");
    assert!(start_response.status().is_success());

    let delete_while_running = server
        .client
        .delete(format!("{}/api/queries/stop_delete", server.base_url))
        .send()
        .await
        .expect("delete running request failed");
    assert_eq!(delete_while_running.status(), reqwest::StatusCode::BAD_REQUEST);
    let error_body: Value = delete_while_running.json().await.expect("invalid error response");
    assert!(
        error_body["error"]
            .as_str()
            .expect("error should be a string")
            .contains("Stop it before deleting"),
        "unexpected delete error body: {error_body:?}"
    );

    let stop_response = server
        .client
        .post(format!("{}/api/queries/stop_delete/stop", server.base_url))
        .send()
        .await
        .expect("stop request failed");
    assert!(stop_response.status().is_success());

    let get_response = server
        .client
        .get(format!("{}/api/queries/stop_delete", server.base_url))
        .send()
        .await
        .expect("get request failed");
    assert!(get_response.status().is_success());
    let get_body: Value = get_response.json().await.expect("invalid get response");
    assert_eq!(get_body["is_running"], false);
    assert_eq!(get_body["status"], "Stopped");
    assert_eq!(get_body["execution_count"], 1);

    let delete_response = server
        .client
        .delete(format!("{}/api/queries/stop_delete", server.base_url))
        .send()
        .await
        .expect("delete request failed");
    assert!(delete_response.status().is_success());
}

#[tokio::test]
async fn test_results_websocket_requires_running_query() {
    let server = spawn_test_server().await;

    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&historical_query("ws_not_started"))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let ws_result =
        connect_async(format!("{}/api/queries/ws_not_started/results", server.ws_base_url)).await;

    match ws_result {
        Err(WsError::Http(response)) => {
            assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
        }
        other => panic!("expected websocket http error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_results_websocket_broadcasts_to_multiple_subscribers() {
    let server = spawn_test_server().await;

    let register_response = server
        .client
        .post(format!("{}/api/queries", server.base_url))
        .json(&historical_query("ws_broadcast"))
        .send()
        .await
        .expect("register request failed");
    assert!(register_response.status().is_success());

    let (sender, _) = broadcast::channel(16);
    server
        .state
        .query_streams
        .lock()
        .unwrap()
        .insert("ws_broadcast".to_string(), QueryResultBroadcast { sender: sender.clone() });

    let (mut first_socket, _) =
        connect_async(format!("{}/api/queries/ws_broadcast/results", server.ws_base_url))
            .await
            .expect("first websocket should connect");
    let (mut second_socket, _) =
        connect_async(format!("{}/api/queries/ws_broadcast/results", server.ws_base_url))
            .await
            .expect("second websocket should connect");

    let mut bindings = HashMap::new();
    bindings.insert("sensor".to_string(), "http://example.org/sensor1".to_string());
    bindings.insert("temp".to_string(), "21".to_string());

    sender
        .send(QueryResult {
            query_id: "ws_broadcast".to_string(),
            timestamp: 1_234,
            source: ResultSource::Historical,
            bindings: vec![bindings],
        })
        .expect("send to subscribers should succeed");

    let first_message = tokio::time::timeout(Duration::from_secs(2), first_socket.next())
        .await
        .expect("timed out waiting for first subscriber")
        .expect("first websocket closed unexpectedly")
        .expect("first websocket message failed");
    let second_message = tokio::time::timeout(Duration::from_secs(2), second_socket.next())
        .await
        .expect("timed out waiting for second subscriber")
        .expect("second websocket closed unexpectedly")
        .expect("second websocket message failed");

    let first_body = parse_ws_json(first_message);
    let second_body = parse_ws_json(second_message);

    assert_eq!(first_body["query_id"], "ws_broadcast");
    assert_eq!(first_body["type"], "result");
    assert_eq!(first_body["source"], "historical");
    assert_eq!(first_body["bindings"][0]["sensor"], "http://example.org/sensor1");
    assert_eq!(first_body, second_body);
}

fn parse_ws_json(message: tokio_tungstenite::tungstenite::Message) -> Value {
    let text = message.into_text().expect("websocket payload should be text");
    serde_json::from_str(&text).expect("websocket message should be valid json")
}
