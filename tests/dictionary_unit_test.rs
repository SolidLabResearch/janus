use janus::core::{Event, RDFEvent};
use janus::storage::indexing::dictionary::Dictionary;

#[test]
fn test_dictionary_encoding_decoding() {
    let mut dict = Dictionary::new();

    // Encode some RDF terms
    let subject_id = dict.encode("http://example.org/person/Alice");
    let predicate_id = dict.encode("http://example.org/knows");
    let object_id = dict.encode("http://example.org/person/Bob");
    let graph_id = dict.encode("http://example.org/graph1");

    println!("Encoded IDs:");
    println!("Subject: http://example.org/person/Alice -> {}", subject_id);
    println!("Predicate: http://example.org/knows -> {}", predicate_id);
    println!("Object: http://example.org/person/Bob -> {}", object_id);
    println!("Graph: http://example.org/graph1 -> {}", graph_id);

    // Create an event
    let event = Event {
        timestamp: 1_234_567_890,
        subject: subject_id,
        predicate: predicate_id,
        object: object_id,
        graph: graph_id,
    };

    // Decode the event
    let decoded = dict.decode_graph(&event);
    println!("\nDecoded event: {}", decoded);

    // Verify individual decodings
    assert_eq!(dict.decode(subject_id), Some("http://example.org/person/Alice"));
    assert_eq!(dict.decode(predicate_id), Some("http://example.org/knows"));
    assert_eq!(dict.decode(object_id), Some("http://example.org/person/Bob"));
    assert_eq!(dict.decode(graph_id), Some("http://example.org/graph1"));

    // Test that the decoded string contains the expected format
    assert!(decoded.contains("http://example.org/person/Alice"));
    assert!(decoded.contains("http://example.org/knows"));
    assert!(decoded.contains("http://example.org/person/Bob"));
    assert!(decoded.contains("http://example.org/graph1"));
    assert!(decoded.contains("1234567890"));
}

#[test]
fn test_clean_rdf_api() {
    let mut dict = Dictionary::new();

    // Test the clean API - user provides URIs directly
    let rdf_event = RDFEvent::new(
        1_234_567_890,
        "http://example.org/person/Alice",
        "http://example.org/knows",
        "http://example.org/person/Bob",
        "http://example.org/graph1",
    );

    // Encoding happens internally
    let encoded_event = rdf_event.encode(&mut dict);

    // Decoding happens internally
    let decoded_event = encoded_event.decode(&dict);

    // Verify the round-trip works
    assert_eq!(decoded_event.subject, "http://example.org/person/Alice");
    assert_eq!(decoded_event.predicate, "http://example.org/knows");
    assert_eq!(decoded_event.object, "http://example.org/person/Bob");
    assert_eq!(decoded_event.graph, "http://example.org/graph1");
    assert_eq!(decoded_event.timestamp, 1_234_567_890);

    println!("Clean API test passed!");
    println!(
        "Original: {} {} {} in {} at timestamp {}",
        rdf_event.subject,
        rdf_event.predicate,
        rdf_event.object,
        rdf_event.graph,
        rdf_event.timestamp
    );
    println!(
        "Encoded IDs: {} {} {} {} at timestamp {}",
        encoded_event.subject,
        encoded_event.predicate,
        encoded_event.object,
        encoded_event.graph,
        encoded_event.timestamp
    );
    println!(
        "Decoded: {} {} {} in {} at timestamp {}",
        decoded_event.subject,
        decoded_event.predicate,
        decoded_event.object,
        decoded_event.graph,
        decoded_event.timestamp
    );
}
