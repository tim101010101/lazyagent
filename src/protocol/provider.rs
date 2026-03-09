use std::path::Path;

use super::{AgentStatus, ExecPlan, ProviderManifest};

pub trait Provider: Send + Sync {
    fn manifest(&self) -> ProviderManifest;
    fn detect_status(&self, pane_output: &str) -> AgentStatus;
    fn match_process(&self, process_name: &str) -> bool;
    fn exec_plan(&self, cwd: &Path) -> ExecPlan;
}
