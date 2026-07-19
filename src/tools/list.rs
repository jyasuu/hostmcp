use super::{Tool, ToolCtx};
use async_trait::async_trait;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;

const DEFAULT_DEPTH: usize = 3;
const MAX_ENTRIES: usize = 500;

#[derive(Deserialize)]
struct Params {
    path: Option<String>,
    depth: Option<usize>,
}

pub struct ListTool;

#[async_trait]
impl Tool for ListTool {
    fn name(&self) -> &'static str {
        "list"
    }

    fn description(&self) -> String {
        "Lists files and directories in a tree under the given path (defaults to the working directory).\n\n\
Usage:\n\
- path should be an absolute directory path; relative paths are resolved against the server's working directory.\n\
- depth controls how many directory levels to descend (default 3).\n\
- Respects .gitignore and always skips .git.\n\
- Output is capped at 500 entries; narrow the path or depth for large trees."
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The directory to list. Defaults to the working directory." },
                "depth": { "type": "integer", "minimum": 1, "description": "How many directory levels to descend (default 3)" }
            }
        })
    }

    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String> {
        let params: Params =
            serde_json::from_value(args).map_err(|e| format!("invalid parameters: {e}"))?;
        let root = params
            .path
            .as_deref()
            .map(|p| ctx.state.resolve(p))
            .unwrap_or_else(|| ctx.state.root.clone());
        let depth = params.depth.unwrap_or(DEFAULT_DEPTH);

        if !root.exists() {
            return Err(format!("path not found: {}", root.display()));
        }
        if !root.is_dir() {
            return Err(format!("not a directory: {}", root.display()));
        }

        let mut count = 0usize;
        let mut out = vec![format!("{}/", root.display())];

        let walker = WalkBuilder::new(&root)
            .hidden(false)
            .max_depth(Some(depth))
            .sort_by_file_path(|a, b| a.cmp(b))
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path == root {
                continue;
            }
            if count >= MAX_ENTRIES {
                out.push(format!("… (truncated at {MAX_ENTRIES} entries)"));
                break;
            }
            let depth = entry.depth();
            let indent = "  ".repeat(depth.saturating_sub(1));
            let name = display_name(path, entry.file_type().map(|t| t.is_dir()).unwrap_or(false));
            out.push(format!("{indent}{name}"));
            count += 1;
        }

        Ok(out.join("\n"))
    }
}

fn display_name(path: &Path, is_dir: bool) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if is_dir {
        format!("{name}/")
    } else {
        name
    }
}
