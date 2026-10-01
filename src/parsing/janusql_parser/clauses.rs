use crate::parsing::janusql_parser::ast::{WindowClause, WindowDefinition, WindowSpec, WindowType};
use crate::parsing::janusql_parser::JanusQLParser;
use std::collections::HashSet;

impl JanusQLParser {
    pub(crate) fn lower_window_clause(&self, window: &WindowClause) -> WindowDefinition {
        match window.spec {
            WindowSpec::LiveSliding { range, step } => WindowDefinition {
                window_name: window.window_name.clone(),
                source_kind: window.source_kind.clone(),
                source_name: window.source_name.clone(),
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
                source_name: window.source_name.clone(),
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
                source_name: window.source_name.clone(),
                width: 0,
                slide: 0,
                offset: None,
                start: Some(start),
                end: Some(end),
                window_type: WindowType::HistoricalFixed,
            },
        }
    }

    pub(crate) fn filter_select_clause(
        &self,
        select_clause: &str,
        allowed_vars: &HashSet<String>,
    ) -> String {
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
