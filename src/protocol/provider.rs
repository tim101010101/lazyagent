use std::path::Path;

use super::{status::StatusResolver, ExecPlan, ProviderManifest};

pub trait Provider: Send + Sync {
    fn manifest(&self) -> ProviderManifest;
    fn match_process(&self, process_name: &str) -> bool;
    fn exec_plan(&self, cwd: &Path) -> ExecPlan;
    fn resolvers(&self) -> &[Box<dyn StatusResolver>];
}
