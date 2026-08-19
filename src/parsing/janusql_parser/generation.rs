use crate::parsing::janusql_parser::ast::{
    BaselineDefinition, GeneratedBaselineQuery, HistoricalMaterializationKind, ParsedJanusQuery,
};
use crate::parsing::janusql_parser::JanusQLParser;

impl JanusQLParser {
    pub(crate) fn generate_rspql_query(
        &self,
        parsed: &ParsedJanusQuery,
        prefix_lines: &[String],
    ) -> String {
        let mut lines: Vec<String> = Vec::new();

        for prefix in prefix_lines {
            lines.push(prefix.clone());
        }

        lines.push(String::new());

        if let Some(ref r2s) = parsed.r2s {
            let wrapped_name = self.wrap_iri(&r2s.name, &parsed.prefixes);
            lines.push(format!("REGISTER {} {} AS", r2s.operator, wrapped_name));
        }

        if !parsed.select_clause.is_empty() {
            lines.push(parsed.select_clause.clone());
        }

        lines.push(String::new());

        for window in &parsed.live_windows {
            let wrapped_window_name = self.wrap_iri(&window.window_name, &parsed.prefixes);
            let wrapped_source_name = self.wrap_iri(&window.source_name, &parsed.prefixes);

            lines.push(format!(
                "FROM NAMED WINDOW {} ON STREAM {} [RANGE {} STEP {}]",
                wrapped_window_name, wrapped_source_name, window.width, window.slide
            ));
        }

        if !parsed.where_clause.is_empty() {
            let adapted_where = self.adapt_where_clause_for_live(
                &parsed.ast.where_windows,
                &parsed.where_clause,
                &parsed.live_windows,
                &parsed.prefixes,
            );
            lines.push(adapted_where);
        }

        if let Some(group_by_clause) = &parsed.group_by_clause {
            lines.push(group_by_clause.clone());
        }

        if let Some(having_clause) = &parsed.having_clause {
            lines.push(having_clause.clone());
        }

        lines.join("\n")
    }

    pub(crate) fn generate_sparql_queries(
        &self,
        parsed: &ParsedJanusQuery,
        prefix_lines: &[String],
    ) -> Vec<String> {
        let mut queries = Vec::new();

        for window in &parsed.historical_windows {
            let mut lines: Vec<String> = Vec::new();

            for prefix in prefix_lines {
                lines.push(prefix.clone());
            }

            lines.push(String::new());

            if self
                .find_window_body(&parsed.ast.where_windows, window, &parsed.prefixes)
                .is_none()
            {
                continue;
            }

            let (where_clause, bound_vars) = self.generate_where_and_extract_vars(
                &parsed.ast.where_windows,
                &parsed.where_clause,
                window,
                &parsed.prefixes,
            );

            if !parsed.select_clause.is_empty() {
                let clean_select = self.filter_select_clause(&parsed.select_clause, &bound_vars);
                lines.push(clean_select);
            }

            lines.push(String::new());
            lines.push(where_clause);
            if parsed.live_windows.is_empty() {
                if let Some(group_by_clause) = &parsed.group_by_clause {
                    lines.push(group_by_clause.clone());
                }
                if let Some(having_clause) = &parsed.having_clause {
                    lines.push(having_clause.clone());
                }
            }
            queries.push(lines.join("\n"));
        }

        queries
    }

    pub(crate) fn generate_baseline_queries(
        &self,
        baseline_definitions: &[BaselineDefinition],
        prefix_lines: &[String],
    ) -> Vec<GeneratedBaselineQuery> {
        baseline_definitions
            .iter()
            .map(|definition| {
                let mut lines = prefix_lines.to_vec();
                lines.push(String::new());
                lines.push(definition.select_clause.clone());
                lines.push(String::new());
                let where_clause = match definition.materialization_kind {
                    HistoricalMaterializationKind::ExplicitBaseline => {
                        self.wrap_baseline_where_clause(&definition.where_clause)
                    }
                    HistoricalMaterializationKind::NestedSubquery => {
                        definition.where_clause.clone()
                    }
                };
                lines.push(where_clause);
                if let Some(group_by_clause) = &definition.group_by_clause {
                    lines.push(group_by_clause.clone());
                }
                if let Some(having_clause) = &definition.having_clause {
                    lines.push(having_clause.clone());
                }

                GeneratedBaselineQuery {
                    name: definition.name.clone(),
                    source_window: definition.source_window.clone(),
                    source_windows: definition.source_windows.clone(),
                    sparql_query: lines.join("\n"),
                    output_variables: definition.output_variables.clone(),
                }
            })
            .collect()
    }

    pub(crate) fn wrap_baseline_where_clause(&self, where_clause: &str) -> String {
        let inner = self.extract_where_inner(where_clause);
        format!("WHERE {{\n  GRAPH ?__janus_log_graph {{\n    {}\n  }}\n}}", inner)
    }
}
