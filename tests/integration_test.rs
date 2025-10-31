//! Integration tests for Janus
//!
//! These tests verify the overall functionality of the Janus engine
//! by testing the integration of multiple components together.

use janus::{Error, Result};

#[test]
fn test_basic_functionality() {
    // TODO: Add integration tests
    assert!(true);
}

#[test]
fn test_error_types() {
    let config_error = Error::Config("test".to_string());
    assert!(format!("{}", config_error).contains("Configuration error"));

    let store_error = Error::Store("test".to_string());
    assert!(format!("{}", store_error).contains("Store error"));

    let stream_error = Error::Stream("test".to_string());
    assert!(format!("{}", stream_error).contains("Stream error"));

    let query_error = Error::Query("test".to_string());
    assert!(format!("{}", query_error).contains("Query error"));
}

#[test]
fn test_result_type() {
    fn returns_ok() -> Result<i32> {
        Ok(42)
    }

    fn returns_err() -> Result<i32> {
        Err(Error::Other("test error".to_string()))
    }

    assert!(returns_ok().is_ok());
    assert!(returns_err().is_err());
}
