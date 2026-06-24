use std::collections::{HashMap, HashSet};
use crate::parsing::janusql_parser::JanusQLParser;
use crate::parsing::janusql_parser::ast::{
    PrefixDeclaration, WindowClause, SourceKind, WindowSpec, WindowDefinition, WindowType,
    BaselineClause, BaselineBootstrapMode, BaselineUse, BaselineDefinition,
    HistoricalMaterializationKind, RegisterClause, NestedSubquery
};

impl JanusQLParser {
    pub(crate) fn parse_prefix_declaration(
        &self,
        line: &str,
    ) -> Result<PrefixDeclaration, Box<dyn std::error::Error>> {
        let rest = line
            .strip_prefix("PREFIX")
            .ok_or_else(|| self.parse_error("PREFIX clause must start with PREFIX"))?
            .trim();
        let (prefix, namespace) = rest
            .split_once(':')
            .ok_or_else(|| self.parse_error(format!("Invalid PREFIX clause: {line}")))?;
        let namespace = namespace.trim();

        if !namespace.starts_with('<') || !namespace.ends_with('>') {
            return Err(self.parse_error(format!(
                "PREFIX namespace must be enclosed in angle brackets: {line}"
            )));
        }

        Ok(PrefixDeclaration {
            prefix: prefix.trim().to_string(),
            namespace: namespace[1..namespace.len() - 1].to_string(),
        })
    }

    pub(crate) fn parse_window_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<WindowClause, Box<dyn std::error::Error>> {
        let (header, spec) = line
            .split_once('[')
            .ok_or_else(|| self.parse_error(format!("Missing window spec in clause: {line}")))?;
        let spec = spec
            .trim()
            .strip_suffix(']')
            .ok_or_else(|| self.parse_error(format!("Window spec must end with ']': {line}")))?;
        let header_parts = header.split_whitespace().collect::<Vec<_>>();

        if header_parts.len() != 7
            || header_parts[0] != "FROM"
            || header_parts[1] != "NAMED"
            || header_parts[2] != "WINDOW"
            || header_parts[4] != "ON"
        {
            return Err(self.parse_error(format!("Invalid window clause header: {line}")));
        }

        let source_kind = self.parse_source_kind(header_parts[5])?;
        let window_name = self.unwrap_iri(header_parts[3], prefix_mapper);
        let source_name = self.unwrap_iri(header_parts[6], prefix_mapper);
        let spec_parts = spec.split_whitespace().collect::<Vec<_>>();
        let spec = match spec_parts.as_slice() {
            ["RANGE", range, "STEP", step] => {
                if source_kind != SourceKind::Stream {
                    return Err(self.parse_error(
                        "Live RANGE/STEP windows are only supported on STREAM sources",
                    ));
                }
                WindowSpec::LiveSliding { range: range.parse()?, step: step.parse()? }
            }
            ["OFFSET", offset, "RANGE", range, "STEP", step] => WindowSpec::HistoricalSliding {
                offset: offset.parse()?,
                range: range.parse()?,
                step: step.parse()?,
            },
            ["START", start, "END", end] => {
                WindowSpec::HistoricalFixed { start: start.parse()?, end: end.parse()? }
            }
            _ => {
                return Err(self.parse_error(format!("Unsupported window specification: [{spec}]")));
            }
        };

        Ok(WindowClause { window_name, source_kind, source_name, spec })
    }

    pub(crate) fn parse_source_kind(&self, raw: &str) -> Result<SourceKind, Box<dyn std::error::Error>> {
        match raw {
            "STREAM" => Ok(SourceKind::Stream),
            "LOG" => Ok(SourceKind::Log),
            _ => Err(self.parse_error(format!("Unsupported source kind: {raw}"))),
        }
    }

    pub(crate) fn lower_window_clause(&self, window: &WindowClause) -> WindowDefinition {
        match window.spec {
            WindowSpec::LiveSliding { range, step } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                stream_name: window.source_name.clone(),
                width: range,
                slide: step,
                offset: None,
                start: None,
                end: None,
                window_type: WindowType::Live,
            },
            WindowSpec::HistoricalSliding { offset, range, step } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                stream_name: window.source_name.clone(),
                width: range,
                slide: step,
                offset: Some(offset),
                start: None,
                end: None,
                window_type: WindowType::HistoricalSliding,
            },
            WindowSpec::HistoricalFixed { start, end } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                stream_name: window.source_name.clone(),
                width: 0,
                slide: 0,
                offset: None,
                start: Some(start),
                end: Some(end),
                window_type: WindowType::HistoricalFixed,
            },
        }
    }

    pub(crate) fn is_legacy_baseline_clause(&self, line: &str) -> bool {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        parts.len() == 4 && parts[0] == "USING" && parts[1] == "BASELINE"
    }

    pub(crate) fn parse_baseline_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<BaselineClause, Box<dyn std::error::Error>> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "USING" || parts[1] != "BASELINE" {
            return Err(self.parse_error(format!("Invalid USING BASELINE clause: {line}")));
        }

        let mode = match parts[3] {
            "LAST" => BaselineBootstrapMode::Last,
            "AGGREGATE" => BaselineBootstrapMode::Aggregate,
            other => {
                return Err(self.parse_error(format!(
                    "Unsupported baseline mode '{other}'. Use LAST or AGGREGATE"
                )))
            }
        };

        Ok(BaselineClause { window_name: self.unwrap_iri(parts[2], prefix_mapper), mode })
    }

    pub(crate) fn parse_baseline_use_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<BaselineUse, Box<dyn std::error::Error>> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 3 || parts[0] != "USING" || parts[1] != "BASELINE" {
            return Err(self.parse_error(format!("Invalid USING BASELINE clause: {line}")));
        }

        Ok(BaselineUse { name: self.unwrap_iri(parts[2], prefix_mapper) })
    }

    pub(crate) fn parse_baseline_definition_header(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 7
            || parts[0] != "DEFINE"
            || parts[1] != "BASELINE"
            || parts[3] != "ON"
            || parts[4] != "WINDOW"
            || parts[6] != "AS"
        {
            return Err(self.parse_error(format!("Invalid DEFINE BASELINE clause: {line}")));
        }

        Ok((
            self.unwrap_iri(parts[2], prefix_mapper),
            self.unwrap_iri(parts[5], prefix_mapper),
        ))
    }

    pub(crate) fn build_baseline_definition(
        &self,
        name: String,
        source_window: String,
        raw_lines: &[String],
    ) -> Result<BaselineDefinition, Box<dyn std::error::Error>> {
        let raw_query = raw_lines.join("\n").trim().to_string();
        if raw_query.is_empty() {
            return Err(
                self.parse_error(format!("DEFINE BASELINE '{}' must include a SELECT query", name))
            );
        }

        let mut select_clause = String::new();
        let mut where_lines = Vec::new();
        let mut group_by_clause = None;
        let mut having_clause = None;
        let mut in_where = false;

        for line in raw_query.lines().map(str::to_string).collect::<Vec<_>>() {
            let trimmed = line.trim();
            if trimmed.starts_with("SELECT") {
                select_clause = trimmed.to_string();
                in_where = false;
            } else if trimmed.starts_with("WHERE") {
                in_where = true;
                where_lines.push(line);
            } else if trimmed.starts_with("GROUP BY") {
                group_by_clause = Some(trimmed.to_string());
                in_where = false;
            } else if trimmed.starts_with("HAVING") {
                having_clause = Some(trimmed.to_string());
                in_where = false;
            } else if in_where {
                where_lines.push(line);
            } else if !select_clause.is_empty() {
                select_clause.push(' ');
                select_clause.push_str(trimmed);
            }
        }

        if select_clause.is_empty() || where_lines.is_empty() {
            return Err(self.parse_error(format!(
                "DEFINE BASELINE '{}' must include SELECT and WHERE clauses",
                name
            )));
        }

        let output_variables = self.extract_output_variables(&select_clause);

        Ok(BaselineDefinition {
            name,
            source_window: source_window.clone(),
            source_windows: vec![source_window.clone()],
            raw_query,
            select_clause,
            where_clause: where_lines.join("\n"),
            group_by_clause,
            having_clause,
            output_variables,
            materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
        })
    }

    pub(crate) fn parse_register_clause(
        &self,
        line: &str,
        prefix_mapper: &HashMap<String, String>,
    ) -> Result<RegisterClause, Box<dyn std::error::Error>> {
        let rest = line
            .strip_prefix("REGISTER")
            .ok_or_else(|| self.parse_error("REGISTER clause must start with REGISTER"))?
            .trim();
        let parts = rest.split_whitespace().collect::<Vec<_>>();

        if parts.len() != 3 || parts[2] != "AS" {
            return Err(self.parse_error(format!("Invalid REGISTER clause: {line}")));
        }

        Ok(RegisterClause {
            operator: parts[0].to_string(),
            name: self.unwrap_iri(parts[1], prefix_mapper),
        })
    }

    pub(crate) fn parse_nested_subquery(
        &self,
        where_clause: &str,
        block_start: usize,
        block_end: usize,
    ) -> Result<NestedSubquery, Box<dyn std::error::Error>> {
        let raw_query = where_clause[block_start + 1..block_end].trim().to_string();
        let mut select_clause = String::new();
        let mut where_lines = Vec::new();
        let mut group_by_clause = None;
        let mut having_clause = None;
        let mut in_where = false;

        for line in raw_query.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("SELECT") {
                select_clause = trimmed.to_string();
                in_where = false;
            } else if trimmed.starts_with("WHERE") {
                in_where = true;
                where_lines.push(line.to_string());
            } else if trimmed.starts_with("GROUP BY") {
                group_by_clause = Some(trimmed.to_string());
                in_where = false;
            } else if trimmed.starts_with("HAVING") {
                having_clause = Some(trimmed.to_string());
                in_where = false;
            } else if in_where {
                where_lines.push(line.to_string());
            } else if !select_clause.is_empty() {
                select_clause.push(' ');
                select_clause.push_str(trimmed);
            }
        }

        if select_clause.is_empty() || where_lines.is_empty() {
            return Err(self.parse_error("Nested subquery must include SELECT and WHERE clauses"));
        }

        let where_clause = where_lines.join("\n");
        Ok(NestedSubquery {
            raw_query,
            select_clause: select_clause.clone(),
            where_windows: self.extract_where_windows(&where_clause),
            where_clause,
            group_by_clause,
            having_clause,
            output_variables: self.extract_output_variables(&select_clause),
            block_start,
            block_end: block_end + 1,
        })
    }

    pub(crate) fn filter_select_clause(&self, select_clause: &str, allowed_vars: &HashSet<String>) -> String {
        if allowed_vars.is_empty() {
            return select_clause.to_string();
        }

        let trimmed = select_clause.trim();
        if !trimmed.to_uppercase().starts_with("SELECT") {
            return select_clause.to_string();
        }

        let content = trimmed[6..].trim();
        let projection_items = self.extract_projection_items(content);
        let mut kept_items = Vec::new();

        for item in projection_items {
            let vars_in_item = self.extract_variables(&item);
            if vars_in_item.iter().any(|var| allowed_vars.contains(var)) {
                kept_items.push(item);
            }
        }

        if kept_items.is_empty() {
            return select_clause.to_string();
        }

        format!("SELECT {}", kept_items.join(" "))
    }
}
