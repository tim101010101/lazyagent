use std::path::PathBuf;
use std::process::Command;

use tracing::trace;

use crate::protocol::AgentStatus;

/// Context for status resolution, passed to each resolver in the chain.
pub struct ResolveContext {
    pub pane_pid: u32,
    /// The actual agent process PID found during discovery (descendant of pane_pid).
    pub matched_pid: Option<u32>,
    pub pane_cwd: String,
    pub pane_id: String,
    pub process_start_time: Option<u64>,
}

impl ResolveContext {
    pub fn new(
        pane_pid: u32,
        pane_cwd: String,
        pane_id: String,
        process_start_time: Option<u64>,
        matched_pid: Option<u32>,
    ) -> Self {
        Self {
            pane_pid,
            matched_pid,
            pane_cwd,
            pane_id,
            process_start_time,
        }
    }
}

/// Trait for status resolution strategies. Each provider returns an ordered list.
/// First resolver to return Some(status) wins.
pub trait StatusResolver: Send + Sync {
    /// Try to resolve status. None = can't determine, try next resolver.
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus>;
}

/// Find .jsonl files opened by a process via `lsof -p <pid> -Fn`.
/// Returns the first .jsonl path found.
pub fn find_open_jsonl(pid: u32) -> Option<PathBuf> {
    let output = Command::new("lsof")
        .args(["-p", &pid.to_string(), "-Fn"])
        .output()
        .ok()?;

    if !output.status.success() {
        trace!(pid, "lsof failed or process gone");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // lsof -Fn outputs "n<path>" lines for file names
        if let Some(path) = line.strip_prefix('n') {
            if path.ends_with(".jsonl") {
                trace!(pid, path, "found open jsonl via lsof");
                return Some(PathBuf::from(path));
            }
        }
    }

    trace!(pid, "no open jsonl found via lsof");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command as StdCommand, Stdio};

    #[test]
    fn test_find_open_jsonl_nonexistent_pid() {
        let result = find_open_jsonl(999_999_999);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_open_jsonl_with_real_fd() {
        // Create a temp .jsonl file
        let dir = std::env::temp_dir();
        let jsonl_path = dir.join("test_lsof_integration.jsonl");
        std::fs::write(&jsonl_path, r#"{"type":"test"}"#).unwrap();

        // Spawn a child process that holds the file open via `tail -f`
        let child = StdCommand::new("tail")
            .args(["-f", jsonl_path.to_str().unwrap()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                std::fs::remove_file(&jsonl_path).ok();
                return; // skip if tail not available
            }
        };

        let pid = child.id();

        // Small delay for fd to be visible to lsof
        std::thread::sleep(std::time::Duration::from_millis(100));

        let result = find_open_jsonl(pid);

        // Canonicalize before cleanup (file must exist)
        let expected = jsonl_path.canonicalize().unwrap();

        // Cleanup
        child.kill().ok();
        child.wait().ok();
        std::fs::remove_file(&jsonl_path).ok();

        assert_eq!(
            result.map(|p| std::fs::canonicalize(&p).unwrap_or(p)),
            Some(expected)
        );
    }

    #[test]
    fn test_find_open_jsonl_no_jsonl_fd() {
        // Spawn a process that does NOT hold any .jsonl file
        let child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(_) => return,
        };

        let pid = child.id();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = find_open_jsonl(pid);

        child.kill().ok();
        child.wait().ok();

        assert!(result.is_none());
    }
}
