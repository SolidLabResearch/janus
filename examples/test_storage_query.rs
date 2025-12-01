use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;

fn main() {
    let config = StreamingConfig {
        segment_base_path: "./data/storage".to_string(),
        max_batch_bytes: 10485760,
        max_batch_age_seconds: 5,
        max_batch_events: 100_000,
        sparse_interval: 1000,
        entries_per_index_block: 1024,
    };

    let storage = StreamingSegmentedStorage::new(config).expect("Failed to load storage");
    
    let events = storage.query(0, u64::MAX).expect("Query failed");
    
    println!("Total events in storage: {}", events.len());
    
    if events.len() > 0 {
        println!("\nFirst 3 events:");
        for (i, event) in events.iter().take(3).enumerate() {
            println!("Event {}: timestamp={}, subject={}, predicate={}, object={}, graph={}", 
                i+1, event.timestamp, event.subject, event.predicate, event.object, event.graph);
        }
        
        let dict = storage.get_dictionary().read().unwrap();
        println!("\nDecoded first 3 events:");
        for (i, e) in events.iter().take(3).enumerate() {
            println!("Event {}:", i+1);
            println!("  subject: {:?}", dict.decode(e.subject));
            println!("  predicate: {:?}", dict.decode(e.predicate));
            println!("  object: {:?}", dict.decode(e.object));
            println!("  graph: {:?}", dict.decode(e.graph));
        }
    }
}
