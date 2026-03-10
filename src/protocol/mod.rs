pub mod binding;
mod provider;
pub mod status;

pub use provider::Provider;
pub use status::{ResolveContext, StatusResolver};

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionKind {
    Managed,
    Discovered,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Thinking,
    Waiting,
    NeedsInput,
    Idle,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionSource {
    Local,
    Remote { host: String },
}

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub kind: SessionKind,
    pub tmux_session: String,
    pub tmux_pane: String,
    pub provider: String,
    pub cwd: PathBuf,
    pub status: AgentStatus,
    pub started_at: Option<SystemTime>,
    pub source: SessionSource,
    /// Pre-computed git root directory name (resolved in background worker).
    pub git_root: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecPlan {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ProviderManifest {
    pub id: String,
    pub name: String,
}
