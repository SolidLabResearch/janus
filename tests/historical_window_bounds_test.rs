use janus::parsing::janusql_parser::{SourceKind, WindowDefinition, WindowType};
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use tempfile::TempDir;

fn sliding_window() -> WindowDefinition {
    WindowDefinition {
        window_name: "http://example.org/sameMinuteYesterday".to_string(),
        source_kind: SourceKind::Log,
        source_name: "http://example.org/stream".to_string(),
        width: 60_000,
        slide: 60_000,
        offset: Some(86_400_000),
        start: None,
        end: None,
        window_type: WindowType::HistoricalSliding,
    }
}

fn fixed_window() -> WindowDefinition {
    WindowDefinition {
        window_name: "http://example.org/historyDay".to_string(),
        source_kind: SourceKind::Log,
        source_name: "http://example.org/stream".to_string(),
        width: 0,
        slide: 0,
        offset: None,
        start: Some(0),
        end: Some(86_400_000),
        window_type: WindowType::HistoricalFixed,
    }
}

#[test]
fn resolves_sliding_historical_bounds_with_range_less_than_offset() {
    let window = sliding_window();
    assert_eq!(window.resolve_historical_bounds(172_800_000), Some((86_400_000, 86_460_000)));
}

#[test]
fn resolves_sliding_historical_bounds_for_next_evaluation() {
    let window = sliding_window();
    assert_eq!(window.resolve_historical_bounds(172_860_000), Some((86_460_000, 86_520_000)));
}

#[test]
fn sliding_historical_bounds_reject_range_greater_than_offset() {
    let mut window = sliding_window();
    window.width = 86_400_001;
    assert_eq!(window.resolve_historical_bounds(172_800_000), None);
}

#[test]
fn sliding_historical_bounds_return_none_when_evaluation_precedes_offset() {
    let window = sliding_window();
    assert_eq!(window.resolve_historical_bounds(86_399_999), None);
}

#[test]
fn sliding_historical_bounds_have_the_configured_width() {
    let window = sliding_window();
    let (start, end) = window.resolve_historical_bounds(172_800_000).unwrap();
    assert_eq!(start, 172_800_000 - 86_400_000);
    assert_eq!(end, start + 60_000);
    assert_eq!(end - start, 60_000);
}

#[test]
fn sliding_historical_bounds_end_at_evaluation_time_when_range_equals_offset() {
    let mut window = sliding_window();
    window.width = 86_400_000;

    assert_eq!(window.resolve_historical_bounds(172_800_000), Some((86_400_000, 172_800_000)));
}

#[test]
fn sliding_historical_storage_query_is_half_open() {
    let temp_dir = TempDir::new().expect("failed to create temporary storage directory");
    let storage = StreamingSegmentedStorage::new(StreamingConfig {
        segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
        ..StreamingConfig::default()
    })
    .expect("failed to create storage");

    for timestamp in [100, 150] {
        storage
            .write_rdf(
                timestamp,
                "http://example.org/sensor",
                "http://example.org/value",
                &timestamp.to_string(),
                "http://example.org/graph",
            )
            .expect("failed to write event");
    }
    storage.flush().expect("failed to flush storage");

    let mut window = sliding_window();
    window.width = 50;
    window.offset = Some(100);
    let (start, end) = window.resolve_historical_bounds(200).expect("bounds should resolve");
    assert_eq!((start, end), (100, 150));

    let events = storage
        .query_rdf_half_open(start, end)
        .expect("half-open historical query should succeed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].timestamp, start);
}

#[test]
fn resolves_fixed_historical_bounds_independent_of_evaluation_time() {
    let window = fixed_window();
    assert_eq!(window.resolve_historical_bounds(1), Some((0, 86_400_000)));
    assert_eq!(window.resolve_historical_bounds(172_860_000), Some((0, 86_400_000)));
}
