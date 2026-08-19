use crate::core::RDFEvent;
use crate::execution::rdf_event_to_quad;
use crate::querying::oxigraph_adapter::OxigraphAdapter;
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
