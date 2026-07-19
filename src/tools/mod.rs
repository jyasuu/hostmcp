pub mod bash;
pub mod edit;
pub mod glob_tool;
pub mod grep;
pub mod list;
pub mod read;

use crate::state::AppState;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct ToolCtx {
    pub state: Arc<AppState>,
    pub session: String,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> String;
    fn input_schema(&self) -> Value;
    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String>;
}

pub fn registry() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(read::ReadTool),
        Box::new(edit::EditTool),
        Box::new(glob_tool::GlobTool),
        Box::new(grep::GrepTool),
        Box::new(list::ListTool),
        Box::new(bash::BashTool),
    ]
}

/// Shared truncation limits, mirroring opencode's `truncate.ts` defaults.
pub const MAX_OUTPUT_BYTES: usize = 30 * 1024;
pub const MAX_LINE_LENGTH: usize = 2000;

pub fn truncate_output(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}
