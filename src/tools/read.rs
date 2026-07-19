use super::{Tool, ToolCtx, MAX_LINE_LENGTH};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const DEFAULT_READ_LIMIT: usize = 2000;

#[derive(Deserialize)]
struct Params {
    #[serde(rename = "filePath")]
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> String {
        "Read a file or directory from the local filesystem. If the path does not exist, an error is returned.\n\n\
Usage:\n\
- The filePath parameter should be an absolute path.\n\
- By default, this tool returns up to 2000 lines from the start of the file.\n\
- The offset parameter is the line number to start from (1-indexed).\n\
- To read later sections, call this tool again with a larger offset.\n\
- Use the grep tool to find specific content in large files or files with long lines.\n\
- If you are unsure of the correct file path, use the glob tool to look up filenames by glob pattern.\n\
- Contents are returned with each line prefixed by its line number as `<line>: <content>`.\n\
- Any line longer than 2000 characters is truncated.\n\
- For directories, entries are returned one per line (without line numbers) with a trailing `/` for subdirectories."
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filePath": { "type": "string", "description": "The absolute path to the file or directory to read" },
                "offset": { "type": "integer", "minimum": 1, "description": "The line number to start reading from (1-indexed)" },
                "limit": { "type": "integer", "minimum": 1, "description": "The maximum number of lines to read (defaults to 2000)" }
            },
            "required": ["filePath"]
        })
    }

    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String> {
        let params: Params =
            serde_json::from_value(args).map_err(|e| format!("invalid parameters: {e}"))?;
        let path = ctx.state.resolve(&params.file_path);

        if !path.exists() {
            return Err(miss_message(&path));
        }

        if path.is_dir() {
            return read_directory(&path);
        }

        let out = read_file(&path, params.offset.unwrap_or(1), params.limit.unwrap_or(DEFAULT_READ_LIMIT))?;
        ctx.state.mark_read(&ctx.session, &path);
        Ok(out)
    }
}

fn miss_message(path: &Path) -> String {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut suggestions = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.contains(&base) || base.contains(&lower) {
                suggestions.push(dir.join(name).display().to_string());
            }
            if suggestions.len() >= 3 {
                break;
            }
        }
    }

    if suggestions.is_empty() {
        format!("File not found: {}", path.display())
    } else {
        format!(
            "File not found: {}\n\nDid you mean one of these?\n{}",
            path.display(),
            suggestions.join("\n")
        )
    }
}

fn read_directory(path: &Path) -> Result<String, String> {
    let entries = fs::read_dir(path).map_err(|e| format!("failed to read directory {}: {e}", path.display()))?;
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let mut name = entry.file_name().to_string_lossy().to_string();
        let is_dir = if file_type.is_symlink() {
            fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            file_type.is_dir()
        };
        if is_dir {
            name.push('/');
        }
        names.push(name);
    }
    names.sort();

    if names.is_empty() {
        Ok(format!("{}\n(empty directory)", path.display()))
    } else {
        Ok(names.join("\n"))
    }
}

fn read_file(path: &Path, offset: usize, limit: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let content = String::from_utf8_lossy(&bytes);

    // Detect obviously-binary content the lossy conversion can't make sense of.
    if bytes.iter().take(4096).filter(|&&b| b == 0).count() > 0 {
        return Ok(format!(
            "{}\n(binary file, {} bytes — this server does not decode binary attachments; use bash with a hex/text tool if you need to inspect it)",
            path.display(),
            bytes.len()
        ));
    }

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let start = offset.saturating_sub(1).min(total_lines);
    let end = (start + limit).min(total_lines);

    let mut out = String::new();
    for (i, line) in lines[start..end].iter().enumerate() {
        let line_no = start + i + 1;
        if line.chars().count() > MAX_LINE_LENGTH {
            let truncated: String = line.chars().take(MAX_LINE_LENGTH).collect();
            out.push_str(&format!("{line_no}: {truncated}... (line truncated to {MAX_LINE_LENGTH} chars)\n"));
        } else {
            out.push_str(&format!("{line_no}: {line}\n"));
        }
    }

    if end < total_lines {
        out.push_str(&format!(
            "\n(File has more lines. Showing {start_disp}-{end} of {total_lines}. Call again with offset={next} to continue.)\n",
            start_disp = start + 1,
            next = end + 1
        ));
    }

    if out.is_empty() {
        out = "(empty file)".to_string();
    }

    Ok(out)
}
