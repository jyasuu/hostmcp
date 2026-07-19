use super::{Tool, ToolCtx};
use async_trait::async_trait;
use glob::Pattern;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use std::time::SystemTime;

const LIMIT: usize = 100;

#[derive(Deserialize)]
struct Params {
    pattern: String,
    path: Option<String>,
}

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> String {
        "- Fast file pattern matching tool that works with any codebase size\n\
- Supports glob patterns like \"**/*.js\" or \"src/**/*.ts\"\n\
- Returns matching file paths, most recently modified first\n\
- Use this tool when you need to find files by name patterns\n\
- Respects .gitignore; results are capped at 100 matches"
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "The glob pattern to match files against" },
                "path": { "type": "string", "description": "The directory to search in. Defaults to the working directory." }
            },
            "required": ["pattern"]
        })
    }

    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String> {
        let params: Params =
            serde_json::from_value(args).map_err(|e| format!("invalid parameters: {e}"))?;
        let search = params
            .path
            .as_deref()
            .map(|p| ctx.state.resolve(p))
            .unwrap_or_else(|| ctx.state.root.clone());

        if search.is_file() {
            return Err(format!("glob path must be a directory: {}", search.display()));
        }

        let pattern = Pattern::new(&params.pattern)
            .map_err(|e| format!("invalid glob pattern '{}': {e}", params.pattern))?;

        let mut matches: Vec<(std::path::PathBuf, SystemTime)> = Vec::new();
        for entry in WalkBuilder::new(&search).hidden(false).build().flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel = path.strip_prefix(&search).unwrap_or(path);
            if pattern.matches_path(rel) || pattern.matches(&rel.to_string_lossy()) {
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                matches.push((path.to_path_buf(), mtime));
            }
        }

        matches.sort_by(|a, b| b.1.cmp(&a.1));
        let truncated = matches.len() > LIMIT;
        matches.truncate(LIMIT);

        if matches.is_empty() {
            return Ok("No files found".to_string());
        }

        let mut out: Vec<String> = matches.iter().map(|(p, _)| p.display().to_string()).collect();
        if truncated {
            out.push(String::new());
            out.push(format!(
                "(Results are truncated: showing first {LIMIT} results. Consider using a more specific path or pattern.)"
            ));
        }
        Ok(out.join("\n"))
    }
}

#[allow(dead_code)]
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}
