use std::path::Path;
use std::process::Command;

fn get_janus_binary() -> String {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_janus") {
        return path;
    }

    let bin_name = if cfg!(windows) { "janus.exe" } else { "janus" };
    let candidates = [
        format!("target/debug/{bin_name}"),
        format!("target/release/{bin_name}"),
        format!("target/llvm-cov-target/debug/{bin_name}"),
        format!("target/llvm-cov-target/release/{bin_name}"),
    ];

    for candidate in candidates {
        if Path::new(&candidate).exists() {
            return candidate;
        }
    }

    panic!("Could not find janus binary in expected target locations");
}

#[test]
fn test_janus_help_lists_primary_entry_points() {
    let output = Command::new(get_janus_binary())
        .arg("--help")
        .output()
        .expect("failed to run janus --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("package-level help"));
    assert!(stdout.contains("benchmark-storage-rdf"));
    assert!(stdout.contains("benchmark-storage"));
}

#[test]
fn test_janus_default_output_points_to_real_binaries() {
    let output = Command::new(get_janus_binary()).output().expect("failed to run janus");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("http_server"));
    assert!(stdout.contains("stream_bus_cli"));
    assert!(stdout.contains("http_client_example"));
}
