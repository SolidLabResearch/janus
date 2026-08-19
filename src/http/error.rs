//! API error types for the HTTP API server.

use crate::api::janus_api::JanusApiError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Error response structure.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Custom error type for API errors.
#[derive(Debug)]
pub enum ApiError {
    JanusError(JanusApiError),
    NotFound(String),
    BadRequest(String),
    InternalError(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::JanusError(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse { error: message });
        (status, body).into_response()
    }
}

impl From<JanusApiError> for ApiError {
    fn from(err: JanusApiError) -> Self {
        ApiError::JanusError(err)
    }
}
