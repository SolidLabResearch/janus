//! HTTP API module for Janus
//!
//! Provides REST and WebSocket endpoints for:
//! - Query registration and management
//! - Live result streaming
//! - Stream bus replay control

pub mod error;
pub mod handlers;
pub mod server;
pub mod types;

pub use server::{
    create_server, create_server_with_state, start_server, AppState, ErrorResponse,
    ListQueriesResponse, QueryDetailsResponse, QueryResultBroadcast, RegisterQueryRequest,
    RegisterQueryResponse, ReplayStatusResponse, StartReplayRequest, SuccessResponse,
};
