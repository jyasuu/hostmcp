use super::{Tool, ToolCtx};
use async_trait::async_trait;
use glob::Pattern;
use ignore::{WalkBuilder, WalkState};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const LIMIT: usize = 100;

#[derive(Deserialize)]
struct Params {
    pattern: String,
    path: Option<String>,
    include: Option<String>,
}

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> String {
        "- Fast content search tool that works with any codebase size\n\
- Searches file contents using regular expressions (Rust `regex` syntax)\n\
- Filter files by pattern with the include parameter (e.g. \"*.js\", \"*.{ts,tsx}\")\n\
- Returns file paths and line numbers with matching lines, capped at 100 matches\n\
- Respects .gitignore"
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "The regex pattern to search for in file contents" },
                "path": { "type": "string", "description": "The directory (or file) to search in. Defaults to the working directory." },
                "include": { "type": "string", "description": "File pattern to include in the search (e.g. \"*.js\", \"*.{ts,tsx}\")" }
            },
            "required": ["pattern"]
        })
    }

    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String> {
        let params: Params =
            serde_json::from_value(args).map_err(|e| format!("invalid parameters: {e}"))?;
        if params.pattern.is_empty() {
            return Err("pattern is required".to_string());
        }
        let re = Regex::new(&params.pattern)
            .map_err(|e| format!("invalid regex '{}': {e}", params.pattern))?;

        let search = params
            .path
            .as_deref()
            .map(|p| ctx.state.resolve(p))
            .unwrap_or_else(|| ctx.state.root.clone());

        let include = match &params.include {
            Some(pat) => Some(
                Pattern::new(pat).map_err(|e| format!("invalid include pattern '{pat}': {e}"))?,
            ),
            None => None,
        };

        let rows: Arc<Mutex<Vec<(PathBuf, usize, String)>>> = Arc::new(Mutex::new(Vec::new()));

        if search.is_file() {
            search_file(&search, &re, &rows);
        } else {
            let walker = WalkBuilder::new(&search).hidden(false).build_parallel();
            walker.run(|| {
                let re = re.clone();
                let include = include.clone();
                let rows = rows.clone();
                Box::new(move |entry| {
                    if rows.lock().unwrap().len() >= LIMIT {
                        return WalkState::Quit;
                    }
                    let Ok(entry) = entry else { return WalkState::Continue };
                    let path = entry.path();
                    if !path.is_file() {
                        return WalkState::Continue;
                    }
                    if let Some(pat) = &include {
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if !pat.matches(&name) {
                            return WalkState::Continue;
                        }
                    }
                    search_file(path, &re, &rows);
                    WalkState::Continue
                })
            });
        }

        let mut rows = Arc::try_unwrap(rows).unwrap().into_inner().unwrap();
        if rows.is_empty() {
            return Ok(format!("No matches found for pattern: {}", params.pattern));
        }

        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let truncated = rows.len() > LIMIT;
        rows.truncate(LIMIT);

        let total = rows.len();
        let mut out = vec![format!(
            "Found {total} match{}{}",
            if total == 1 { "" } else { "es" },
            if truncated { " (more matches available)" } else { "" }
        )];

        let mut current: Option<PathBuf> = None;
        for (path, line, text) in &rows {
            if current.as_ref() != Some(path) {
                if current.is_some() {
                    out.push(String::new());
                }
                current = Some(path.clone());
                out.push(format!("{}:", path.display()));
            }
            out.push(format!("  Line {line}: {text}"));
        }

        if truncated {
            out.push(String::new());
            out.push("(Results truncated. Consider using a more specific path or pattern.)".to_string());
        }

        Ok(out.join("\n"))
    }
}

fn search_file(path: &std::path::Path, re: &Regex, rows: &Mutex<Vec<(PathBuf, usize, String)>>) {
    let Ok(content) = std::fs::read_to_string(path) else { return };
    // Skip binary-ish files (content with NUL bytes slips through read_to_string as an error already).
    for (i, line) in content.lines().enumerate() {
        if re.is_match(line) {
            let mut rows = rows.lock().unwrap();
            if rows.len() >= LIMIT {
                return;
            }
            rows.push((path.to_path_buf(), i + 1, line.chars().take(500).collect()));
        }
    }
}
