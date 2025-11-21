use rsp_rs::QuadContainer;
use std::error::Error;

pub trait SparqlEngine {
    type EngineError: Error + 'static;

    fn execute_query(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<String>, Self::EngineError>;
}

pub struct QueryProcessor<E: SparqlEngine> {
    engine: E,
}

impl<E: SparqlEngine> QueryProcessor<E> {
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    pub fn process_query(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<String>, E::EngineError> {
        self.engine.execute_query(query, container)
    }
}
