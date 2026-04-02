//! Modular anomaly detection extension function system.
//!
//! Provides pure math helpers, rule trait + implementations, a function name
//! registry, an Oxigraph `SparqlEvaluator` builder that wires all rules as
//! SPARQL extension functions under the `https://janus.rs/fn#` namespace, and
//! a materialised-statistic predicate vocabulary under `https://janus.rs/stat#`.

pub mod math;
pub mod registry;
pub mod rules;
pub mod query_options;
pub mod vocab;

// Re-export vocab so callers can use `janus::anomaly::HIST_MEAN_IRI` etc.
pub use vocab::{
    hist_mean, hist_slope, hist_std_dev, live_mean, live_slope, live_std_dev, HIST_MEAN_IRI,
    HIST_SLOPE_IRI, HIST_STD_DEV_IRI, LIVE_MEAN_IRI, LIVE_SLOPE_IRI, LIVE_STD_DEV_IRI,
};
