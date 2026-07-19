use crate::protocol::{error_codes, ToolCallResult, ToolDescriptor};
use crate::state::AppState;
use crate::tools::{registry, ToolCtx};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct HandleResult {
    pub result: Option<Value>,
    pub error: Option<(i64, String)>,
}

fn ok(v: Value) -> HandleResult {
    HandleResult { result: Some(v), error: None }
}

fn err(code: i64, msg: impl Into<String>) -> HandleResult {
    HandleResult { result: None, error: Some((code, msg.into())) }
}

pub async fn dispatch(state: Arc<AppState>, session: String, method: &str, params: Value) -> HandleResult {
    match method {
        "initialize" => ok(json!({
            "protocolVersion": crate::protocol::MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": "hostmcp",
                "version": env!("CARGO_PKG_VERSION"),
            }
        })),

        "ping" => ok(json!({})),

        "tools/list" => {
            let tools: Vec<ToolDescriptor> = registry()
                .iter()
                .map(|t| ToolDescriptor {
                    name: t.name(),
                    description: t.description(),
                    input_schema: t.input_schema(),
                })
                .collect();
            ok(json!({ "tools": tools }))
        }

        "tools/call" => {
            let name = match params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => return err(error_codes::INVALID_PARAMS, "missing required field: name"),
            };
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

            let tools = registry();
            let Some(tool) = tools.into_iter().find(|t| t.name() == name) else {
                return err(error_codes::METHOD_NOT_FOUND, format!("unknown tool: {name}"));
            };

            let ctx = ToolCtx { state, session };
            let call_result = match tool.call(arguments, &ctx).await {
                Ok(text) => ToolCallResult::success(text),
                Err(text) => ToolCallResult::failure(text),
            };
            ok(serde_json::to_value(call_result).unwrap())
        }

        // Notifications / lifecycle messages we accept but don't act on.
        "notifications/initialized" | "notifications/cancelled" => ok(json!({})),

        other => err(error_codes::METHOD_NOT_FOUND, format!("method not found: {other}")),
    }
}
