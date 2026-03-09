use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use tracing::{debug, trace};

use crate::protocol::{
    find_open_jsonl, AgentStatus, ExecPlan, Provider, ProviderManifest, ResolveContext,
    StatusResolver,
};

// ===== Claude JSONL Resolver (lsof + cache) =====

/// Max age (in seconds) for a JSONL file to be considered active.
const JSONL_ACTIVE_WINDOW_SECS: u64 = 60;

struct ClaudeJsonlResolver {
    cache: Mutex<HashMap<u32, PathBuf>>,
}

impl ClaudeJsonlResolver {
    fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Encode cwd to Claude's project dir format: /foo/bar → -foo-bar
    fn encode_cwd(cwd: &str) -> String {
        cwd.replace('/', "-")
    }

    /// Find active JSONL file for a given cwd (no stale fallback).
    /// Returns None if no recently-modified file or ambiguous.
    fn find_session_jsonl(cwd: &str) -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let encoded = Self::encode_cwd(cwd);
        let project_dir = home.join(".claude").join("projects").join(&encoded);

        if !project_dir.is_dir() {
            trace!(cwd, encoded, "no claude project dir found");
            return None;
        }

        let now = SystemTime::now();
        let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();

        let entries = fs::read_dir(&project_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(meta) = path.metadata() {
                    if let Ok(modified) = meta.modified() {
                        let age = now.duration_since(modified).unwrap_or_default().as_secs();
                        if age <= JSONL_ACTIVE_WINDOW_SECS {
                            candidates.push((path, modified));
                        }
                    }
                }
            }
        }

        if candidates.len() > 1 {
            debug!(
                cwd,
                count = candidates.len(),
                "ambiguous: multiple active JSONL files"
            );
            return None;
        }

        candidates.into_iter().next().map(|(p, _)| p)
    }

    /// Tool names that indicate human-in-the-loop interaction.
    const INTERACTIVE_TOOLS: &'static [&'static str] = &["AskUserQuestion"];

    /// Parse JSONL last line to determine status.
    fn parse_status(line: &str) -> Option<AgentStatus> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let msg_type = v.get("type")?.as_str()?;

        match msg_type {
            "user" => Some(AgentStatus::Thinking),
            "assistant" => {
                let stop_reason = v
                    .get("message")
                    .and_then(|m| m.get("stop_reason"))
                    .and_then(|s| s.as_str());
                match stop_reason {
                    Some("end_turn") => Some(AgentStatus::Waiting),
                    Some("tool_use") => {
                        if Self::has_interactive_tool(&v) {
                            Some(AgentStatus::NeedsInput)
                        } else {
                            Some(AgentStatus::Thinking)
                        }
                    }
                    _ => None,
                }
            }
            "progress" => Some(AgentStatus::Thinking),
            _ => None,
        }
    }

    /// Check if assistant message contains an interactive tool_use block.
    fn has_interactive_tool(v: &serde_json::Value) -> bool {
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array());
        if let Some(blocks) = content {
            for block in blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                        if Self::INTERACTIVE_TOOLS.contains(&name) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Check if message is an AskUserQuestion tool_use.
    fn is_ask_user_question(v: &serde_json::Value) -> bool {
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            return false;
        }
        if v.get("message")
            .and_then(|m| m.get("stop_reason"))
            .and_then(|s| s.as_str())
            != Some("tool_use")
        {
            return false;
        }
        Self::has_interactive_tool(v)
    }

    /// Check if message is a tool_result (user response).
    fn is_tool_result(v: &serde_json::Value) -> bool {
        if v.get("type").and_then(|t| t.as_str()) != Some("user") {
            return false;
        }
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array());
        if let Some(blocks) = content {
            return blocks
                .iter()
                .any(|block| block.get("type").and_then(|t| t.as_str()) == Some("tool_result"));
        }
        false
    }

    /// Analyze conversation history to determine if waiting for user input.
    fn parse_status_with_history(jsonl_path: &Path) -> Option<AgentStatus> {
        let content = fs::read_to_string(jsonl_path).ok()?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

        if lines.is_empty() {
            return None;
        }

        let scan_limit = 100.min(lines.len());
        let mut last_ask_idx: Option<usize> = None;
        let mut last_result_idx: Option<usize> = None;

        for (i, line) in lines.iter().enumerate().rev().take(scan_limit) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if last_ask_idx.is_none() && Self::is_ask_user_question(&v) {
                    last_ask_idx = Some(i);
                }
                if last_result_idx.is_none() && Self::is_tool_result(&v) {
                    last_result_idx = Some(i);
                }
                if last_ask_idx.is_some() && last_result_idx.is_some() {
                    break;
                }
            }
        }

        match (last_ask_idx, last_result_idx) {
            (Some(_), None) => Some(AgentStatus::NeedsInput),
            (Some(ask_idx), Some(result_idx)) if ask_idx > result_idx => {
                Some(AgentStatus::NeedsInput)
            }
            _ => Self::parse_status(lines.last()?),
        }
    }
}

impl StatusResolver for ClaudeJsonlResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let pid = ctx.matched_pid.unwrap_or(ctx.pane_pid);

        // Check cache first
        let cached = self.cache.lock().ok()?.get(&pid).cloned();
        if let Some(ref path) = cached {
            let status = Self::parse_status_with_history(path);
            debug!(?status, path = %path.display(), pid, "claude status from cache");
            return status;
        }

        // lsof lookup
        if let Some(path) = find_open_jsonl(pid) {
            debug!(pid, path = %path.display(), "claude jsonl discovered via lsof");
            if let Ok(mut cache) = self.cache.lock() {
                cache.insert(pid, path.clone());
            }
            let status = Self::parse_status_with_history(&path);
            debug!(?status, path = %path.display(), pid, "claude status resolved");
            return status;
        }

        // Fallback: CWD-based lookup (active files only, no stale)
        if let Some(path) = Self::find_session_jsonl(&ctx.pane_cwd) {
            let status = Self::parse_status_with_history(&path);
            debug!(?status, path = %path.display(), "claude status via cwd fallback");
            return status;
        }

        None
    }
}

// ===== Provider =====

pub struct ClaudeProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![Box::new(ClaudeJsonlResolver::new())];
        Self { resolvers }
    }
}

impl Provider for ClaudeProvider {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            id: "claude".into(),
            name: "Claude Code".into(),
        }
    }

    fn match_process(&self, process_name: &str) -> bool {
        let matched = process_name == "claude" || process_name == "claude-code";
        trace!(process_name, matched, "claude match_process");
        matched
    }

    fn resolvers(&self) -> &[Box<dyn StatusResolver>] {
        &self.resolvers
    }

    fn exec_plan(&self, cwd: &Path) -> ExecPlan {
        ExecPlan {
            program: "claude".into(),
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
        let p = ClaudeProvider::new();
        assert!(p.match_process("claude"));
        assert!(p.match_process("claude-code"));
        assert!(!p.match_process("aider"));
        assert!(!p.match_process("node"));
    }

    #[test]
    fn test_encode_cwd() {
        assert_eq!(
            ClaudeJsonlResolver::encode_cwd("/home/user/Code/project"),
            "-home-user-Code-project"
        );
        assert_eq!(ClaudeJsonlResolver::encode_cwd("/tmp"), "-tmp");
    }

    #[test]
    fn test_parse_status_user() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Thinking)
        );
    }

    #[test]
    fn test_parse_status_assistant_end_turn() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn"}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Waiting)
        );
    }

    #[test]
    fn test_parse_status_assistant_tool_use() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"Read","id":"123"}]}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Thinking)
        );
    }

    #[test]
    fn test_parse_status_ask_user_question() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"AskUserQuestion","id":"456","input":{}}]}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn test_parse_status_tool_use_no_content() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use"}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Thinking)
        );
    }

    #[test]
    fn test_parse_status_progress() {
        let line = r#"{"type":"progress","data":{}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Thinking)
        );
    }

    #[test]
    fn test_parse_status_unknown_type() {
        let line = r#"{"type":"system","data":{}}"#;
        assert_eq!(ClaudeJsonlResolver::parse_status(line), None);
    }

    #[test]
    fn test_parse_status_with_history_needs_input() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_needs_input_lsof.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion","id":"1","input":{{}}}}]}}}}"#
        )
        .unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::NeedsInput));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_answered() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_answered_lsof.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion","id":"1","input":{{}}}}]}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"1","content":"yes"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"end_turn"}}}}"#
        )
        .unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::Waiting));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_no_interactive_tools() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_no_interactive_lsof.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"Read"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"end_turn"}}}}"#
        )
        .unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::Waiting));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_empty_file() {
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_empty_lsof.jsonl");
        let _file = std::fs::File::create(&jsonl_path).unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, None);

        std::fs::remove_file(jsonl_path).ok();
    }
}
