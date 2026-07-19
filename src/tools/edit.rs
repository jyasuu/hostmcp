use super::{Tool, ToolCtx};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;

#[derive(Deserialize)]
struct Params {
    #[serde(rename = "filePath")]
    file_path: String,
    #[serde(rename = "oldString")]
    old_string: String,
    #[serde(rename = "newString")]
    new_string: String,
    #[serde(rename = "replaceAll", default)]
    replace_all: bool,
}

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> String {
        "Performs exact string replacements in files.\n\n\
Usage:\n\
- You must use the `read` tool at least once on this file (in the same session) before editing it. This tool errors if you attempt an edit without reading the file first.\n\
- When editing text from `read` tool output, preserve exact indentation as it appears AFTER the line-number prefix (`<n>: `); never include the prefix itself in oldString/newString.\n\
- The edit FAILS if oldString is not found in the file.\n\
- The edit FAILS if oldString matches multiple locations and replaceAll is not set — provide more surrounding context, or set replaceAll to true to replace every occurrence.\n\
- ALWAYS prefer editing existing files over creating new ones."
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filePath": { "type": "string", "description": "The absolute path to the file to edit" },
                "oldString": { "type": "string", "description": "The exact text to replace" },
                "newString": { "type": "string", "description": "The text to replace it with" },
                "replaceAll": { "type": "boolean", "description": "Replace all occurrences of oldString (default false)" }
            },
            "required": ["filePath", "oldString", "newString"]
        })
    }

    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String> {
        let params: Params =
            serde_json::from_value(args).map_err(|e| format!("invalid parameters: {e}"))?;
        let path = ctx.state.resolve(&params.file_path);

        if !path.exists() {
            return Err(format!(
                "File not found: {}. Use the `read` tool first (edit only modifies existing files; use bash/write to create new ones).",
                path.display()
            ));
        }
        if !ctx.state.has_read(&ctx.session, &path) {
            return Err(format!(
                "You must read {} with the `read` tool before editing it.",
                path.display()
            ));
        }
        if params.old_string == params.new_string {
            return Err("oldString and newString are identical — nothing to do.".to_string());
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

        let count = content.matches(params.old_string.as_str()).count();
        if count == 0 {
            return Err("oldString not found in content".to_string());
        }
        if count > 1 && !params.replace_all {
            return Err(format!(
                "Found {count} matches for oldString. Provide more surrounding lines in oldString to identify the correct match, or set replaceAll to true."
            ));
        }

        let updated = if params.replace_all {
            content.replace(params.old_string.as_str(), params.new_string.as_str())
        } else {
            content.replacen(params.old_string.as_str(), params.new_string.as_str(), 1)
        };

        fs::write(&path, &updated).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        ctx.state.mark_read(&ctx.session, &path);

        let replacements = if params.replace_all { count } else { 1 };
        Ok(format!(
            "Edited {} ({replacements} replacement{}).\n\n{}",
            path.display(),
            if replacements == 1 { "" } else { "s" },
            preview(&params.old_string, &params.new_string)
        ))
    }
}

fn preview(old: &str, new: &str) -> String {
    let clip = |s: &str| {
        let s = s.replace('\n', "\\n");
        if s.chars().count() > 120 {
            format!("{}…", s.chars().take(120).collect::<String>())
        } else {
            s
        }
    };
    format!("- {}\n+ {}", clip(old), clip(new))
}
