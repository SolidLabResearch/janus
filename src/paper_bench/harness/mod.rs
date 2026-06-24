pub mod coordination;
pub mod data_gen;
pub mod helpers;
pub mod io;
pub mod metrics;
pub mod scaling;
pub mod sustained;
pub mod system_info;
pub mod types;

pub use coordination::*;
pub use data_gen::*;
pub use helpers::*;
pub use io::*;
pub use metrics::*;
pub use scaling::*;
pub use sustained::*;
pub use system_info::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RDFEvent;
    use crate::paper_bench::external::OxigraphExternalAdapter;

    fn test_metadata() -> ReproMetadata {
        ReproMetadata {
            git_commit_sha: "test".to_string(),
            branch: "test".to_string(),
            rustc_version: "test".to_string(),
            os: "test".to_string(),
            cpu_model: "test".to_string(),
            ram_bytes: Some(1),
            benchmark_command: "test".to_string(),
            timestamp_unix_ms: 0,
        }
    }

    #[test]
    fn tiny_h1_equivalence_matches_between_janus_and_oxigraph() {
        let metadata = test_metadata();
        let adapter = OxigraphExternalAdapter::new();
        let workload = prepare_coordination_workload(1, 1).expect("workload should build");
        let pair = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload),
            debug_output_dir: None,
        })
        .expect("pair should run");

        assert_eq!(pair.unified.result_count, 1);
        assert_eq!(pair.decomposed.result_count, 1);
        assert_eq!(pair.unified.result_hash, pair.decomposed.result_hash);
        assert_eq!(pair.unified.equivalent_to_baseline, Some(true));
    }

    #[test]
    fn test_deterministic_hybrid_dependency() {
        let metadata = test_metadata();
        let adapter = OxigraphExternalAdapter::new();

        // 1. historical=1, live=1 -> hybrid_result_count=1
        let workload_1_1 = prepare_coordination_workload(1, 1).expect("workload should build");
        let pair_1_1 = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_1_1),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_1_1.unified.hybrid_result_count, 1);
        assert_eq!(pair_1_1.decomposed.hybrid_result_count, 1);

        // 2. historical=1, live=0 -> hybrid_result_count=0
        let workload_1_0 = prepare_coordination_workload(1, 0).expect("workload should build");
        let pair_1_0 = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 0,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_1_0),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_1_0.unified.hybrid_result_count, 0);
        assert_eq!(pair_1_0.decomposed.hybrid_result_count, 0);

        // 3. historical=0, live=1 -> hybrid_result_count=0
        let workload_0_1 = prepare_coordination_workload(0, 1).expect("workload should build");
        let pair_0_1 = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 0,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_0_1),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_0_1.unified.hybrid_result_count, 0);
        assert_eq!(pair_0_1.decomposed.hybrid_result_count, 0);

        // 4. historical=1, live=1 but mismatched sensor/join key -> hybrid_result_count=0
        let mut workload_mismatched =
            prepare_coordination_workload(1, 1).expect("workload should build");
        // Change live event sensor to mismatched
        if !workload_mismatched.live_events.is_empty() {
            workload_mismatched.live_events[0] = RDFEvent::new(
                workload_mismatched.live_events[0].timestamp,
                "http://example.org/junction/mismatched_sensor",
                TRAFFIC_PREDICATE,
                &workload_mismatched.live_events[0].object,
                GRAPH_URI,
            );
        }
        let pair_mismatched = run_coordination_pair(CoordinationRunConfig {
            mode: ExecutionMode::Warm,
            run_index: 0,
            is_warmup: false,
            historical_events: 1,
            live_events: 1,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_mismatched),
            debug_output_dir: None,
        })
        .expect("pair should run");
        assert_eq!(pair_mismatched.unified.hybrid_result_count, 0);
        assert_eq!(pair_mismatched.decomposed.hybrid_result_count, 0);
    }

    #[test]
    fn test_sustained_expected_completed_windows_formula() {
        // window_size=120s, slide=60s, duration=240s
        let duration = 240;
        let window_size = 120;
        let slide = 60;
        let first_start = window_size - slide - 1;
        let last_event = duration - 1;
        let close_ts = last_event + 20;
        let expected = if close_ts >= first_start {
            1 + (close_ts - first_start) / slide
        } else {
            0
        };
        assert_eq!(expected, 4);
    }

    #[test]
    fn test_sustained_wall_clock_config_helpers() {
        assert!((sustained_event_interval_ms(4) - 250.0).abs() < f64::EPSILON);
        assert_eq!(sustained_expected_wall_clock_duration_ms(20), 20_000);
    }

    #[test]
    fn test_sustained_hybrid_dependency_and_equivalence() {
        let metadata = test_metadata();
        let adapter = OxigraphExternalAdapter::new();

        // 1. Janus and decomposed baseline match per-window result hashes on a tiny workload
        let workload_1_1 =
            prepare_sustained_workload(100, 240, 1, 120, 60).expect("workload should build");
        let pair_1_1 = run_sustained_pair(SustainedRunConfig {
            mode: ExecutionMode::Warm,
            time_mode: TimeMode::Virtual,
            run_index: 0,
            is_warmup: false,
            historical_events: 100,
            live_duration_seconds: 240,
            event_rate_hz: 1,
            event_interval_ms: sustained_event_interval_ms(1),
            expected_wall_clock_duration_ms: sustained_expected_wall_clock_duration_ms(240),
            window_size_seconds: 120,
            window_slide_seconds: 60,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_1_1),
            debug_output_dir: None,
        })
        .expect("sustained pair should run");

        assert_eq!(pair_1_1.unified.expected_completed_windows, 2);
        assert_eq!(pair_1_1.unified.completed_windows_total, 4);
        assert_eq!(pair_1_1.unified.completed_windows_in_horizon, 2);
        assert_eq!(pair_1_1.unified.flush_windows, 2);
        assert_eq!(pair_1_1.unified.time_mode, "virtual");
        assert!(pair_1_1.unified.uses_virtual_event_time);
        assert!((pair_1_1.unified.event_interval_ms - 1000.0).abs() < f64::EPSILON);
        assert_eq!(pair_1_1.unified.expected_wall_clock_duration_ms, 240_000);
        assert_eq!(pair_1_1.decomposed.completed_windows_total, 4);
        assert_eq!(pair_1_1.decomposed.completed_windows_in_horizon, 2);
        assert_eq!(pair_1_1.decomposed.flush_windows, 2);
        assert!(pair_1_1.decomposed.uses_virtual_event_time);
        assert_eq!(pair_1_1.unified.equivalent_to_baseline, Some(true));
        assert!(pair_1_1.unified.hybrid_result_count_total > 0);

        // 2. Removing historical input yields zero hybrid results
        let workload_no_hist =
            prepare_sustained_workload(0, 240, 1, 120, 60).expect("workload should build");
        let pair_no_hist = run_sustained_pair(SustainedRunConfig {
            mode: ExecutionMode::Warm,
            time_mode: TimeMode::Virtual,
            run_index: 0,
            is_warmup: false,
            historical_events: 0,
            live_duration_seconds: 240,
            event_rate_hz: 1,
            event_interval_ms: sustained_event_interval_ms(1),
            expected_wall_clock_duration_ms: sustained_expected_wall_clock_duration_ms(240),
            window_size_seconds: 120,
            window_slide_seconds: 60,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_no_hist),
            debug_output_dir: None,
        })
        .expect("sustained pair should run");
        assert_eq!(pair_no_hist.unified.hybrid_result_count_total, 0);
        assert_eq!(pair_no_hist.decomposed.hybrid_result_count_total, 0);

        // 3. Removing live input yields zero hybrid results
        let workload_no_live =
            prepare_sustained_workload(100, 0, 1, 120, 60).expect("workload should build");
        let pair_no_live = run_sustained_pair(SustainedRunConfig {
            mode: ExecutionMode::Warm,
            time_mode: TimeMode::Virtual,
            run_index: 0,
            is_warmup: false,
            historical_events: 100,
            live_duration_seconds: 0,
            event_rate_hz: 1,
            event_interval_ms: sustained_event_interval_ms(1),
            expected_wall_clock_duration_ms: sustained_expected_wall_clock_duration_ms(0),
            window_size_seconds: 120,
            window_slide_seconds: 60,
            metadata: &metadata,
            adapter: &adapter,
            warm_workload: Some(&workload_no_live),
            debug_output_dir: None,
        })
        .expect("sustained pair should run");
        assert_eq!(pair_no_live.unified.hybrid_result_count_total, 0);
        assert_eq!(pair_no_live.decomposed.hybrid_result_count_total, 0);
    }
}
