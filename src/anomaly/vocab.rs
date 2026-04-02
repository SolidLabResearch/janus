//! Materialised-statistic predicate vocabulary for the Janus two-pass executor.
//!
//! All predicates live under the `https://janus.rs/stat#` namespace.  Use the
//! constructor functions (e.g. [`hist_mean`]) wherever an owned [`NamedNode`] is
//! needed, and the `_IRI` string constants wherever the raw IRI string is needed
//! (e.g. inside SPARQL query templates).
//!
//! # Rules
//!
//! - **Never** hardcode `"https://janus.rs/stat#..."` strings anywhere else in
//!   the codebase.  Always import from this module.
//! - `histSlope` and `liveSlope` are defined here for completeness but are
//!   **not yet materialised** by `materialise_stats`.  See the comment in
//!   `historical_executor::materialise_stats` for the next steps.

use oxigraph::model::NamedNode;

// ---------------------------------------------------------------------------
// IRI string constants — use in SPARQL query templates
// ---------------------------------------------------------------------------

/// `https://janus.rs/stat#histMean`
pub const HIST_MEAN_IRI: &str = "https://janus.rs/stat#histMean";

/// `https://janus.rs/stat#histStdDev`
pub const HIST_STD_DEV_IRI: &str = "https://janus.rs/stat#histStdDev";

/// `https://janus.rs/stat#histSlope` — vocab reserved; not yet materialised.
pub const HIST_SLOPE_IRI: &str = "https://janus.rs/stat#histSlope";

/// `https://janus.rs/stat#liveMean`
pub const LIVE_MEAN_IRI: &str = "https://janus.rs/stat#liveMean";

/// `https://janus.rs/stat#liveStdDev`
pub const LIVE_STD_DEV_IRI: &str = "https://janus.rs/stat#liveStdDev";

/// `https://janus.rs/stat#liveSlope` — vocab reserved; not yet materialised.
pub const LIVE_SLOPE_IRI: &str = "https://janus.rs/stat#liveSlope";

// ---------------------------------------------------------------------------
// NamedNode constructors — use when inserting triples into a Store
// ---------------------------------------------------------------------------

/// Returns the [`NamedNode`] for `janus:histMean`.
pub fn hist_mean() -> NamedNode {
    NamedNode::new(HIST_MEAN_IRI).unwrap()
}

/// Returns the [`NamedNode`] for `janus:histStdDev`.
pub fn hist_std_dev() -> NamedNode {
    NamedNode::new(HIST_STD_DEV_IRI).unwrap()
}

/// Returns the [`NamedNode`] for `janus:histSlope`.
///
/// **Not yet materialised** — reserved for a future task.
pub fn hist_slope() -> NamedNode {
    NamedNode::new(HIST_SLOPE_IRI).unwrap()
}

/// Returns the [`NamedNode`] for `janus:liveMean`.
pub fn live_mean() -> NamedNode {
    NamedNode::new(LIVE_MEAN_IRI).unwrap()
}

/// Returns the [`NamedNode`] for `janus:liveStdDev`.
pub fn live_std_dev() -> NamedNode {
    NamedNode::new(LIVE_STD_DEV_IRI).unwrap()
}

/// Returns the [`NamedNode`] for `janus:liveSlope`.
///
/// **Not yet materialised** — reserved for a future task.
pub fn live_slope() -> NamedNode {
    NamedNode::new(LIVE_SLOPE_IRI).unwrap()
}
