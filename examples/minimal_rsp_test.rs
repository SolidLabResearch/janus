//! Minimal test to verify rsp-rs 0.3.1 graph name fix
//!
//! This uses very small time windows to ensure windows close quickly
//! and we can verify that results are now being received.

use janus::core::RDFEvent;
use janus::stream::live_stream_processing::LiveStreamProcessing;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== RSP-RS 0.3.1 Integration Test ===\n");

    // Use small windows: 1 second range, 200ms step
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?p ?o
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 200]
        WHERE {
            WINDOW ex:w1 { ?s ?p ?o }
        }
    "#;

    println!("Query: RANGE 1000ms, STEP 200ms\n");

    let mut processor = LiveStreamProcessing::new(query.to_string())?;
    processor.register_stream("http://example.org/stream1")?;
    processor.start_processing()?;

    println!("Adding 11 events (t=0 to t=1000ms)...");
    for i in 0..11 {
        let timestamp = (i * 100) as u64;
        let event = RDFEvent::new(
            timestamp,
            &format!("http://example.org/subject{}", i),
            "http://example.org/predicate",
            &format!("object{}", i),
            "",
        );
        processor.add_event("http://example.org/stream1", event)?;
    }
    println!("✓ Events added\n");

    println!("Closing stream at t=5000ms...");
    processor.close_stream("http://example.org/stream1", 5000)?;
    println!("✓ Stream closed\n");

    println!("Waiting 1 second for processing...");
    thread::sleep(Duration::from_secs(1));
    println!();

    println!("=== Collecting Results ===\n");

    let mut count = 0;
    for _ in 0..50 {
        match processor.try_receive_result() {
            Ok(Some(result)) => {
                count += 1;
                if count <= 3 {
                    println!(
                        "Result {}: t={} to t={}",
                        count, result.timestamp_from, result.timestamp_to
                    );
                    println!("  Bindings: {}", result.bindings);
                    println!();
                }
            }
            Ok(None) => break,
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
        }
    }

    if count > 3 {
        println!("... ({} more results)\n", count - 3);
    }

    println!("=== RESULTS ===");
    println!("Total results received: {}\n", count);

    if count == 0 {
        println!("❌ FAILED: No results received");
        println!("The graph name fix in rsp-rs 0.3.1 may not be working.");
        std::process::exit(1);
    } else {
        println!("✅ SUCCESS: Integration working!");
        println!("The rsp-rs 0.3.1 fix is confirmed working with Janus.");
    }

    Ok(())
}
