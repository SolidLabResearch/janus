use crate::core::RDFEvent;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};

const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

pub fn rdf_event_to_quad(event: &RDFEvent) -> Result<Quad, String> {
    let subject = NamedNode::new(&event.subject)
        .map_err(|e| format!("Invalid subject URI '{}': {}", event.subject, e))?;
    let predicate = NamedNode::new(&event.predicate)
        .map_err(|e| format!("Invalid predicate URI '{}': {}", event.predicate, e))?;

    let object = if event.object_is_literal {
        Term::Literal(build_literal(event)?)
    } else {
        let object_node = NamedNode::new(&event.object)
            .map_err(|e| format!("Invalid object URI '{}': {}", event.object, e))?;
        Term::NamedNode(object_node)
    };

    let graph = if event.graph.is_empty() || event.graph == "default" {
        GraphName::DefaultGraph
    } else {
        let graph_node = NamedNode::new(&event.graph)
            .map_err(|e| format!("Invalid graph URI '{}': {}", event.graph, e))?;
        GraphName::NamedNode(graph_node)
    };

    Ok(Quad::new(subject, predicate, object, graph))
}

fn build_literal(event: &RDFEvent) -> Result<Literal, String> {
    if let Some(datatype) = event.object_datatype.as_deref() {
        let datatype_node = NamedNode::new(datatype)
            .map_err(|e| format!("Invalid datatype URI '{datatype}': {e}"))?;
        return Ok(Literal::new_typed_literal(&event.object, datatype_node));
    }

    if event.object.parse::<i64>().is_ok() {
        return Ok(Literal::new_typed_literal(&event.object, NamedNode::new(XSD_INTEGER).unwrap()));
    }

    if event.object.parse::<f64>().is_ok() {
        return Ok(Literal::new_typed_literal(&event.object, NamedNode::new(XSD_DECIMAL).unwrap()));
    }

    Ok(Literal::new_simple_literal(&event.object))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rdf_event_to_quad_explicit_integer() {
        let event = RDFEvent::new_typed_literal_object(
            1,
            "http://example.org/s",
            "http://example.org/p",
            "23",
            "http://www.w3.org/2001/XMLSchema#integer",
            "",
        );
        let quad = rdf_event_to_quad(&event).unwrap();

        let Term::Literal(literal) = quad.object else {
            panic!("expected literal object");
        };
        assert_eq!(literal.value(), "23");
        assert_eq!(literal.datatype().as_str(), XSD_INTEGER);
    }

    #[test]
    fn test_rdf_event_to_quad_explicit_decimal() {
        let event = RDFEvent::new_typed_literal_object(
            1,
            "http://example.org/s",
            "http://example.org/p",
            "23",
            "http://www.w3.org/2001/XMLSchema#decimal",
            "",
        );
        let quad = rdf_event_to_quad(&event).unwrap();

        let Term::Literal(literal) = quad.object else {
            panic!("expected literal object");
        };
        assert_eq!(literal.datatype().as_str(), XSD_DECIMAL);
    }

    #[test]
    fn test_rdf_event_to_quad_legacy_integer_before_float() {
        let event = RDFEvent::new_literal_object(
            1,
            "http://example.org/s",
            "http://example.org/p",
            "23",
            "",
        );
        let quad = rdf_event_to_quad(&event).unwrap();

        let Term::Literal(literal) = quad.object else {
            panic!("expected literal object");
        };
        assert_eq!(literal.datatype().as_str(), XSD_INTEGER);
    }

    #[test]
    fn test_rdf_event_to_quad_url_like_literal_stays_literal() {
        let event = RDFEvent::new_literal_object(
            1,
            "http://example.org/s",
            "http://example.org/p",
            "http://example.org",
            "",
        );
        let quad = rdf_event_to_quad(&event).unwrap();

        assert!(matches!(quad.object, Term::Literal(_)));
    }

    #[test]
    fn test_rdf_event_to_quad_urn_object_stays_named_node() {
        let event = RDFEvent::new_iri_object(
            1,
            "http://example.org/s",
            "http://example.org/p",
            "urn:patient:123",
            "",
        );
        let quad = rdf_event_to_quad(&event).unwrap();

        assert!(matches!(quad.object, Term::NamedNode(_)));
    }
}
