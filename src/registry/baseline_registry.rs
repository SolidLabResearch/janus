use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

/// Baseline identifier used by the registry.
pub type BaselineId = String;

/// One stored SELECT-result snapshot for a baseline at a specific evaluation timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineSnapshot {
    pub baseline_id: BaselineId,
    pub valid_at: u64,
    pub source_window: String,
    pub window_start: u64,
    pub window_end: u64,
    pub variables: Vec<String>,
    pub rows: Vec<HashMap<String, String>>,
}

/// Registry of versioned baseline snapshots keyed by baseline id and evaluation timestamp.
#[derive(Debug, Clone, Default)]
pub struct BaselineRegistry {
    snapshots: Arc<RwLock<HashMap<BaselineId, BTreeMap<u64, BaselineSnapshot>>>>,
}

impl BaselineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_snapshot(&self, snapshot: BaselineSnapshot) {
        let mut snapshots = self.snapshots.write().unwrap();
        snapshots
            .entry(snapshot.baseline_id.clone())
            .or_default()
            .insert(snapshot.valid_at, snapshot);
    }

    pub fn get_snapshot(&self, baseline_id: &str, valid_at: u64) -> Option<BaselineSnapshot> {
        let snapshots = self.snapshots.read().unwrap();
        snapshots.get(baseline_id)?.get(&valid_at).cloned()
    }

    pub fn get_latest_snapshot(&self, baseline_id: &str) -> Option<BaselineSnapshot> {
        let snapshots = self.snapshots.read().unwrap();
        snapshots
            .get(baseline_id)?
            .iter()
            .next_back()
            .map(|(_, snapshot)| snapshot.clone())
    }

    pub fn get_snapshot_at_or_before(
        &self,
        baseline_id: &str,
        valid_at: u64,
    ) -> Option<BaselineSnapshot> {
        let snapshots = self.snapshots.read().unwrap();
        snapshots
            .get(baseline_id)?
            .range(..=valid_at)
            .next_back()
            .map(|(_, snapshot)| snapshot.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{BaselineRegistry, BaselineSnapshot};
    use std::collections::HashMap;

    fn sample_snapshot(valid_at: u64, avg: &str) -> BaselineSnapshot {
        BaselineSnapshot {
            baseline_id: "http://example.org/yesterdayBaseline".to_string(),
            valid_at,
            source_window: "http://example.org/sameMinuteYesterday".to_string(),
            window_start: 86_340_000,
            window_end: 86_400_000,
            variables: vec!["?sensor".to_string(), "?yesterdayAvgValue".to_string()],
            rows: vec![HashMap::from([
                ("sensor".to_string(), "http://example.org/sensor1".to_string()),
                ("yesterdayAvgValue".to_string(), avg.to_string()),
            ])],
        }
    }

    #[test]
    fn insert_and_retrieve_exact_snapshot() {
        let registry = BaselineRegistry::new();
        let snapshot = sample_snapshot(172_800_000, "15");
        registry.insert_snapshot(snapshot.clone());

        assert_eq!(
            registry.get_snapshot("http://example.org/yesterdayBaseline", 172_800_000),
            Some(snapshot)
        );
    }

    #[test]
    fn latest_snapshot_retrieval_returns_most_recent_version() {
        let registry = BaselineRegistry::new();
        registry.insert_snapshot(sample_snapshot(172_800_000, "15"));
        let latest = sample_snapshot(172_860_000, "40");
        registry.insert_snapshot(latest.clone());

        assert_eq!(
            registry.get_latest_snapshot("http://example.org/yesterdayBaseline"),
            Some(latest)
        );
    }

    #[test]
    fn exact_valid_at_can_be_replaced() {
        let registry = BaselineRegistry::new();
        registry.insert_snapshot(sample_snapshot(172_800_000, "15"));
        let replacement = sample_snapshot(172_800_000, "18");
        registry.insert_snapshot(replacement.clone());

        assert_eq!(
            registry.get_snapshot("http://example.org/yesterdayBaseline", 172_800_000),
            Some(replacement)
        );
    }

    #[test]
    fn versioned_snapshots_remain_independently_addressable() {
        let registry = BaselineRegistry::new();
        let first = sample_snapshot(172_800_000, "15");
        let second = sample_snapshot(172_860_000, "40");
        registry.insert_snapshot(first.clone());
        registry.insert_snapshot(second.clone());

        assert_eq!(
            registry.get_snapshot("http://example.org/yesterdayBaseline", 172_800_000),
            Some(first)
        );
        assert_eq!(
            registry.get_snapshot("http://example.org/yesterdayBaseline", 172_860_000),
            Some(second)
        );
    }

    #[test]
    fn snapshots_store_binding_rows_not_live_stream_events() {
        let registry = BaselineRegistry::new();
        let snapshot = sample_snapshot(172_800_000, "15");
        registry.insert_snapshot(snapshot.clone());

        let stored = registry
            .get_snapshot("http://example.org/yesterdayBaseline", 172_800_000)
            .expect("snapshot should exist");
        assert_eq!(stored.rows.len(), 1);
        assert_eq!(stored.rows[0]["sensor"], "http://example.org/sensor1");
        assert_eq!(stored.rows[0]["yesterdayAvgValue"], "15");
    }
}
