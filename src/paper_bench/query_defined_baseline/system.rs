use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::types::{ResourceSample, ResourceSampler, ResourceSummary};
use super::reporting::summarize_resource_samples;

impl ResourceSampler {
    pub fn start(interval: Duration) -> Self {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_samples = Arc::clone(&samples);
        let handle = thread::spawn(move || {
            let pid: Pid = (std::process::id() as usize).into();
            let mut system = System::new_all();
            let refresh = |system: &mut System| {
                system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    ProcessRefreshKind::new().with_memory().with_cpu(),
                );
            };

            refresh(&mut system);
            let mut next_tick = Instant::now();

            loop {
                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }

                next_tick += interval;
                if let Some(remaining) = next_tick.checked_duration_since(Instant::now()) {
                    thread::sleep(remaining);
                }

                if thread_stop.load(Ordering::Relaxed) {
                    break;
                }

                refresh(&mut system);
                if let Some(process) = system.process(pid) {
                    let mut guard = thread_samples.lock().expect("resource samples mutex poisoned");
                    guard.push(ResourceSample {
                        rss_mb: process.memory() as f64 / (1024.0 * 1024.0),
                        cpu_percent: process.cpu_usage() as f64,
                    });
                }
            }
        });

        Self { stop, samples, handle: Some(handle) }
    }

    pub fn finish(mut self) -> ResourceSummary {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        let samples = self.samples.lock().expect("resource samples mutex poisoned");
        summarize_resource_samples(samples.as_slice())
    }
}
