/// Parses an MQTT stream URI into `(host, port, topic)`.
///
/// Handles `mqtt://host:port/topic` and `mqtts://host:port/topic` directly.
/// For any other URI scheme (e.g. `http://example.org/sensors`) it falls back
/// to `localhost:1883` with the last path segment as the topic, keeping all
/// existing queries backward compatible.
pub(crate) fn parse_mqtt_uri(stream_uri: &str) -> (String, u16, String) {
    if stream_uri.starts_with("mqtt://") || stream_uri.starts_with("mqtts://") {
        let without_scheme =
            stream_uri.trim_start_matches("mqtts://").trim_start_matches("mqtt://");

        let (authority, path) = if let Some(slash) = without_scheme.find('/') {
            (&without_scheme[..slash], &without_scheme[slash + 1..])
        } else {
            (without_scheme, "")
        };

        let (host, port) = if let Some(colon) = authority.rfind(':') {
            let port = authority[colon + 1..].parse::<u16>().unwrap_or(1883);
            (authority[..colon].to_string(), port)
        } else {
            (authority.to_string(), 1883u16)
        };

        let topic = if path.is_empty() {
            "default".to_string()
        } else {
            path.to_string()
        };
        return (host, port, topic);
    }

    // Non-mqtt URI: derive topic from last path segment, use localhost:1883.
    let topic = stream_uri
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(stream_uri)
        .to_string();
    ("localhost".to_string(), 1883u16, topic)
}
