use std::cell::OnceCell;
use std::process::Command;

use crate::protocol::AgentStatus;

/// Context for status resolution, passed to each resolver in the chain.
pub struct ResolveContext {
    pub pane_pid: u32,
    /// The actual agent process PID found during discovery (descendant of pane_pid).
    pub matched_pid: Option<u32>,
    pub pane_cwd: String,
    pub pane_id: String,
    pub(crate) pane_output: OnceCell<String>,
    pub process_start_time: Option<u64>,
    /// Descendant processes of the matched agent pid: (pid, comm).
    pub process_descendants: Vec<(String, String)>,
}

impl ResolveContext {
    pub fn new(
        pane_pid: u32,
        pane_cwd: String,
        pane_id: String,
        process_start_time: Option<u64>,
        matched_pid: Option<u32>,
        process_descendants: Vec<(String, String)>,
    ) -> Self {
        Self {
            pane_pid,
            matched_pid,
            pane_cwd,
            pane_id,
            pane_output: OnceCell::new(),
            process_start_time,
            process_descendants,
        }
    }

    /// Lazy fetch pane output via tmux capture-pane. Only called if needed.
    pub fn pane_output(&self) -> &str {
        self.pane_output.get_or_init(|| {
            Command::new("tmux")
                .args(["capture-pane", "-p", "-t", &self.pane_id])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default()
        })
    }
}

/// Trait for status resolution strategies. Each provider returns an ordered list.
/// First resolver to return Some(status) wins.
pub trait StatusResolver: Send + Sync {
    /// Try to resolve status. None = can't determine, try next resolver.
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus>;
}
