/// Shared utilities for benchmarking
use std::time::Duration;

/// Analyzes a series of timing measurements, discarding warmup and outlier runs.
///
/// Discards:
/// - First 3 runs (warmup)
/// - Last 2 runs (outliers)
///
/// Returns (mean, std_dev) of the remaining runs.
pub fn analyse_runs(times_ms: &[f64]) -> (f64, f64) {
    if times_ms.len() < 6 {
        // Not enough runs to discard warmup and outliers
        let mean = times_ms.iter().sum::<f64>() / times_ms.len() as f64;
        let variance = times_ms
            .iter()
            .map(|t| (t - mean).powi(2))
            .sum::<f64>()
            / times_ms.len() as f64;
        return (mean, variance.sqrt());
    }

    // Discard first 3 and last 2
    let stable_times = &times_ms[3..times_ms.len() - 2];

    let mean = stable_times.iter().sum::<f64>() / stable_times.len() as f64;
    let variance = stable_times
        .iter()
        .map(|t| (t - mean).powi(2))
        .sum::<f64>()
        / stable_times.len() as f64;

    (mean, variance.sqrt())
}

/// Get system hardware information
pub fn get_hardware_info() -> String {
    let mut info = String::new();

    // Try to read /proc/cpuinfo on Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if line.starts_with("model name") {
                    info.push_str(&format!("CPU: {}\n", line.split(':').nth(1).unwrap_or("").trim()));
                    break;
                }
            }
        }
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal") {
                    info.push_str(&format!("Memory: {}\n", line.split(':').nth(1).unwrap_or("").trim()));
                    break;
                }
            }
        }
    }

    // macOS
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "machdep.cpu.brand_string"])
            .output()
        {
            if let Ok(cpu) = String::from_utf8(output.stdout) {
                info.push_str(&format!("CPU: {}", cpu));
            }
        }

        if let Ok(output) = Command::new("sysctl")
            .args(&["-n", "hw.memsize"])
            .output()
        {
            if let Ok(mem_str) = String::from_utf8(output.stdout) {
                if let Ok(mem_bytes) = mem_str.trim().parse::<u64>() {
                    let mem_gb = mem_bytes / 1_073_741_824;
                    info.push_str(&format!("Memory: {} GB\n", mem_gb));
                }
            }
        }
    }

    if info.is_empty() {
        info.push_str("Hardware: Unknown (system info not available on this platform)\n");
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyse_runs() {
        let times = vec![100.0, 101.0, 102.0, 50.0, 50.5, 51.0, 51.5, 51.2, 200.0, 205.0];
        let (mean, std_dev) = analyse_runs(&times);

        // Should use runs 3-7 (50.0, 50.5, 51.0, 51.5, 51.2)
        // Mean ≈ 50.84, which is close to what we'd expect
        assert!(mean > 50.0 && mean < 52.0);
        assert!(std_dev > 0.0 && std_dev < 1.0);
    }

    #[test]
    fn test_analyse_runs_few_samples() {
        let times = vec![10.0, 11.0, 12.0];
        let (mean, std_dev) = analyse_runs(&times);
        assert_eq!(mean, 11.0);
        assert!(std_dev > 0.0);
    }
}
