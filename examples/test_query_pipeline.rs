use janus::{
    api::janus_api::JanusApi, parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry, storage::segmented_storage::StreamingSegmentedStorage,
    storage::util::StreamingConfig,
};
use std::sync::Arc;

fn main() {
    let janusql = r#"
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1000000000000 END 2000000000000]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
"#
    .trim();

    println!("Testing query pipeline...\n");
    println!("Query:\n{}\n", janusql);

    let config = StreamingConfig {
        segment_base_path: "./data/storage".to_string(),
        max_batch_bytes: 10485760,
        max_batch_age_seconds: 5,
        max_batch_events: 100_000,
        sparse_interval: 1000,
        entries_per_index_block: 1024,
    };

    let storage = Arc::new(StreamingSegmentedStorage::new(config).expect("Failed to load storage"));

    let events = storage.query(0, u64::MAX).expect("Storage query failed");
    println!("Storage has {} events", events.len());

    if events.len() > 0 {
        let dict = storage.get_dictionary().read().unwrap();
        println!("\nFirst 3 events decoded:");
        for (i, e) in events.iter().take(3).enumerate() {
            println!("Event {}:", i + 1);
            println!("  subject: {:?}", dict.decode(e.subject));
            println!("  predicate: {:?}", dict.decode(e.predicate));
            println!("  object: {:?}", dict.decode(e.object));
            println!("  graph: {:?}", dict.decode(e.graph));
            println!("  timestamp: {}", e.timestamp);
        }
    }

    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, storage).expect("Failed to create API");

    println!("\nRegistering query...");
    let query_id = "test_query".to_string();
    match api.register_query(query_id.clone(), janusql) {
        Ok(_) => println!("✓ Query registered"),
        Err(e) => {
            println!("✗ Failed to register: {}", e);
            return;
        }
    }

    println!("Starting query...");
    let handle = match api.start_query(&query_id) {
        Ok(handle) => {
            println!("✓ Query started");
            handle
        }
        Err(e) => {
            println!("✗ Failed to start: {}", e);
            return;
        }
    };

    println!("\nWaiting for results (5 seconds)...");
    let start = std::time::Instant::now();
    let mut result_count = 0;

    while start.elapsed().as_secs() < 5 {
        if let Some(result) = handle.try_receive() {
            result_count += 1;
            println!("\nResult {}:", result_count);
            println!("  Source: {:?}", result.source);
            println!("  Timestamp: {}", result.timestamp);
            println!("  Bindings ({} items):", result.bindings.len());
            for (i, binding) in result.bindings.iter().take(3).enumerate() {
                println!("    {}: {:?}", i + 1, binding);
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    println!("\nTotal: {} results received", result_count);
}
