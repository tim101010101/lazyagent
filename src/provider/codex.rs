use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use tracing::{debug, trace, warn};

use crate::protocol::{
    AgentStatus, ExecPlan, Provider, ProviderManifest, ResolveContext, StatusResolver,
};

// ===== lsof-based JSONL discovery (Codex holds files open, unlike Claude) =====

fn find_open_jsonl(pid: u32) -> Option<PathBuf> {
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

// ===== Codex JSONL Resolver (lsof → SQLite fallback, with cache) =====

struct CodexJsonlResolver {
    cache: Mutex<HashMap<u32, PathBuf>>,
}

impl CodexJsonlResolver {
    fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Query sqlite3 for the rollout_path associated with a PID.
    fn query_rollout_path(pid: u32) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let db_path = home.join(".codex").join("state_5.sqlite");
        if !db_path.exists() {
            return None;
        }

        let query = format!(
            "SELECT t.rollout_path FROM logs l \
             JOIN threads t ON l.thread_id = t.id \
             WHERE l.process_uuid GLOB 'pid:{}:*' \
             AND l.thread_id IS NOT NULL \
             ORDER BY l.id DESC LIMIT 1",
            pid
        );

        let output = Command::new("sqlite3")
            .args(["-readonly", db_path.to_str()?, &query])
            .output()
            .ok()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(pid, stderr = %stderr, "sqlite3 query failed");
            return None;
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if result.is_empty() {
            None
        } else {
            Some(PathBuf::from(result))
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

    /// Find rollout path: cache → lsof → SQLite.
    fn find_rollout_path(&self, pid: u32) -> Option<PathBuf> {
        // Check cache
        if let Some(path) = self.cache.lock().ok()?.get(&pid).cloned() {
            return Some(path);
        }

        // Try lsof
        let path = find_open_jsonl(pid).or_else(|| {
            // Fallback: SQLite query
            let p = Self::query_rollout_path(pid)?;
            debug!(pid, path = %p.display(), "codex rollout via sqlite");
            Some(p)
        })?;

        // Cache the result
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(pid, path.clone());
        }

        Some(path)
    }
}

impl StatusResolver for CodexJsonlResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);
        let path = self.find_rollout_path(pid)?;
        let status = Self::parse_rollout_status(&path);
        debug!(?status, path = %path.display(), pid, "codex status resolved");
        status
    }
}

// ===== Pane Output Fallback (idle/prompt detection) =====

struct CodexPaneResolver;

impl StatusResolver for CodexPaneResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let output = ctx.pane_output();
        let lines: Vec<&str> = output
            .lines()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(5)
            .collect();

        for line in &lines {
            let trimmed = line.trim();
            // Codex uses › (U+203A) for its idle prompt
            if trimmed.starts_with('›') || trimmed.starts_with('>') {
                return Some(AgentStatus::Waiting);
            }
            if trimmed.contains("esc to interrupt") || trimmed.contains("working") {
                return Some(AgentStatus::Thinking);
            }
        }

        None
    }
}

// ===== Provider =====

pub struct CodexProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl CodexProvider {
    pub fn new() -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![
            Box::new(CodexJsonlResolver::new()),
            Box::new(CodexPaneResolver),
        ];
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
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, Some(48610), vec![]);
        assert_eq!(ctx.matched_pid, Some(48610));
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);
        assert_eq!(pid, 48610);
    }

    #[test]
    fn test_resolver_falls_back_to_pane_pid() {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, None, vec![]);
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);
        assert_eq!(pid, 1234);
    }

    #[test]
    fn test_resolver_cache_hit() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_codex_cache_hit.jsonl");
        std::fs::write(&path, r#"{"type":"task_complete","data":{}}"#).unwrap();

        let resolver = CodexJsonlResolver::new();
        resolver.cache.lock().unwrap().insert(42, path.clone());

        let ctx = ResolveContext::new(1, "/tmp".into(), "%0".into(), None, Some(42), vec![]);
        let status = resolver.resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));

        std::fs::write(&path, r#"{"type":"task_started","data":{}}"#).unwrap();
        let status = resolver.resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Thinking));

        std::fs::remove_file(&path).ok();
    }

    fn mock_pane_ctx(output: &str) -> ResolveContext {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, None, vec![]);
        let _ = ctx.pane_output.set(output.to_string());
        ctx
    }

    #[test]
    fn test_pane_resolver_waiting_prompt() {
        let r = CodexPaneResolver;
        let ctx = mock_pane_ctx("  gpt-5.4 xhigh · 100% left · ~/.dotfiles\n\n› ");
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_pane_resolver_waiting_with_text() {
        let r = CodexPaneResolver;
        let ctx = mock_pane_ctx("› Write tests for @filename");
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_pane_resolver_no_match() {
        let r = CodexPaneResolver;
        let ctx = mock_pane_ctx("random output");
        assert_eq!(r.resolve(&ctx), None);
    }
}
