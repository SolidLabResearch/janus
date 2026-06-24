use super::helpers::now_ms;
use super::types::ReproMetadata;
use std::{env, process::Command};

pub fn collect_repro_metadata() -> ReproMetadata {
    ReproMetadata {
        git_commit_sha: capture_command("git", &["rev-parse", "HEAD"]),
        branch: capture_command("git", &["branch", "--show-current"]),
        rustc_version: capture_command("rustc", &["--version"]),
        os: detect_os(),
        cpu_model: detect_cpu_model(),
        ram_bytes: detect_ram_bytes(),
        benchmark_command: env::args().collect::<Vec<_>>().join(" "),
        timestamp_unix_ms: now_ms(),
    }
}

pub fn capture_command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn detect_os() -> String {
    format!("{} {}", env::consts::OS, capture_command("uname", &["-r"]))
}

pub fn detect_cpu_model() -> String {
    #[cfg(target_os = "macos")]
    {
        let model = capture_command("sysctl", &["-n", "machdep.cpu.brand_string"]);
        if model != "unknown" {
            return model;
        }
    }
    #[cfg(target_os = "linux")]
    {
        let model =
            capture_command("sh", &["-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"]);
        if model != "unknown" {
            return model.trim().to_string();
        }
    }
    "unknown".to_string()
}

pub fn detect_ram_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        return capture_command("sysctl", &["-n", "hw.memsize"]).parse::<u64>().ok();
    }
    #[cfg(target_os = "linux")]
    {
        return capture_command("sh", &["-c", "grep MemTotal /proc/meminfo | awk '{print $2}'"])
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024);
    }
    #[allow(unreachable_code)]
    None
}

pub fn current_rss_bytes() -> Option<u64> {
    capture_command("ps", &["-o", "rss=", "-p", &std::process::id().to_string()])
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}
