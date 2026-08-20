use janus::parsing::janusql_parser::{SourceKind, WindowDefinition, WindowType};

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
fn resolves_sliding_historical_bounds_for_first_evaluation() {
    let window = sliding_window();
    assert_eq!(window.resolve_historical_bounds(172_800_000), Some((86_340_000, 86_400_000)));
}

#[test]
fn resolves_sliding_historical_bounds_for_next_evaluation() {
    let window = sliding_window();
    assert_eq!(window.resolve_historical_bounds(172_860_000), Some((86_400_000, 86_460_000)));
}

#[test]
fn sliding_historical_bounds_return_none_when_first_window_would_cross_evaluation_time() {
    let mut window = sliding_window();
    window.width = 90_000_000;
    assert_eq!(window.resolve_historical_bounds(172_800_000), None);
}

#[test]
fn sliding_historical_bounds_return_none_on_underflow() {
    let window = sliding_window();
    assert_eq!(window.resolve_historical_bounds(86_400_000), None);
}

#[test]
fn sliding_historical_bounds_have_the_configured_width() {
    let window = sliding_window();
    let (start, end) = window.resolve_historical_bounds(172_800_000).unwrap();
    assert_eq!(end, 172_800_000 - 86_400_000);
    assert_eq!(start, end - 60_000);
    assert_eq!(end - start, 60_000);
}

#[test]
fn resolves_fixed_historical_bounds_independent_of_evaluation_time() {
    let window = fixed_window();
    assert_eq!(window.resolve_historical_bounds(1), Some((0, 86_400_000)));
    assert_eq!(window.resolve_historical_bounds(172_860_000), Some((0, 86_400_000)));
}
