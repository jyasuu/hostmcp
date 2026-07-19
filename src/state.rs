use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Per-session bookkeeping. opencode's `edit` tool requires the file to have
/// been read at least once in the same conversation before it can be edited;
/// we mirror that here, keyed by the MCP session id (or "default" when a
/// client doesn't send one).
#[derive(Default)]
pub struct SessionState {
    pub read_files: HashSet<PathBuf>,
}

pub struct AppState {
    /// Filesystem root new relative paths are resolved against. Absolute
    /// paths supplied by the caller are used as-is (matching opencode, which
    /// operates on the whole filesystem, not a sandboxed subtree).
    pub root: PathBuf,
    pub sessions: Mutex<HashMap<String, SessionState>>,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve a user-supplied path against root, without requiring the path
    /// to exist yet (so `edit`/`bash` can reference files about to be created).
    pub fn resolve(&self, p: &str) -> PathBuf {
        let path = Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    pub fn mark_read(&self, session: &str, path: &Path) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions
            .entry(session.to_string())
            .or_default()
            .read_files
            .insert(normalize(path));
    }

    pub fn has_read(&self, session: &str, path: &Path) -> bool {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(session)
            .map(|s| s.read_files.contains(&normalize(path)))
            .unwrap_or(false)
    }
}

/// Best-effort path normalization (canonicalize when possible, else lexical).
fn normalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
