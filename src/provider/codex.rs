use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing::{debug, trace};

use crate::protocol::{
    find_open_jsonl, AgentStatus, ExecPlan, Provider, ProviderManifest, ResolveContext,
    StatusResolver,
};

// ===== Codex JSONL Resolver (lsof + cache) =====

struct CodexJsonlResolver {
    cache: Mutex<HashMap<u32, PathBuf>>,
}

impl CodexJsonlResolver {
    fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Read last line of rollout JSONL and determine status.
    fn parse_rollout_status(rollout_path: &Path) -> Option<AgentStatus> {
        let content = std::fs::read_to_string(rollout_path).ok()?;
        let last_line = content.lines().rev().find(|l| !l.trim().is_empty())?;

        let v: serde_json::Value = serde_json::from_str(last_line).ok()?;
        let event_type = v.get("type").and_then(|t| t.as_str());

        match event_type {
            Some(t) if t.contains("task_complete") => Some(AgentStatus::Waiting),
            Some(t) if t.contains("task_started") => Some(AgentStatus::Thinking),
            Some(t) if t.starts_with("response_item") => Some(AgentStatus::Thinking),
            Some(t) if t.contains("event_msg") && t.contains("task_complete") => {
                Some(AgentStatus::Waiting)
            }
            _ => None,
        }
    }
}

impl StatusResolver for CodexJsonlResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);

        // Check cache first
        let cached = self.cache.lock().ok()?.get(&pid).cloned();
        if let Some(ref path) = cached {
            let status = Self::parse_rollout_status(path);
            debug!(?status, path = %path.display(), pid, "codex status from cache");
            return status;
        }

        // lsof lookup
        let path = find_open_jsonl(pid)?;
        debug!(pid, path = %path.display(), "codex jsonl discovered via lsof");

        // Cache the path
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(pid, path.clone());
        }

        let status = Self::parse_rollout_status(&path);
        debug!(?status, path = %path.display(), pid, "codex status resolved");
        status
    }
}

// ===== Provider =====

pub struct CodexProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl CodexProvider {
    pub fn new() -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![Box::new(CodexJsonlResolver::new())];
        Self { resolvers }
    }
}

impl Provider for CodexProvider {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            id: "codex".into(),
            name: "Codex CLI".into(),
        }
    }

    fn match_process(&self, process_name: &str) -> bool {
        let name = Path::new(process_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(process_name);
        let matched = name == "codex";
        trace!(process_name, matched, "codex match_process");
        matched
    }

    fn resolvers(&self) -> &[Box<dyn StatusResolver>] {
        &self.resolvers
    }

    fn exec_plan(&self, cwd: &Path) -> ExecPlan {
        ExecPlan {
            program: "codex".into(),
            args: vec![],
            cwd: Some(cwd.to_string_lossy().into()),
            env: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_process() {
        let p = CodexProvider::new();
        assert!(p.match_process("codex"));
        assert!(!p.match_process("claude"));
        assert!(!p.match_process("node"));
    }

    #[test]
    fn test_match_process_full_path() {
        let p = CodexProvider::new();
        assert!(p.match_process("/usr/local/bin/codex"));
        assert!(p.match_process("/home/user/.npm/bin/codex"));
    }

    #[test]
    fn test_parse_rollout_status_task_complete() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_codex_complete.jsonl");
        std::fs::write(&path, r#"{"type":"task_complete","data":{}}"#).unwrap();
        assert_eq!(
            CodexJsonlResolver::parse_rollout_status(&path),
            Some(AgentStatus::Waiting)
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_parse_rollout_status_task_started() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_codex_started.jsonl");
        std::fs::write(&path, r#"{"type":"task_started","data":{}}"#).unwrap();
        assert_eq!(
            CodexJsonlResolver::parse_rollout_status(&path),
            Some(AgentStatus::Thinking)
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_parse_rollout_status_response_item() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_codex_response.jsonl");
        std::fs::write(&path, r#"{"type":"response_item.created","data":{}}"#).unwrap();
        assert_eq!(
            CodexJsonlResolver::parse_rollout_status(&path),
            Some(AgentStatus::Thinking)
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_resolver_uses_matched_pid() {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, Some(48610));
        assert_eq!(ctx.matched_pid, Some(48610));
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);
        assert_eq!(pid, 48610);
    }

    #[test]
    fn test_resolver_falls_back_to_pane_pid() {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, None);
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);
        assert_eq!(pid, 1234);
    }

    #[test]
    fn test_resolver_cache_hit() {
        // Pre-populate cache with a known path, verify resolver reads from it
        let dir = std::env::temp_dir();
        let path = dir.join("test_codex_cache_hit.jsonl");
        std::fs::write(&path, r#"{"type":"task_complete","data":{}}"#).unwrap();

        let resolver = CodexJsonlResolver::new();
        // Inject into cache — simulates a previous lsof discovery
        resolver.cache.lock().unwrap().insert(42, path.clone());

        let ctx = ResolveContext::new(1, "/tmp".into(), "%0".into(), None, Some(42));
        let status = resolver.resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));

        // Update file content, cache still points to same path
        std::fs::write(&path, r#"{"type":"task_started","data":{}}"#).unwrap();
        let status = resolver.resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Thinking));

        std::fs::remove_file(&path).ok();
    }
}
