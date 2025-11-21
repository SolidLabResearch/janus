use crate::querying::kolibrie_adapter::KolibrieAdapter;
use crate::querying::oxigraph_adapter::OxigraphAdapter;
use crate::querying::query_processing::QueryProcessor;
use oxigraph::model::{Literal, NamedNode, Quad};
use rsp_rs::QuadContainer;
use std::collections::HashSet;

fn main() {
    let query = "SELECT ?s WHERE { ?s ?p ?o }";

    // Create sample quads
    let mut quads = HashSet::new();

    // Add sample quad to the set
    // Example: <http://example.org/subject1> <http://example.org/predicate1> "Object1" in default graph
    let subject = NamedNode::new("http://example.org/subject1").unwrap();
    let predicate = NamedNode::new("http://example.org/predicate1").unwrap();
    let object = Literal::new_simple_literal("Object1");
    let quad = Quad::new(subject, predicate, object, oxigraph::model::GraphName::DefaultGraph);
    quads.insert(quad);

    // Create a QuadContainer with the quads and a timestamp
    let timestamp = 1000; // milliseconds since epoch
    let container = QuadContainer::new(quads, timestamp);

    let oxigraph_adapter = OxigraphAdapter::new();
    let kolibrie_adapter = KolibrieAdapter::new();

    let query_processor_oxigraph = QueryProcessor::new(oxigraph_adapter);
    let query_processor_kolibrie = QueryProcessor::new(kolibrie_adapter);

    // Pass the container to the query processor
    match query_processor_oxigraph.process_query(query, &container) {
        Ok(results) => println!("Oxigraph results: {:?}", results),
        Err(e) => eprintln!("Oxigraph error: {}", e),
    }

    match query_processor_kolibrie.process_query(query, &container) {
        Ok(results) => println!("Kolibrie results: {:?}", results),
        Err(e) => eprintln!("Kolibrie error: {}", e),
    }
}
