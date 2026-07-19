mod mcp;
mod protocol;
mod state;
mod tools;

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use protocol::{RpcRequest, RpcResponse};
use serde_json::{json, Value};
use state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Parser, Debug)]
#[command(name = "hostmcp", about = "HTTP MCP server exposing filesystem/shell tools for an agent to operate a remote host")]
struct Args {
    /// Host to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to bind to.
    #[arg(long, default_value_t = 8787)]
    port: u16,

    /// Filesystem root that relative tool paths are resolved against.
    /// Absolute paths passed by callers are used as-is.
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

const SESSION_HEADER: &str = "mcp-session-id";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    let root = std::fs::canonicalize(&args.root).unwrap_or(args.root.clone());
    tracing::info!(root = %root.display(), "hostmcp root directory");

    let app_state = Arc::new(AppState::new(root));

    let app = Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/mcp", get(handle_mcp_get))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    tracing::info!(%addr, "listening (POST /mcp)");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

/// MCP Streamable HTTP transport requires the server to accept GET for the
/// SSE stream; we don't push server-initiated messages, so just acknowledge.
async fn handle_mcp_get() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

async fn handle_mcp(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let session = headers
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".to_string());

    // Support both a single JSON-RPC request object and a batch array.
    let is_batch = body.is_array();
    let requests: Vec<Value> = if is_batch {
        body.as_array().cloned().unwrap_or_default()
    } else {
        vec![body]
    };

    let mut responses: Vec<Value> = Vec::new();
    let mut new_session_header: Option<String> = None;

    for raw in requests {
        let parsed: Result<RpcRequest, _> = serde_json::from_value(raw);
        let req = match parsed {
            Ok(r) => r,
            Err(e) => {
                responses.push(
                    serde_json::to_value(RpcResponse::err(
                        Value::Null,
                        protocol::error_codes::PARSE_ERROR,
                        format!("parse error: {e}"),
                    ))
                    .unwrap(),
                );
                continue;
            }
        };

        let is_notification = req.id.is_none();

        if req.method == "initialize" && headers.get(SESSION_HEADER).is_none() {
            new_session_header = Some(uuid::Uuid::new_v4().to_string());
        }

        let effective_session = new_session_header.clone().unwrap_or_else(|| session.clone());
        let result = mcp::dispatch(state.clone(), effective_session, &req.method, req.params).await;

        if is_notification {
            // Notifications never get a response body per JSON-RPC 2.0.
            continue;
        }

        let id = req.id.unwrap_or(Value::Null);
        let response = match result.error {
            Some((code, msg)) => RpcResponse::err(id, code, msg),
            None => RpcResponse::ok(id, result.result.unwrap_or(json!(null))),
        };
        responses.push(serde_json::to_value(response).unwrap());
    }

    let body = if is_batch {
        json!(responses)
    } else {
        responses.into_iter().next().unwrap_or(json!(null))
    };

    let mut response = if body.is_null() {
        // All-notification request: 202 Accepted with empty body.
        StatusCode::ACCEPTED.into_response()
    } else {
        Json(body).into_response()
    };

    if let Some(sid) = new_session_header {
        if let Ok(hv) = HeaderValue::from_str(&sid) {
            response.headers_mut().insert(SESSION_HEADER, hv);
        }
    }

    response
}
