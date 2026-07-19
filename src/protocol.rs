use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC_VERSION: &str = "2.0";
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[allow(dead_code)]
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    /// Absent on notifications (e.g. `notifications/initialized`).
    #[serde(default)]
    pub id: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: JSONRPC_VERSION, id, result: Some(result), error: None }
    }

    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION,
            id,
            result: None,
            error: Some(RpcError { code, message: message.into(), data: None }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[allow(dead_code)]
pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

#[derive(Debug, Serialize)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

#[derive(Debug, Serialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

impl ToolContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self { kind: "text", text: text.into() }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolCallResult {
    pub content: Vec<ToolContent>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolCallResult {
    pub fn success(text: impl Into<String>) -> Self {
        Self { content: vec![ToolContent::text(text)], is_error: false }
    }

    pub fn failure(text: impl Into<String>) -> Self {
        Self { content: vec![ToolContent::text(text)], is_error: true }
    }
}
