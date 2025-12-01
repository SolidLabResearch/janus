use janus::core::RDFEvent;
use janus::stream::live_stream_processing::LiveStreamProcessing;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("Starting debug_live reproduction...");

    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?sensor ?temp
        FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
        WHERE {
            WINDOW ex:liveWindow {
                ?sensor ex:temperature ?temp .
            }
        }
    "#;

    let mut processor =
        LiveStreamProcessing::new(query.to_string()).expect("Failed to create processor");

    let stream_uri = "http://example.org/sensorStream";
    processor.register_stream(stream_uri).expect("Failed to register stream");

    processor.start_processing().expect("Failed to start processing");

    println!("Processor started. Feeding events...");

    let start_time = 60_000_000_000;

    // Feed 20 events over 10 seconds (one every 500ms)
    for i in 0..20 {
        let timestamp = start_time + (i * 500);

        let event = RDFEvent::new(
            timestamp,
            "http://example.org/sensor1",
            "http://example.org/temperature",
            "25.0",
            "http://example.org/liveWindow", // Named graph matching the window
        );

        println!("Adding event #{} at timestamp {}", i, timestamp);
        processor.add_event(stream_uri, event).expect("Failed to add event");

        // Try to receive results
        match processor.try_receive_result() {
            Ok(Some(result)) => {
                println!("!!! RECEIVED RESULT !!!");
                println!("Bindings: {:?}", result.bindings);
            }
            Ok(None) => {
                // println!("No result yet");
            }
            Err(e) => println!("Error receiving result: {}", e),
        }

        thread::sleep(Duration::from_millis(100));
    }

    println!("Finished feeding events.");
}
