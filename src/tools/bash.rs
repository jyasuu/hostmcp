use super::{truncate_output, Tool, ToolCtx, MAX_OUTPUT_BYTES};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

const DEFAULT_TIMEOUT_MS: u64 = 2 * 60 * 1000;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;

#[derive(Deserialize)]
struct Params {
    command: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    cwd: Option<String>,
}

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> String {
        "Executes a shell command (via `sh -c`) and returns its output.\n\n\
Usage:\n\
- command is required; description is an optional short note about what the command does (for logging/UI only).\n\
- timeout is in milliseconds, default 120000, max 600000. The process is killed on timeout.\n\
- cwd optionally overrides the working directory for this command (defaults to the server's working directory).\n\
- stdout and stderr are captured separately and each truncated to 30KB.\n\
- Prefer the read/edit/glob/grep/list tools for file operations instead of shelling out to cat/find/grep/ls."
            .to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" },
                "description": { "type": "string", "description": "A short human-readable description of what this command does" },
                "timeout": { "type": "integer", "minimum": 1, "description": "Timeout in milliseconds (default 120000, max 600000)" },
                "cwd": { "type": "string", "description": "Working directory to run the command in" }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value, ctx: &ToolCtx) -> Result<String, String> {
        let params: Params =
            serde_json::from_value(args).map_err(|e| format!("invalid parameters: {e}"))?;
        let _ = &params.description; // documentation-only field

        let timeout_ms = params.timeout.unwrap_or(DEFAULT_TIMEOUT_MS).min(MAX_TIMEOUT_MS);
        let cwd = params
            .cwd
            .as_deref()
            .map(|p| ctx.state.resolve(p))
            .unwrap_or_else(|| ctx.state.root.clone());

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&params.command)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn command: {e}"))?;

        let mut stdout_pipe = child.stdout.take().expect("stdout piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr piped");

        let read_stdout = async {
            let mut buf = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buf).await;
            buf
        };
        let read_stderr = async {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf).await;
            buf
        };

        let run = async {
            let (stdout, stderr) = tokio::join!(read_stdout, read_stderr);
            let status = child.wait().await;
            (stdout, stderr, status)
        };

        let (stdout, stderr, status, timed_out) = match timeout(Duration::from_millis(timeout_ms), run).await {
            Ok((stdout, stderr, status)) => (stdout, stderr, status, false),
            Err(_) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                (Vec::new(), Vec::new(), Err(std::io::Error::other("timed out")), true)
            }
        };

        let stdout = String::from_utf8_lossy(&stdout).to_string();
        let stderr = String::from_utf8_lossy(&stderr).to_string();
        let (stdout, stdout_trunc) = truncate_output(&stdout, MAX_OUTPUT_BYTES);
        let (stderr, stderr_trunc) = truncate_output(&stderr, MAX_OUTPUT_BYTES);

        if timed_out {
            return Err(format!(
                "Command timed out after {timeout_ms}ms and was killed.\n\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            ));
        }

        let exit_code = status
            .ok()
            .and_then(|s| s.code())
            .unwrap_or(-1);

        let mut out = format!("exit code: {exit_code}\n\n--- stdout ---\n{stdout}");
        if stdout_trunc {
            out.push_str("\n(stdout truncated)");
        }
        out.push_str("\n\n--- stderr ---\n");
        out.push_str(&stderr);
        if stderr_trunc {
            out.push_str("\n(stderr truncated)");
        }

        if exit_code != 0 {
            Err(out)
        } else {
            Ok(out)
        }
    }
}
