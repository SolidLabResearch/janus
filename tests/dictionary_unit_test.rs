use janus::core::{Event, RDFEvent};
use janus::execution::rdf_conversion::rdf_event_to_quad;
use janus::storage::indexing::dictionary::Dictionary;
use oxigraph::model::Term;

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

#[test]
fn test_object_term_encoding_roundtrip_special_characters() {
    let cases = [
        (r#"quoted "value""#, None),
        (r#"escaped \"quote\""#, None),
        ("http://example.org/path?x=1&y=two", None),
        ("punctuation: !@#$%^&*()[]{};:,./?", None),
        ("line one\nline two", Some("http://www.w3.org/2001/XMLSchema#string")),
    ];

    for (value, datatype) in cases {
        let encoded = Dictionary::encode_object_term(value, true, datatype);
        let (decoded_value, is_literal, decoded_datatype) =
            Dictionary::decode_object_term(&encoded);

        assert_eq!(decoded_value, value);
        assert!(is_literal);
        assert_eq!(decoded_datatype.as_deref(), datatype);
    }
}

#[test]
fn test_object_term_encoding_preserves_identity_variants() {
    let integer = Dictionary::encode_object_term(
        "23",
        true,
        Some("http://www.w3.org/2001/XMLSchema#integer"),
    );
    let decimal = Dictionary::encode_object_term(
        "23",
        true,
        Some("http://www.w3.org/2001/XMLSchema#decimal"),
    );
    let plain = Dictionary::encode_object_term("23", true, None);
    let iri = Dictionary::encode_object_term("urn:23", false, None);

    assert_ne!(integer, decimal);
    assert_ne!(integer, plain);
    assert_ne!(decimal, plain);
    assert_ne!(plain, iri);

    assert_eq!(
        Dictionary::decode_object_term(&integer),
        (
            "23".to_string(),
            true,
            Some("http://www.w3.org/2001/XMLSchema#integer".to_string())
        )
    );
    assert_eq!(
        Dictionary::decode_object_term(&decimal),
        (
            "23".to_string(),
            true,
            Some("http://www.w3.org/2001/XMLSchema#decimal".to_string())
        )
    );
    assert_eq!(Dictionary::decode_object_term(&plain), ("23".to_string(), true, None));
    assert_eq!(Dictionary::decode_object_term(&iri), ("urn:23".to_string(), false, None));
}

#[test]
fn test_legacy_unprefixed_dictionary_values_remain_compatible() {
    assert_eq!(
        Dictionary::decode_object_term("http://example.org/x"),
        ("http://example.org/x".to_string(), false, None)
    );
    assert_eq!(Dictionary::decode_object_term("23"), ("23".to_string(), true, None));

    let legacy_event =
        RDFEvent::new_literal_object(1, "http://example.org/s", "http://example.org/p", "23", "");
    let quad = rdf_event_to_quad(&legacy_event).expect("legacy literal should convert to quad");

    let Term::Literal(literal) = quad.object else {
        panic!("expected literal object");
    };
    assert_eq!(literal.value(), "23");
    assert_eq!(literal.datatype().as_str(), "http://www.w3.org/2001/XMLSchema#integer");
}
