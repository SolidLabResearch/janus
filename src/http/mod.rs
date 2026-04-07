//! HTTP API module for Janus
//!
//! Provides REST and WebSocket endpoints for:
//! - Query registration and management
//! - Live result streaming
//! - Stream bus replay control

pub mod server;

pub use server::{
    create_server, start_server, AppState, ErrorResponse, ListQueriesResponse,
    QueryDetailsResponse, RegisterQueryRequest, RegisterQueryResponse, ReplayStatusResponse,
    StartReplayRequest, SuccessResponse,
};
