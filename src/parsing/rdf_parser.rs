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
    let (object, remaining) = parse_object(remaining)?;

    // Parse optional graph (URI in angle brackets)
    let (graph, _) = if !remaining.trim().is_empty() {
        match parse_uri(remaining, "graph") {
            Ok((g, rest)) => (g.to_string(), rest),
            Err(_) => (String::new(), remaining),
        }
    } else {
        (String::new(), remaining)
    };

    Ok(RDFEvent::new(timestamp, &subject, &predicate, &object, &graph))
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
fn parse_object(input: &str) -> Result<(String, &str), String> {
    let input = input.trim_start();

    if input.starts_with('<') {
        // It's a URI
        return parse_uri(input, "object");
    }

    if input.starts_with('"') {
        // It's a literal (plain, typed, or language-tagged)
        return parse_literal(input);
    }

    Err(format!("Invalid object format: {}", input))
}

/// Parse a literal with optional datatype or language tag
fn parse_literal(input: &str) -> Result<(String, &str), String> {
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
    let (final_value, remaining) = if after_quote.trim_start().starts_with("^^") {
        // Typed literal - extract just the base value without the datatype annotation
        // The datatype is for SPARQL semantics, but we store just the numeric value
        let after_caret = after_quote.trim_start()[2..].trim_start();

        if after_caret.starts_with('<') {
            // Parse the datatype URI
            let (datatype_uri, rest) = parse_uri(after_caret, "datatype")?;

            // For numeric datatypes, store just the numeric value
            // SPARQL engines will interpret these as numbers for aggregation
            if datatype_uri.contains("XMLSchema#decimal")
                || datatype_uri.contains("XMLSchema#integer")
                || datatype_uri.contains("XMLSchema#double")
                || datatype_uri.contains("XMLSchema#float")
            {
                (literal_value, rest)
            } else {
                // For other datatypes, could append type info, but for now just store value
                (literal_value, rest)
            }
        } else {
            // Malformed datatype
            (literal_value, after_quote)
        }
    } else if after_quote.trim_start().starts_with('@') {
        // Language-tagged literal
        let after_at = after_quote.trim_start()[1..].trim_start();
        let lang_end =
            after_at.find(|c: char| c.is_whitespace() || c == '.').unwrap_or(after_at.len());
        let remaining = after_at[lang_end..].trim_start();
        (literal_value, remaining)
    } else {
        // Plain literal
        (literal_value, after_quote.trim_start())
    };

    Ok((final_value, remaining))
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
    }

    #[test]
    fn test_parse_plain_literal() {
        let line = r#"<http://example.org/sensor1> <http://example.org/name> "Temperature Sensor" <http://example.org/graph> ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.object, "Temperature Sensor");
    }

    #[test]
    fn test_parse_uri_object() {
        let line = r#"<http://example.org/sensor1> <http://example.org/type> <http://example.org/Sensor> ."#;
        let result = parse_rdf_line(line, false).unwrap();

        assert_eq!(result.object, "http://example.org/Sensor");
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
}
