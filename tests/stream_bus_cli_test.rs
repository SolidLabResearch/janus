//! Stream Bus CLI Integration Tests
//!
//! These tests verify the CLI functionality by running it as a subprocess
//! and checking the output and results.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::Command;

const TEST_DATA_DIR: &str = "test_data_cli";

fn setup_test_environment(test_name: &str) -> std::io::Result<String> {
    let test_dir = format!("{}_{}", TEST_DATA_DIR, test_name);
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir)?;
    Ok(test_dir)
}

fn cleanup_test_environment(test_dir: &str) {
    let _ = fs::remove_dir_all(test_dir);
}

fn create_test_rdf_file(path: &str, num_events: usize) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    for i in 0..num_events {
        writeln!(
            file,
            "<http://example.org/sensor{}> <http://example.org/temperature> \"{}\" <http://example.org/graph1> .",
            i,
            20.0 + (i as f64 * 0.1)
        )?;
    }
    file.sync_all()?;
    Ok(())
}

fn get_cli_binary() -> String {
    if Path::new("target/debug/stream_bus_cli").exists() {
        "target/debug/stream_bus_cli".to_string()
    } else {
        "target/release/stream_bus_cli".to_string()
    }
}

#[test]
fn test_cli_help_flag() {
    let output = Command::new(get_cli_binary())
        .arg("--help")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Stream Bus - Publish RDF Events"));
    assert!(stdout.contains("--input"));
    assert!(stdout.contains("--broker"));
    assert!(stdout.contains("--topics"));
    assert!(stdout.contains("--rate"));
}

#[test]
fn test_cli_storage_only_mode() {
    let test_dir = setup_test_environment("storage_only").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 10).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--rate")
        .arg("0")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Events read:      10"));
    assert!(stdout.contains("Events stored:    10"));
    assert!(stdout.contains("Storage errors:   0"));

    assert!(Path::new(&storage_path).exists());

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_with_rate_limiting() {
    let test_dir = setup_test_environment("rate_limiting").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 20).unwrap();

    let start = std::time::Instant::now();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--rate")
        .arg("50")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    let elapsed = start.elapsed();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Events read:      20"));

    assert!(elapsed.as_millis() >= 300);

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_missing_input_file() {
    let test_dir = setup_test_environment("missing_file").unwrap();
    let input_file = format!("{}/nonexistent.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--storage-path")
        .arg(&storage_path)
        .output()
        .expect("Failed to run CLI");

    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("Failed to open the file")
            || combined.contains("No such file")
            || combined.contains("File Error")
    );

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_invalid_broker_type() {
    let test_dir = setup_test_environment("invalid_broker").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 5).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("invalid_broker")
        .arg("--storage-path")
        .arg(&storage_path)
        .output()
        .expect("Failed to run CLI");

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown broker type") || stderr.contains("invalid_broker"));

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_multiple_topics() {
    let test_dir = setup_test_environment("multiple_topics").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 5).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--topics")
        .arg("sensors,devices,readings")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[\"sensors\", \"devices\", \"readings\"]"));
    assert!(stdout.contains("Events read:      5"));

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_custom_storage_path() {
    let test_dir = setup_test_environment("custom_storage").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/my_custom_storage", test_dir);

    create_test_rdf_file(&input_file, 5).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());
    assert!(Path::new(&storage_path).exists());

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_with_timestamps_flag() {
    let test_dir = setup_test_environment("with_timestamps").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 5).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Add timestamps: true"));

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_throughput_calculation() {
    let test_dir = setup_test_environment("throughput").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 100).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--rate")
        .arg("0")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Throughput:"));
    assert!(stdout.contains("events/sec"));

    cleanup_test_environment(&test_dir);
}

#[test]
fn test_cli_configuration_display() {
    let test_dir = setup_test_environment("config_display").unwrap();
    let input_file = format!("{}/input.nq", test_dir);
    let storage_path = format!("{}/storage", test_dir);

    create_test_rdf_file(&input_file, 3).unwrap();

    let output = Command::new(get_cli_binary())
        .arg("--input")
        .arg(&input_file)
        .arg("--broker")
        .arg("none")
        .arg("--topics")
        .arg("test_topic")
        .arg("--rate")
        .arg("100")
        .arg("--storage-path")
        .arg(&storage_path)
        .arg("--add-timestamps")
        .output()
        .expect("Failed to run CLI");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Configuration:"));
    assert!(stdout.contains(&format!("Input file: {}", input_file)));
    assert!(stdout.contains("Broker: None"));
    assert!(stdout.contains("[\"test_topic\"]"));
    assert!(stdout.contains("Rate: 100 Hz"));
    assert!(stdout.contains(&format!("Storage: {}", storage_path)));

    cleanup_test_environment(&test_dir);
}
