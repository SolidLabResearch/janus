use crate::core::RDFEvent;
use crate::querying::oxigraph_adapter::OxigraphAdapter;
use oxigraph::model::{GraphName, NamedNode, Quad, Term};
use rsp_rs::QuadContainer;
use std::collections::{HashMap, HashSet};

pub type ExternalBindings = Vec<HashMap<String, String>>;

pub trait ExternalHistoricalAdapter {
    fn name(&self) -> &'static str;

    fn execute_bindings_query(
        &self,
        query: &str,
        events: &[RDFEvent],
    ) -> Result<ExternalBindings, Box<dyn std::error::Error>>;
}

pub struct OxigraphExternalAdapter {
    adapter: OxigraphAdapter,
}

impl OxigraphExternalAdapter {
    pub fn new() -> Self {
        Self { adapter: OxigraphAdapter::new() }
    }
}

impl Default for OxigraphExternalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalHistoricalAdapter for OxigraphExternalAdapter {
    fn name(&self) -> &'static str {
        "oxigraph"
    }

    fn execute_bindings_query(
        &self,
        query: &str,
        events: &[RDFEvent],
    ) -> Result<ExternalBindings, Box<dyn std::error::Error>> {
        let mut quads = HashSet::with_capacity(events.len());
        for event in events {
            quads.insert(rdf_event_to_quad(event)?);
        }
        let container = QuadContainer::new(
            quads,
            events
                .last()
                .map_or(0_i64, |event| i64::try_from(event.timestamp).unwrap_or(i64::MAX)),
        );
        Ok(self.adapter.execute_query_bindings(query, &container)?)
    }
}

pub struct JenaExternalAdapterStub;

impl ExternalHistoricalAdapter for JenaExternalAdapterStub {
    fn name(&self) -> &'static str {
        "jena_stub"
    }

    fn execute_bindings_query(
        &self,
        _query: &str,
        _events: &[RDFEvent],
    ) -> Result<ExternalBindings, Box<dyn std::error::Error>> {
        Err("Apache Jena adapter is not implemented yet".into())
    }
}

fn rdf_event_to_quad(event: &RDFEvent) -> Result<Quad, Box<dyn std::error::Error>> {
    let subject = NamedNode::new(event.subject.as_str())?;
    let predicate = NamedNode::new(event.predicate.as_str())?;
    let object = if event.object.starts_with("http://") || event.object.starts_with("https://") {
        Term::NamedNode(NamedNode::new(event.object.as_str())?)
    } else {
        Term::Literal(event.object.as_str().into())
    };
    let graph = if event.graph.is_empty() {
        GraphName::DefaultGraph
    } else {
        GraphName::NamedNode(NamedNode::new(event.graph.as_str())?)
    };
    Ok(Quad::new(subject, predicate, object, graph))
}
