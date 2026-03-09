mod provider;

pub use provider::Provider;

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
