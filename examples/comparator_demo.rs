use janus::stream::comparator::{ComparatorConfig, StatefulComparator};

fn main() {
    println!("=== Stateful Comparator Demo ===\n");

    // 1. Setup Configuration
    let config = ComparatorConfig {
        abs_threshold: 5.0,
        rel_threshold: 0.2, // 20% change
        catchup_trigger: 10.0,
        slope_epsilon: 0.1,
        volatility_buffer: 2.0,
        window_size: 10,
        outlier_z_threshold: 3.0,
    };
    println!("Configuration: {:#?}\n", config);

    // 2. Create Stateful Comparator
    let mut comparator = StatefulComparator::new(config);

    // 3. Simulate streaming data over time
    println!("Feeding streaming data and checking for anomalies:\n");

    // Historical baseline: stable around 100.0
    // Live data: starts normal, then becomes volatile and drops
    let data_points = vec![
        (0.0, 100.0, 100.0), // Both normal
        (1.0, 101.0, 100.1), // Both normal
        (2.0, 102.0, 100.2), // Both normal
        (3.0, 103.0, 100.3), // Both normal
        (4.0, 104.0, 100.4), // Both normal
        (5.0, 80.0, 100.5),  // Live drops significantly (catch-up + outlier)
        (6.0, 75.0, 100.6),  // Live continues dropping
        (7.0, 70.0, 100.7),  // Live continues dropping
        (8.0, 65.0, 100.8),  // Live continues dropping
        (9.0, 60.0, 100.9),  // Live continues dropping
    ];

    for (timestamp, live_val, hist_val) in data_points {
        let anomalies = comparator.update_and_compare(timestamp, live_val, hist_val);

        if !anomalies.is_empty() {
            println!(
                "T={:.0}: Live={:.1}, Hist={:.1} -> {} anomalies:",
                timestamp,
                live_val,
                hist_val,
                anomalies.len()
            );
            for anomaly in &anomalies {
                println!("  - {}", anomaly);
            }
            println!();
        } else {
            println!(
                "T={:.0}: Live={:.1}, Hist={:.1} -> No anomalies",
                timestamp, live_val, hist_val
            );
        }
    }
}
