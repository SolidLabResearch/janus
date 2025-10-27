//! HTTP client for external RDF stores (Apache Jena, remote Oxigraph, etc.)

#[cfg(feature = "http-client")]
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::error::{RdfError, Result};
use crate::{QueryResultFormat, RdfFormat};

/// Configuration for an RDF store endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct RdfStoreEndpoint {
    url: String,
    store_type: String,
    auth_token: Option<String>,
    timeout_secs: u64,
}

#[wasm_bindgen]
impl RdfStoreEndpoint {
    #[wasm_bindgen(constructor)]
    pub fn new(url: String, store_type: String) -> Self {
        Self {
            url,
            store_type,
            auth_token: None,
            timeout_secs: 30,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    #[wasm_bindgen(setter)]
    pub fn set_url(&mut self, url: String) {
        self.url = url;
    }

    #[wasm_bindgen(getter)]
    pub fn store_type(&self) -> String {
        self.store_type.clone()
    }

    #[wasm_bindgen(setter)]
    pub fn set_store_type(&mut self, store_type: String) {
        self.store_type = store_type;
    }

    #[wasm_bindgen(getter)]
    pub fn auth_token(&self) -> Option<String> {
        self.auth_token.clone()
    }

    #[wasm_bindgen(setter)]
    pub fn set_auth_token(&mut self, token: Option<String>) {
        self.auth_token = token;
    }

    #[wasm_bindgen(getter)]
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    #[wasm_bindgen(setter)]
    pub fn set_timeout_secs(&mut self, timeout: u64) {
        self.timeout_secs = timeout;
    }
}

/// HTTP client for interacting with remote RDF stores
#[cfg(feature = "http-client")]
#[wasm_bindgen]
pub struct HttpRdfClient {
    endpoint: RdfStoreEndpoint,
    client: Client,
}

#[cfg(feature = "http-client")]
#[wasm_bindgen]
impl HttpRdfClient {
    /// Create a new HTTP RDF client
    #[wasm_bindgen(constructor)]
    pub fn new(endpoint: RdfStoreEndpoint) -> Result<HttpRdfClient> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(endpoint.timeout_secs))
            .build()
            .map_err(|e| RdfError::HttpError(e.to_string()))?;

        Ok(HttpRdfClient { endpoint, client })
    }

    /// Execute a SPARQL query against the remote store
    #[wasm_bindgen(js_name = query)]
    pub async fn query(&self, sparql: &str, format: &str) -> Result<String> {
        let format_enum = QueryResultFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported query result format".to_string()))?;

        let url = self.get_query_endpoint();

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request);

        let response = match self.endpoint.store_type.as_str() {
            "jena" => {
                // Apache Jena uses form-encoded query parameter
                request
                    .form(&[("query", sparql)])
                    .header("Accept", format_enum.media_type())
                    .send()
                    .await
                    .map_err(|e| RdfError::HttpError(e.to_string()))?
            }
            "oxigraph" => {
                // Oxigraph accepts query in body with appropriate content type
                request
                    .header("Content-Type", "application/sparql-query")
                    .header("Accept", format_enum.media_type())
                    .body(sparql.to_string())
                    .send()
                    .await
                    .map_err(|e| RdfError::HttpError(e.to_string()))?
            }
            _ => {
                return Err(RdfError::ConfigError(format!(
                    "Unsupported store type: {}",
                    self.endpoint.store_type
                )))
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RdfError::HttpError(format!(
                "Query failed with status {}: {}",
                status, error_body
            )));
        }

        response
            .text()
            .await
            .map_err(|e| RdfError::HttpError(e.to_string()))
    }

    /// Upload RDF data to the remote store
    #[wasm_bindgen(js_name = uploadData)]
    pub async fn upload_data(
        &self,
        data: &str,
        format: &str,
        graph_uri: Option<String>,
    ) -> Result<()> {
        let format_enum = RdfFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported format".to_string()))?;

        let url = self.get_data_endpoint(graph_uri.as_deref());

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request);

        let response = request
            .header("Content-Type", format_enum.media_type())
            .body(data.to_string())
            .send()
            .await
            .map_err(|e| RdfError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RdfError::HttpError(format!(
                "Upload failed with status {}: {}",
                status, error_body
            )));
        }

        Ok(())
    }

    /// Download RDF data from the remote store
    #[wasm_bindgen(js_name = downloadData)]
    pub async fn download_data(&self, format: &str, graph_uri: Option<String>) -> Result<String> {
        let format_enum = RdfFormat::from_string(format)
            .ok_or_else(|| RdfError::ParseError("Unsupported format".to_string()))?;

        let url = self.get_data_endpoint(graph_uri.as_deref());

        let mut request = self.client.get(&url);
        request = self.add_auth_header(request);

        let response = request
            .header("Accept", format_enum.media_type())
            .send()
            .await
            .map_err(|e| RdfError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RdfError::HttpError(format!(
                "Download failed with status {}: {}",
                status, error_body
            )));
        }

        response
            .text()
            .await
            .map_err(|e| RdfError::HttpError(e.to_string()))
    }

    /// Execute a SPARQL update operation
    #[wasm_bindgen(js_name = update)]
    pub async fn update(&self, sparql_update: &str) -> Result<()> {
        let url = self.get_update_endpoint();

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request);

        let response = match self.endpoint.store_type.as_str() {
            "jena" => request
                .form(&[("update", sparql_update)])
                .send()
                .await
                .map_err(|e| RdfError::HttpError(e.to_string()))?,
            "oxigraph" => request
                .header("Content-Type", "application/sparql-update")
                .body(sparql_update.to_string())
                .send()
                .await
                .map_err(|e| RdfError::HttpError(e.to_string()))?,
            _ => {
                return Err(RdfError::ConfigError(format!(
                    "Unsupported store type: {}",
                    self.endpoint.store_type
                )))
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RdfError::HttpError(format!(
                "Update failed with status {}: {}",
                status, error_body
            )));
        }

        Ok(())
    }

    /// Check if the remote store is available
    #[wasm_bindgen(js_name = ping)]
    pub async fn ping(&self) -> Result<bool> {
        let mut request = self.client.get(&self.endpoint.url);
        request = self.add_auth_header(request);

        let response = request
            .send()
            .await
            .map_err(|e| RdfError::HttpError(e.to_string()))?;

        Ok(response.status().is_success())
    }

    /// Get store statistics (if supported)
    #[wasm_bindgen(js_name = getStats)]
    pub async fn get_stats(&self) -> Result<String> {
        let stats_query = r#"
            SELECT (COUNT(*) as ?count) WHERE {
                ?s ?p ?o
            }
        "#;

        self.query(stats_query, "json").await
    }
}

#[cfg(feature = "http-client")]
impl HttpRdfClient {
    fn get_query_endpoint(&self) -> String {
        match self.endpoint.store_type.as_str() {
            "jena" => format!("{}/sparql", self.endpoint.url.trim_end_matches('/')),
            "oxigraph" => format!("{}/query", self.endpoint.url.trim_end_matches('/')),
            _ => format!("{}/query", self.endpoint.url.trim_end_matches('/')),
        }
    }

    fn get_update_endpoint(&self) -> String {
        match self.endpoint.store_type.as_str() {
            "jena" => format!("{}/update", self.endpoint.url.trim_end_matches('/')),
            "oxigraph" => format!("{}/update", self.endpoint.url.trim_end_matches('/')),
            _ => format!("{}/update", self.endpoint.url.trim_end_matches('/')),
        }
    }

    fn get_data_endpoint(&self, graph_uri: Option<&str>) -> String {
        let base = match self.endpoint.store_type.as_str() {
            "jena" => format!("{}/data", self.endpoint.url.trim_end_matches('/')),
            "oxigraph" => format!("{}/store", self.endpoint.url.trim_end_matches('/')),
            _ => format!("{}/data", self.endpoint.url.trim_end_matches('/')),
        };

        if let Some(graph) = graph_uri {
            format!("{}?graph={}", base, urlencoding::encode(graph))
        } else {
            base
        }
    }

    fn add_auth_header(&self, request: RequestBuilder) -> RequestBuilder {
        if let Some(token) = &self.endpoint.auth_token {
            request.header("Authorization", format!("Bearer {}", token))
        } else {
            request
        }
    }
}

/// Batch operation for multiple RDF operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    pub operation_type: String,
    pub data: String,
    pub graph_uri: Option<String>,
}

#[cfg(feature = "http-client")]
impl HttpRdfClient {
    /// Execute multiple operations in batch
    pub async fn batch_operations(&self, operations: Vec<BatchOperation>) -> Result<Vec<String>> {
        let mut results = Vec::new();

        for op in operations {
            let result = match op.operation_type.as_str() {
                "query" => self.query(&op.data, "json").await?,
                "update" => {
                    self.update(&op.data).await?;
                    "OK".to_string()
                }
                "upload" => {
                    self.upload_data(&op.data, "turtle", op.graph_uri).await?;
                    "OK".to_string()
                }
                _ => {
                    return Err(RdfError::ConfigError(format!(
                        "Unknown operation type: {}",
                        op.operation_type
                    )))
                }
            };
            results.push(result);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_creation() {
        let endpoint = RdfStoreEndpoint::new(
            "http://localhost:3030/dataset".to_string(),
            "jena".to_string(),
        );
        assert_eq!(endpoint.url, "http://localhost:3030/dataset");
        assert_eq!(endpoint.store_type, "jena");
        assert_eq!(endpoint.timeout_secs, 30);
    }

    #[test]
    fn test_endpoint_with_auth() {
        let mut endpoint = RdfStoreEndpoint::new(
            "http://localhost:3030/dataset".to_string(),
            "jena".to_string(),
        );
        endpoint.set_auth_token(Some("test-token".to_string()));
        assert_eq!(endpoint.auth_token, Some("test-token".to_string()));
    }

    #[cfg(feature = "http-client")]
    #[test]
    fn test_query_endpoint_jena() {
        let endpoint = RdfStoreEndpoint::new(
            "http://localhost:3030/dataset".to_string(),
            "jena".to_string(),
        );
        let client = HttpRdfClient::new(endpoint).unwrap();
        let query_url = client.get_query_endpoint();
        assert_eq!(query_url, "http://localhost:3030/dataset/sparql");
    }

    #[cfg(feature = "http-client")]
    #[test]
    fn test_query_endpoint_oxigraph() {
        let endpoint =
            RdfStoreEndpoint::new("http://localhost:7878".to_string(), "oxigraph".to_string());
        let client = HttpRdfClient::new(endpoint).unwrap();
        let query_url = client.get_query_endpoint();
        assert_eq!(query_url, "http://localhost:7878/query");
    }

    #[cfg(feature = "http-client")]
    #[test]
    fn test_data_endpoint_with_graph() {
        let endpoint = RdfStoreEndpoint::new(
            "http://localhost:3030/dataset".to_string(),
            "jena".to_string(),
        );
        let client = HttpRdfClient::new(endpoint).unwrap();
        let data_url = client.get_data_endpoint(Some("http://example.org/graph"));
        assert!(data_url.contains("graph="));
    }
}
