use crate::core::RDFEvent;
use std::time::{SystemTime, UNIX_EPOCH};

/// Parse a line of N-Quads/N-Triples into an RDFEvent
/// Supports typed literals with datatype URIs (e.g., "23.5"^^<http://www.w3.org/2001/XMLSchema#decimal>)
pub fn parse_rdf_line(line: &str, add_timestamps: bool) -> Result<RDFEvent, String> {
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Err("Empty line".to_string());
    }

    // Remove trailing dot if present
    let trimmed = trimmed.trim_end_matches('.').trim();

    // Check if the first token is a timestamp
    let (timestamp, remaining) = parse_optional_timestamp(trimmed, add_timestamps)?;

    // Parse subject (URI in angle brackets)
    let (subject, remaining) = parse_uri(remaining, "subject")?;

    // Parse predicate (URI in angle brackets)
    let (predicate, remaining) = parse_uri(remaining, "predicate")?;

    // Parse object (can be URI, plain literal, or typed literal)
    let (object, object_is_literal, object_datatype, remaining) = parse_object(remaining)?;

    // Parse optional graph (URI in angle brackets)
    let (graph, _) = if !remaining.trim().is_empty() {
        match parse_uri(remaining, "graph") {
            Ok((g, rest)) => (g.clone(), rest),
            Err(_) => (String::new(), remaining),
        }
    } else {
        (String::new(), remaining)
    };

    let event = if object_is_literal {
        if let Some(datatype) = object_datatype.as_deref() {
            RDFEvent::new_typed_literal_object(
                timestamp, &subject, &predicate, &object, datatype, &graph,
            )
        } else {
            RDFEvent::new_literal_object(timestamp, &subject, &predicate, &object, &graph)
        }
    } else {
        RDFEvent::new_iri_object(timestamp, &subject, &predicate, &object, &graph)
    };

    Ok(event)
}

/// Parse optional timestamp at the beginning of the line
fn parse_optional_timestamp(input: &str, add_timestamps: bool) -> Result<(u64, &str), String> {
    let input = input.trim_start();

    // Try to parse first token as timestamp
    if let Some(space_idx) = input.find(char::is_whitespace) {
        let first_token = &input[..space_idx];
        if let Ok(ts) = first_token.parse::<u64>() {
            return Ok((ts, input[space_idx..].trim_start()));
        }
    }

    // No timestamp found - generate one if needed
    let timestamp = if add_timestamps {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    } else {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    };

    Ok((timestamp, input))
}

/// Parse a URI enclosed in angle brackets
fn parse_uri<'a>(input: &'a str, field_name: &str) -> Result<(String, &'a str), String> {
    let input = input.trim_start();

    if !input.starts_with('<') {
        return Err(format!("Expected '<' for {} URI, got: {}", field_name, input));
    }

    let end_idx = input
        .find('>')
        .ok_or_else(|| format!("Missing closing '>' for {} URI", field_name))?;

    let uri = input[1..end_idx].to_string();
    let remaining = input[end_idx + 1..].trim_start();

    Ok((uri, remaining))
}

/// Parse object which can be:
/// - URI: <http://example.org/resource>
/// - Plain literal: "some text"
/// - Typed literal: "23.5"^^<http://www.w3.org/2001/XMLSchema#decimal>
/// - Language-tagged literal: "hello"@en
fn parse_object(input: &str) -> Result<(String, bool, Option<String>, &str), String> {
    let input = input.trim_start();

    if input.starts_with('<') {
        // It's a URI
        let (uri, remaining) = parse_uri(input, "object")?;
        return Ok((uri, false, None, remaining));
    }

    if input.starts_with('"') {
        // It's a literal (plain, typed, or language-tagged)
        let (value, datatype, remaining) = parse_literal(input)?;
        return Ok((value, true, datatype, remaining));
    }

    Err(format!("Invalid object format: {}", input))
}

/// Parse a literal with optional datatype or language tag
fn parse_literal(input: &str) -> Result<(String, Option<String>, &str), String> {
    let input = input.trim_start();

    if !input.starts_with('"') {
        return Err("Literal must start with '\"'".to_string());
    }

    // Find the closing quote, handling escaped quotes
    let mut end_idx = 1;
    let chars: Vec<char> = input.chars().collect();

    while end_idx < chars.len() {
        if chars[end_idx] == '"' && (end_idx == 1 || chars[end_idx - 1] != '\\') {
            break;
        }
        end_idx += 1;
    }

    if end_idx >= chars.len() {
        return Err("Missing closing quote for literal".to_string());
    }

    // Extract the literal value (without quotes)
    let literal_value: String = chars[1..end_idx].iter().collect();
    let after_quote = &input[end_idx + 1..];

    // Check for datatype (^^<URI>) or language tag (@lang)
    let (final_value, datatype, remaining) = if after_quote.trim_start().starts_with("^^") {
        // Typed literal - extract just the base value without the datatype annotation
        let after_caret = after_quote.trim_start()[2..].trim_start();

        if after_caret.starts_with('<') {
            // Parse the datatype URI
            let (datatype_uri, rest) = parse_uri(after_caret, "datatype")?;
            (literal_value, Some(datatype_uri), rest)
        } else {
            return Err("Malformed datatype annotation".to_string());
        }
    } else if after_quote.trim_start().starts_with('@') {
        // Language-tagged literal
        let after_at = after_quote.trim_start()[1..].trim_start();
        let lang_end =
            after_at.find(|c: char| c.is_whitespace() || c == '.').unwrap_or(after_at.len());
        let remaining = after_at[lang_end..].trim_start();
        (literal_value, None, remaining)
    } else {
        // Plain literal
        (literal_value, None, after_quote.trim_start())
    };

    Ok((final_value, datatype, remaining))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_typed_literal() {
        let line = r#"<http://example.org/sensor1> <http://example.org/temperature> "23.5"^^<http://www.w3.org/2001/XMLSchema#decimal> <http://example.org/sensorStream> ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.subject, "http://example.org/sensor1");
        assert_eq!(result.predicate, "http://example.org/temperature");
        assert_eq!(result.object, "23.5");
        assert_eq!(result.graph, "http://example.org/sensorStream");
        assert!(result.object_is_literal);
        assert_eq!(
            result.object_datatype.as_deref(),
            Some("http://www.w3.org/2001/XMLSchema#decimal")
        );
    }

    #[test]
    fn test_parse_plain_literal() {
        let line = r#"<http://example.org/sensor1> <http://example.org/name> "Temperature Sensor" <http://example.org/graph> ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.object, "Temperature Sensor");
        assert!(result.object_is_literal);
        assert_eq!(result.object_datatype, None);
    }

    #[test]
    fn test_parse_uri_object() {
        let line = r#"<http://example.org/sensor1> <http://example.org/type> <http://example.org/Sensor> ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.object, "http://example.org/Sensor");
        assert!(!result.object_is_literal);
        assert_eq!(result.object_datatype, None);
    }

    #[test]
    fn test_parse_with_timestamp() {
        let line = r#"1234567890 <http://example.org/s> <http://example.org/p> "value" <http://example.org/g> ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.timestamp, 1234567890);
        assert_eq!(result.subject, "http://example.org/s");
    }

    #[test]
    fn test_parse_without_graph() {
        let line = r#"<http://example.org/s> <http://example.org/p> "value" ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.graph, "");
    }

    #[test]
    fn test_parse_typed_numeric_literal_variants() {
        let cases = [
            (
                r#"<sensor1> <hasValue> "23"^^<http://www.w3.org/2001/XMLSchema#integer> ."#,
                "23",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            (
                r#"<sensor1> <hasValue> "23.1"^^<http://www.w3.org/2001/XMLSchema#decimal> ."#,
                "23.1",
                "http://www.w3.org/2001/XMLSchema#decimal",
            ),
            (
                r#"<sensor1> <hasValue> "23e0"^^<http://www.w3.org/2001/XMLSchema#double> ."#,
                "23e0",
                "http://www.w3.org/2001/XMLSchema#double",
            ),
            (
                r#"<sensor1> <hasValue> "23.0"^^<http://www.w3.org/2001/XMLSchema#float> ."#,
                "23.0",
                "http://www.w3.org/2001/XMLSchema#float",
            ),
        ];

        for (line, value, datatype) in cases {
            let result = parse_rdf_line(line, false).unwrap();
            assert_eq!(result.object, value);
            assert!(result.object_is_literal);
            assert_eq!(result.object_datatype.as_deref(), Some(datatype));
        }
    }

    #[test]
    fn test_parse_distinguishes_literal_and_iri_forms() {
        let plain = parse_rdf_line(r#"<s> <p> "23" ."#, false).unwrap();
        assert_eq!(plain.object, "23");
        assert!(plain.object_is_literal);
        assert_eq!(plain.object_datatype, None);

        let typed =
            parse_rdf_line(r#"<s> <p> "23"^^<http://www.w3.org/2001/XMLSchema#integer> ."#, false)
                .unwrap();
        assert!(typed.object_is_literal);
        assert_eq!(
            typed.object_datatype.as_deref(),
            Some("http://www.w3.org/2001/XMLSchema#integer")
        );

        let iri = parse_rdf_line(r#"<s> <p> <http://example.org/object> ."#, false).unwrap();
        assert!(!iri.object_is_literal);

        let urn = parse_rdf_line(r#"<s> <p> <urn:patient:123> ."#, false).unwrap();
        assert_eq!(urn.object, "urn:patient:123");
        assert!(!urn.object_is_literal);

        let url_like_literal = parse_rdf_line(r#"<s> <p> "http://example.org" ."#, false).unwrap();
        assert_eq!(url_like_literal.object, "http://example.org");
        assert!(url_like_literal.object_is_literal);
        assert_eq!(url_like_literal.object_datatype, None);
    }
}
