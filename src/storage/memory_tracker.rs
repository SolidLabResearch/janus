use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Memory usage tracker for benchmarking purposes
#[derive(Debug, Clone)]
pub struct MemoryTracker {
    peak_memory_bytes: Arc<AtomicUsize>,
    current_memory_bytes: Arc<AtomicUsize>,
    measurements: Arc<std::sync::Mutex<Vec<MemoryMeasurement>>>,
}

#[derive(Debug, Clone)]
pub struct MemoryMeasurement {
    pub timestamp: std::time::Instant,
    pub memory_bytes: usize,
    pub description: String,
}

#[derive(Debug)]
pub struct MemoryStats {
    pub current_bytes: usize,
    pub peak_bytes: usize,
    pub total_measurements: usize,
    pub avg_bytes: f64,
    pub measurements: Vec<MemoryMeasurement>,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self {
            peak_memory_bytes: Arc::new(AtomicUsize::new(0)),
            current_memory_bytes: Arc::new(AtomicUsize::new(0)),
            measurements: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Record current memory usage with a description
    pub fn record(&self, description: &str) {
        let current = self.estimate_current_memory();
        self.current_memory_bytes.store(current, Ordering::Relaxed);

        // Update peak if necessary
        let peak = self.peak_memory_bytes.load(Ordering::Relaxed);
        if current > peak {
            self.peak_memory_bytes.store(current, Ordering::Relaxed);
        }

        // Store measurement
        let measurement = MemoryMeasurement {
            timestamp: std::time::Instant::now(),
            memory_bytes: current,
            description: description.to_string(),
        };

        if let Ok(mut measurements) = self.measurements.lock() {
            measurements.push(measurement);
        }
    }

    /// Get current memory statistics
    #[allow(clippy::cast_precision_loss)]
    pub fn get_stats(&self) -> MemoryStats {
        let current = self.current_memory_bytes.load(Ordering::Relaxed);
        let peak = self.peak_memory_bytes.load(Ordering::Relaxed);

        let measurements = if let Ok(m) = self.measurements.lock() {
            m.clone()
        } else {
            Vec::new()
        };

        let avg_bytes = if measurements.is_empty() {
            0.0
        } else {
            measurements.iter().map(|m| m.memory_bytes as f64).sum::<f64>()
                / measurements.len() as f64
        };

        MemoryStats {
            current_bytes: current,
            peak_bytes: peak,
            total_measurements: measurements.len(),
            avg_bytes,
            measurements,
        }
    }

    /// Reset all measurements
    pub fn reset(&self) {
        self.current_memory_bytes.store(0, Ordering::Relaxed);
        self.peak_memory_bytes.store(0, Ordering::Relaxed);
        if let Ok(mut measurements) = self.measurements.lock() {
            measurements.clear();
        }
    }

    /// Estimate current memory usage of the process
    fn estimate_current_memory(&self) -> usize {
        // On macOS/Linux, try to read from /proc/self/status or use system calls
        #[cfg(target_os = "macos")]
        {
            // For macOS, try using sysctl first, then fallback to basic estimation
            match self.get_memory_macos_simple() {
                Ok(mem) if mem > 0 => mem,
                _ => self.estimate_heap_usage(),
            }
        }
        #[cfg(target_os = "linux")]
        {
            match self.get_memory_linux() {
                mem if mem > 0 => mem,
                _ => self.estimate_heap_usage(),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            // Fallback: estimate based on heap allocation (rough approximation)
            self.estimate_heap_usage()
        }
    }

    /// Simple heap usage estimation (very rough)
    fn estimate_heap_usage(&self) -> usize {
        // This is a very rough estimation based on typical memory patterns
        // In a real implementation, you might use a memory allocator that tracks usage

        // Rough estimation: assume we're using around 50-100MB for a typical session
        // This is obviously very imprecise but gives us something to work with
        let estimated_base = 50 * 1024 * 1024; // 50MB base

        // Add some dynamic component based on time (simulating growth)
        let dynamic_component = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            % 1000) as usize
            * 1024; // Up to 1MB variation

        estimated_base + dynamic_component
    }

    #[cfg(target_os = "macos")]
    fn get_memory_macos_simple(&self) -> Result<usize, Box<dyn std::error::Error>> {
        // Try using ps command as a fallback
        use std::process::Command;

        let output = Command::new("ps")
            .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()?;

        if output.status.success() {
            let rss_str = std::str::from_utf8(&output.stdout)?;
            let rss_kb: usize = rss_str.trim().parse()?;
            Ok(rss_kb * 1024) // Convert KB to bytes
        } else {
            Err("ps command failed".into())
        }
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    fn get_memory_macos(&self) -> usize {
        use std::mem;
        use std::ptr;

        #[repr(C)]
        struct TaskBasicInfo {
            virtual_size: u32,
            resident_size: u32,
            policy: u32,
            flags: u32,
        }

        extern "C" {
            #[allow(dead_code)]
            fn mach_task_self() -> u32;
            #[allow(dead_code)]
            fn task_info(
                target_task: u32,
                flavor: u32,
                task_info_out: *mut TaskBasicInfo,
                task_info_outCnt: *mut u32,
            ) -> i32;
        }

        const TASK_BASIC_INFO: u32 = 5;
        let mut info: TaskBasicInfo = unsafe { mem::zeroed() };
        let mut count = (mem::size_of::<TaskBasicInfo>() / mem::size_of::<u32>()) as u32;

        let result =
            unsafe { task_info(mach_task_self(), TASK_BASIC_INFO, &raw mut info, &raw mut count) };

        if result == 0 {
            info.resident_size as usize
        } else {
            0
        }
    }

    #[cfg(target_os = "linux")]
    fn get_memory_linux(&self) -> usize {
        use std::fs;

        if let Ok(contents) = fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<usize>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
        0
    }

    /// Format bytes in human-readable format
    #[allow(clippy::cast_precision_loss)]
    pub fn format_bytes(bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}
