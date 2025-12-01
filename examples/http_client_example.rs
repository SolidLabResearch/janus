//! HTTP Client Example for Janus API
//!
//! This example demonstrates how to interact with the Janus HTTP API server.
//! It shows how to:
//! 1. Register queries
//! 2. Start and stop queries
//! 3. List and get query details
//! 4. Start and stop stream bus replay
//! 5. Connect to WebSocket for streaming results
//!
//! Usage:
//!   cargo run --example http_client_example

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
struct RegisterQueryRequest {
    query_id: String,
    janusql: String,
}

#[derive(Debug, Deserialize)]
struct RegisterQueryResponse {
    query_id: String,
    query_text: String,
    registered_at: u64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct SuccessResponse {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ListQueriesResponse {
    queries: Vec<String>,
    total: usize,
}

#[derive(Debug, Deserialize)]
struct QueryDetailsResponse {
    query_id: String,
    query_text: String,
    registered_at: u64,
    execution_count: u64,
    is_running: bool,
    status: String,
}

#[derive(Debug, Serialize)]
struct StartReplayRequest {
    input_file: String,
    broker_type: String,
    topics: Vec<String>,
    rate_of_publishing: u64,
    loop_file: bool,
    add_timestamps: bool,
}

#[derive(Debug, Deserialize)]
struct ReplayStatusResponse {
    is_running: bool,
    events_read: u64,
    events_published: u64,
    events_stored: u64,
    publish_errors: u64,
    storage_errors: u64,
    events_per_second: f64,
    elapsed_seconds: f64,
}

#[derive(Debug, Deserialize)]
struct QueryResultMessage {
    query_id: String,
    timestamp: u64,
    source: String,
    bindings: Vec<HashMap<String, String>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = "http://127.0.0.1:8080";
    let client = reqwest::Client::new();

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║         Janus HTTP API Client Example                         ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Base URL: {}", base_url);
    println!();

    // 1. Health Check
    println!("1. Health Check");
    println!("   GET {}/health", base_url);
    let response = client.get(format!("{}/health", base_url)).send().await?;
    if response.status().is_success() {
        let body: SuccessResponse = response.json().await?;
        println!("   ✓ {}", body.message);
    } else {
        println!("   ✗ Health check failed: {}", response.status());
    }
    println!();

    // 2. Register a Query
    println!("2. Register a Query");
    println!("   POST {}/api/queries", base_url);
    let query_request = RegisterQueryRequest {
        query_id: "sensor_query_1".to_string(),
        janusql: r#"
            SELECT ?sensor ?temp ?time
            FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z]
            WHERE {
                ?sensor <http://example.org/temperature> ?temp .
                ?sensor <http://example.org/timestamp> ?time .
            }
        "#
        .to_string(),
    };

    let response = client
        .post(format!("{}/api/queries", base_url))
        .json(&query_request)
        .send()
        .await?;

    if response.status().is_success() {
        let body: RegisterQueryResponse = response.json().await?;
        println!("   ✓ Query registered: {}", body.query_id);
        println!("   ✓ Registered at: {}", body.registered_at);
    } else {
        let error_text = response.text().await?;
        println!("   ✗ Registration failed: {}", error_text);
    }
    println!();

    // 3. Register Another Query (Live)
    println!("3. Register Another Query (Live Stream)");
    println!("   POST {}/api/queries", base_url);
    let live_query_request = RegisterQueryRequest {
        query_id: "live_sensor_query".to_string(),
        janusql: r#"
            SELECT ?sensor ?temp
            FROM LIVE SLIDING WINDOW sensors [RANGE PT10S, SLIDE PT5S]
            WHERE {
                ?sensor <http://example.org/temperature> ?temp .
                FILTER(?temp > 25.0)
            }
        "#
        .to_string(),
    };

    let response = client
        .post(format!("{}/api/queries", base_url))
        .json(&live_query_request)
        .send()
        .await?;

    if response.status().is_success() {
        let body: RegisterQueryResponse = response.json().await?;
        println!("   ✓ Live query registered: {}", body.query_id);
    } else {
        let error_text = response.text().await?;
        println!("   ✗ Registration failed: {}", error_text);
    }
    println!();

    // 4. List All Queries
    println!("4. List All Registered Queries");
    println!("   GET {}/api/queries", base_url);
    let response = client.get(format!("{}/api/queries", base_url)).send().await?;

    if response.status().is_success() {
        let body: ListQueriesResponse = response.json().await?;
        println!("   ✓ Total queries: {}", body.total);
        for query_id in &body.queries {
            println!("     - {}", query_id);
        }
    } else {
        println!("   ✗ Failed to list queries");
    }
    println!();

    // 5. Get Query Details
    println!("5. Get Query Details");
    println!("   GET {}/api/queries/sensor_query_1", base_url);
    let response = client.get(format!("{}/api/queries/sensor_query_1", base_url)).send().await?;

    if response.status().is_success() {
        let body: QueryDetailsResponse = response.json().await?;
        println!("   ✓ Query ID: {}", body.query_id);
        println!("   ✓ Registered at: {}", body.registered_at);
        println!("   ✓ Execution count: {}", body.execution_count);
        println!("   ✓ Is running: {}", body.is_running);
        println!("   ✓ Status: {}", body.status);
    } else {
        println!("   ✗ Failed to get query details");
    }
    println!();

    // 6. Start Stream Bus Replay
    println!("6. Start Stream Bus Replay");
    println!("   POST {}/api/replay/start", base_url);
    let replay_request = StartReplayRequest {
        input_file: "data/sensors.nq".to_string(),
        broker_type: "none".to_string(),
        topics: vec!["sensors".to_string()],
        rate_of_publishing: 1000,
        loop_file: false,
        add_timestamps: true,
    };

    let response = client
        .post(format!("{}/api/replay/start", base_url))
        .json(&replay_request)
        .send()
        .await?;

    if response.status().is_success() {
        let body: SuccessResponse = response.json().await?;
        println!("   ✓ {}", body.message);
    } else {
        let error_text = response.text().await?;
        println!("   ✗ Replay start failed: {}", error_text);
    }
    println!();

    // 7. Check Replay Status
    println!("7. Check Replay Status");
    println!("   GET {}/api/replay/status", base_url);
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let response = client.get(format!("{}/api/replay/status", base_url)).send().await?;

    if response.status().is_success() {
        let body: ReplayStatusResponse = response.json().await?;
        println!("   ✓ Is running: {}", body.is_running);
        println!("   ✓ Events read: {}", body.events_read);
        println!("   ✓ Events published: {}", body.events_published);
        println!("   ✓ Events stored: {}", body.events_stored);
        println!("   ✓ Events/sec: {:.2}", body.events_per_second);
        println!("   ✓ Elapsed: {:.2}s", body.elapsed_seconds);
    } else {
        println!("   ✗ Failed to get replay status");
    }
    println!();

    // 8. Start Query Execution
    println!("8. Start Query Execution");
    println!("   POST {}/api/queries/sensor_query_1/start", base_url);
    let response = client
        .post(format!("{}/api/queries/sensor_query_1/start", base_url))
        .send()
        .await?;

    if response.status().is_success() {
        let body: SuccessResponse = response.json().await?;
        println!("   ✓ {}", body.message);
    } else {
        let error_text = response.text().await?;
        println!("   ✗ Query start failed: {}", error_text);
    }
    println!();

    // 9. WebSocket Connection for Streaming Results
    println!("9. Connect to WebSocket for Query Results");
    println!("   WS ws://127.0.0.1:8080/api/queries/sensor_query_1/results");
    println!("   (Streaming results for 5 seconds...)");

    let ws_url = "ws://127.0.0.1:8080/api/queries/sensor_query_1/results";

    match tokio_tungstenite::connect_async(ws_url).await {
        Ok((mut ws_stream, _)) => {
            use futures_util::StreamExt;
            use tokio_tungstenite::tungstenite::Message;

            let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(5));
            tokio::pin!(timeout);

            let mut result_count = 0;

            loop {
                tokio::select! {
                    msg = ws_stream.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                match serde_json::from_str::<QueryResultMessage>(&text) {
                                    Ok(result) => {
                                        result_count += 1;
                                        println!("   ✓ Result #{}: source={}, timestamp={}, bindings={}",
                                            result_count,
                                            result.source,
                                            result.timestamp,
                                            result.bindings.len()
                                        );
                                        if !result.bindings.is_empty() {
                                            println!("     First binding: {:?}", result.bindings[0]);
                                        }
                                    }
                                    Err(e) => {
                                        println!("   ✗ Failed to parse result: {}", e);
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                println!("   ✓ WebSocket closed by server");
                                break;
                            }
                            Some(Err(e)) => {
                                println!("   ✗ WebSocket error: {}", e);
                                break;
                            }
                            None => {
                                println!("   ✓ WebSocket stream ended");
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = &mut timeout => {
                        println!("   ✓ Timeout reached, closing WebSocket");
                        break;
                    }
                }
            }

            println!("   ✓ Received {} results", result_count);
        }
        Err(e) => {
            println!("   ✗ WebSocket connection failed: {}", e);
            println!("   (This is expected if the query has no results yet)");
        }
    }
    println!();

    // 10. Stop Query
    println!("10. Stop Query Execution");
    println!("   DELETE {}/api/queries/sensor_query_1", base_url);
    let response = client.delete(format!("{}/api/queries/sensor_query_1", base_url)).send().await?;

    if response.status().is_success() {
        let body: SuccessResponse = response.json().await?;
        println!("   ✓ {}", body.message);
    } else {
        let error_text = response.text().await?;
        println!("   ✗ Query stop failed: {}", error_text);
    }
    println!();

    // 11. Stop Replay
    println!("11. Stop Stream Bus Replay");
    println!("   POST {}/api/replay/stop", base_url);
    let response = client.post(format!("{}/api/replay/stop", base_url)).send().await?;

    if response.status().is_success() {
        let body: SuccessResponse = response.json().await?;
        println!("   ✓ {}", body.message);
    } else {
        let error_text = response.text().await?;
        println!("   ✗ Replay stop failed: {}", error_text);
    }
    println!();

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║              Example Completed Successfully                    ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    Ok(())
}
