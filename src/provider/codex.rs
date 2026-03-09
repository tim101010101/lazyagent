use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use tracing::{debug, trace, warn};

use crate::protocol::{
    AgentStatus, ExecPlan, LineMatcher, Provider, ProviderManifest, ResolveContext, StatusResolver,
    StatusRule, TextMatchResolver,
};

// ===== Codex SQLite Resolver (Tier 1) =====

/// Resolves status from Codex CLI's native SQLite state database.
/// Path: ~/.codex/state_5.sqlite
/// Uses sqlite3 CLI to avoid adding rusqlite dependency.
struct CodexSqliteResolver;

impl CodexSqliteResolver {
    /// Get the path to the Codex state database.
    fn db_path() -> Option<std::path::PathBuf> {
        let home = dirs::home_dir()?;
        let path = home.join(".codex").join("state_5.sqlite");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Query sqlite3 for the rollout_path associated with a PID.
    fn query_rollout_path(db_path: &Path, pane_pid: u32) -> Option<String> {
        let query = format!(
            "SELECT t.rollout_path FROM logs l \
             JOIN threads t ON l.thread_id = t.id \
             WHERE l.process_uuid GLOB 'pid:{}:*' \
             AND l.thread_id IS NOT NULL \
             ORDER BY l.id DESC LIMIT 1",
            pane_pid
        );

        let output = Command::new("sqlite3")
            .args(["-readonly", db_path.to_str()?, &query])
            .output()
            .ok()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(pid = pane_pid, stderr = %stderr, "sqlite3 query failed");
            return None;
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Read last line of rollout JSONL and determine status.
    fn parse_rollout_status(rollout_path: &str) -> Option<AgentStatus> {
        let content = std::fs::read_to_string(rollout_path).ok()?;
        let last_line = content.lines().rev().find(|l| !l.trim().is_empty())?;

        let v: serde_json::Value = serde_json::from_str(last_line).ok()?;

        // Codex rollout events use type field or nested event structure
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

impl StatusResolver for CodexSqliteResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let db_path = Self::db_path()?;
        let rollout_path = Self::query_rollout_path(&db_path, ctx.pane_pid)?;
        let status = Self::parse_rollout_status(&rollout_path);
        debug!(?status, rollout_path, pid = ctx.pane_pid, "codex sqlite status resolved");
        status
    }
}

// ===== Text Match Rules (Tier 2) =====

static CODEX_TEXT_RULES: &[StatusRule] = &[
    StatusRule {
        status: AgentStatus::Thinking,
        matcher: LineMatcher::Contains("Thinking"),
        max_line: None,
    },
    StatusRule {
        status: AgentStatus::Thinking,
        matcher: LineMatcher::Contains("Running"),
        max_line: None,
    },
    StatusRule {
        status: AgentStatus::Waiting,
        matcher: LineMatcher::StartsWith(">"),
        max_line: None,
    },
    StatusRule {
        status: AgentStatus::Waiting,
        matcher: LineMatcher::Contains("Enter a prompt"),
        max_line: None,
    },
];

// ===== Provider =====

pub struct CodexProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl CodexProvider {
    pub fn new() -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![
            Box::new(CodexSqliteResolver),
            Box::new(TextMatchResolver::new(
                CODEX_TEXT_RULES,
                20,
                AgentStatus::Unknown,
            )),
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
    use crate::protocol::ResolveContext;

    fn mock_context(pane_output: &str) -> ResolveContext {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None);
        let _ = ctx.pane_output.set(pane_output.to_string());
        ctx
    }

    #[test]
    fn test_match_process() {
        let p = CodexProvider::new();
        assert!(p.match_process("codex"));
        assert!(p.match_process("/Users/didi/.bun/install/global/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/codex/codex"));
        assert!(!p.match_process("claude"));
        assert!(!p.match_process("node"));
    }

    #[test]
    fn test_manifest() {
        let p = CodexProvider::new();
        let m = p.manifest();
        assert_eq!(m.id, "codex");
        assert_eq!(m.name, "Codex CLI");
    }

    #[test]
    fn test_exec_plan() {
        let p = CodexProvider::new();
        let plan = p.exec_plan(Path::new("/home/user/project"));
        assert_eq!(plan.program, "codex");
        assert!(plan.args.is_empty());
        assert_eq!(plan.cwd.as_deref(), Some("/home/user/project"));
    }

    #[test]
    fn test_resolver_count() {
        let p = CodexProvider::new();
        assert_eq!(p.resolvers().len(), 2, "should have sqlite + text resolvers");
    }

    // Text matcher tests
    #[test]
    fn test_text_status_thinking() {
        let p = CodexProvider::new();
        let ctx = mock_context("Thinking...\nsome output");
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_text_status_waiting() {
        let p = CodexProvider::new();
        let ctx = mock_context("Done.\n> ");
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_text_status_waiting_prompt() {
        let p = CodexProvider::new();
        let ctx = mock_context("Enter a prompt (or type /help)");
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_text_status_unknown() {
        let p = CodexProvider::new();
        let ctx = mock_context("random output");
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Unknown));
    }

    #[test]
    fn test_db_path_nonexistent() {
        // When ~/.codex/state_5.sqlite doesn't exist, should return None
        // This is a basic sanity check - actual db presence varies
        let path = CodexSqliteResolver::db_path();
        // Just verify it doesn't panic
        let _ = path;
    }
}
