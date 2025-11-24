use janus::Error;

#[test]
fn test_error_display() {
    let err = Error::Config("test error".to_string());
    assert_eq!(format!("{}", err), "Configuration error: test error");
}
