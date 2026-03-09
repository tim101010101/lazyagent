use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use tracing::{debug, trace};

use crate::protocol::{
    AgentStatus, ExecPlan, LineMatcher, Provider, ProviderManifest, ResolveContext, StatusResolver,
    StatusRule, TextMatchResolver,
};

// ===== Claude JSONL Resolver (Tier 1) =====

/// Resolves status from Claude Code's native JSONL session files.
/// Path: ~/.claude/projects/<encoded-cwd>/<session-id>.jsonl
struct ClaudeJsonlResolver;

/// Max age (in seconds) for a JSONL file to be considered active.
const JSONL_ACTIVE_WINDOW_SECS: u64 = 60;

impl ClaudeJsonlResolver {
    /// Encode cwd to Claude's project dir format: /foo/bar → -foo-bar
    fn encode_cwd(cwd: &str) -> String {
        cwd.replace('/', "-")
    }

    /// Find the best JSONL file for a given cwd.
    /// Returns None if ambiguous (multiple recently-modified files, no cmdline hint).
    fn find_session_jsonl(cwd: &str) -> Option<std::path::PathBuf> {
        let home = dirs::home_dir()?;
        let encoded = Self::encode_cwd(cwd);
        let project_dir = home.join(".claude").join("projects").join(&encoded);

        if !project_dir.is_dir() {
            trace!(cwd, encoded, "no claude project dir found");
            return None;
        }

        let now = SystemTime::now();
        let mut candidates: Vec<(std::path::PathBuf, SystemTime)> = Vec::new();

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

        if candidates.len() == 1 {
            return Some(candidates.into_iter().next().unwrap().0);
        }

        // No recently-active file — find most recently modified overall
        let mut best: Option<(std::path::PathBuf, SystemTime)> = None;
        let entries = fs::read_dir(&project_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(meta) = path.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if best.as_ref().map_or(true, |(_, t)| modified > *t) {
                            best = Some((path, modified));
                        }
                    }
                }
            }
        }

        best.map(|(p, _)| p)
    }

    /// Read the last non-empty line of a file efficiently.
    fn read_last_line(path: &Path) -> Option<String> {
        let content = fs::read_to_string(path).ok()?;
        content
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|s| s.to_string())
    }

    /// Tool names that indicate human-in-the-loop interaction.
    const INTERACTIVE_TOOLS: &'static [&'static str] = &["AskUserQuestion"];

    /// Parse JSONL last line to determine status.
    fn parse_status(line: &str) -> Option<AgentStatus> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let msg_type = v.get("type")?.as_str()?;

        match msg_type {
            "user" => {
                // Check if this is a tool_result (user answered)
                if Self::is_tool_result(&v) {
                    Some(AgentStatus::Thinking) // User answered, AI processing
                } else {
                    Some(AgentStatus::Thinking) // Regular user input
                }
            }
            "assistant" => {
                let stop_reason = v
                    .get("message")
                    .and_then(|m| m.get("stop_reason"))
                    .and_then(|s| s.as_str());
                match stop_reason {
                    Some("end_turn") => Some(AgentStatus::Waiting),
                    Some("tool_use") => {
                        // Check if the tool is interactive (needs human input)
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
    /// Scans last 100 lines to find most recent AskUserQuestion and tool_result.
    fn parse_status_with_history(jsonl_path: &Path) -> Option<AgentStatus> {
        let content = fs::read_to_string(jsonl_path).ok()?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

        if lines.is_empty() {
            return None;
        }

        // Scan last 100 lines for performance
        let scan_limit = 100.min(lines.len());
        let mut last_ask_idx: Option<usize> = None;
        let mut last_result_idx: Option<usize> = None;

        for (i, line) in lines.iter().enumerate().rev().take(scan_limit) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                // Find most recent AskUserQuestion
                if last_ask_idx.is_none() && Self::is_ask_user_question(&v) {
                    last_ask_idx = Some(i);
                }
                // Find most recent tool_result
                if last_result_idx.is_none() && Self::is_tool_result(&v) {
                    last_result_idx = Some(i);
                }
                // Stop when both found
                if last_ask_idx.is_some() && last_result_idx.is_some() {
                    break;
                }
            }
        }

        // Determine status based on message order
        match (last_ask_idx, last_result_idx) {
            // AskUserQuestion exists but no subsequent tool_result → waiting for answer
            (Some(_), None) => Some(AgentStatus::NeedsInput),
            // AskUserQuestion after tool_result → waiting for answer
            (Some(ask_idx), Some(result_idx)) if ask_idx > result_idx => {
                Some(AgentStatus::NeedsInput)
            }
            // Otherwise: fallback to single-line parsing
            _ => Self::parse_status(lines.last()?),
        }
    }
}

impl StatusResolver for ClaudeJsonlResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let jsonl_path = Self::find_session_jsonl(&ctx.pane_cwd)?;

        // Prioritize history analysis over single-line parsing
        let status = Self::parse_status_with_history(&jsonl_path);

        debug!(?status, path = %jsonl_path.display(), "jsonl status resolved");
        status
    }
}

// ===== Text Match Rules (Tier 2) =====

static CLAUDE_TEXT_RULES: &[StatusRule] = &[
    // NeedsInput: interactive prompts at terminal bottom only (last 3 lines)
    StatusRule {
        status: AgentStatus::NeedsInput,
        matcher: LineMatcher::Contains("Enter to select"),
        max_line: Some(3),
    },
    // Thinking: active processing (scan full window)
    StatusRule {
        status: AgentStatus::Thinking,
        matcher: LineMatcher::EndsWith("thinking)"),
        max_line: None,
    },
    StatusRule {
        status: AgentStatus::Thinking,
        matcher: LineMatcher::Contains("esc to interrupt"),
        max_line: None,
    },
    // Waiting: idle prompt (scan full window)
    StatusRule {
        status: AgentStatus::Waiting,
        matcher: LineMatcher::StartsWith("❯"),
        max_line: None,
    },
];

// ===== Provider =====

pub struct ClaudeProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![
            Box::new(ClaudeJsonlResolver),
            Box::new(TextMatchResolver::new(
                CLAUDE_TEXT_RULES,
                20,
                AgentStatus::Unknown,
            )),
        ];

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
    use crate::protocol::ResolveContext;

    fn mock_context(pane_output: &str) -> ResolveContext {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None);
        let _ = ctx.pane_output.set(pane_output.to_string());
        ctx
    }

    #[test]
    fn test_match_process() {
        let p = ClaudeProvider::new();
        assert!(p.match_process("claude"));
        assert!(p.match_process("claude-code"));
        assert!(!p.match_process("aider"));
        assert!(!p.match_process("node"));
    }

    // Text matcher tests (resolver index 1 = TextMatchResolver)
    #[test]
    fn test_text_status_waiting() {
        let p = ClaudeProvider::new();
        let output = "✻ Worked for 2m 10s\n\n────────── ▪▪▪ ─\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions\n";
        let ctx = mock_context(output);
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_text_status_thinking() {
        let p = ClaudeProvider::new();
        let output = "✽ Gitifying… (1m 7s · ↑ 856 tokens · thinking)\n\n────────── ▪▪▪ ─\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions · esc to interrupt\n";
        let ctx = mock_context(output);
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_text_status_error_no_false_positive() {
        let p = ClaudeProvider::new();
        let output = "  error: nix eval failed\n\n✻ Worked for 1m\n\n────────── ▪▪▪ ─\n❯\u{a0}\n";
        let ctx = mock_context(output);
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_text_status_waiting_after_clear() {
        let p = ClaudeProvider::new();
        let mut output = String::from(
            "Claude Code v2.1.70\n\n❯ /clear\n  (no content)\n\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions on\n",
        );
        for _ in 0..40 {
            output.push('\n');
        }
        let ctx = mock_context(&output);
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_text_status_unknown() {
        let p = ClaudeProvider::new();
        let ctx = mock_context("random output");
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Unknown));
    }

    #[test]
    fn test_text_status_needs_input_select() {
        let p = ClaudeProvider::new();
        let output = "Which option?\n 1. Option A\n 2. Option B\nEnter to select · Tab/Arrow keys to navigate · Esc to cancel\n";
        let ctx = mock_context(output);
        let status = p.resolvers()[1].resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::NeedsInput));
    }

    // JSONL resolver tests
    #[test]
    fn test_encode_cwd() {
        assert_eq!(
            ClaudeJsonlResolver::encode_cwd("/Users/didi/Code/project"),
            "-Users-didi-Code-project"
        );
        assert_eq!(ClaudeJsonlResolver::encode_cwd("/tmp"), "-tmp");
    }

    #[test]
    fn test_parse_status_user() {
        let line = r#"{"type":"user","message":{"role":"user"}}"#;
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
        // Regular tool_use (no interactive tool) → Thinking
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"Read","id":"123"}]}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Thinking)
        );
    }

    #[test]
    fn test_parse_status_ask_user_question() {
        // AskUserQuestion tool_use → NeedsInput
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"AskUserQuestion","id":"456","input":{}}]}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::NeedsInput)
        );
    }

    #[test]
    fn test_parse_status_tool_use_no_content() {
        // tool_use with no content blocks → Thinking (fallback)
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
    fn test_parse_status_invalid_json() {
        assert_eq!(ClaudeJsonlResolver::parse_status("not json"), None);
    }

    #[test]
    fn test_exec_plan() {
        let p = ClaudeProvider::new();
        let plan = p.exec_plan(Path::new("/home/user/project"));
        assert_eq!(plan.program, "claude");
        assert!(plan.args.is_empty());
        assert_eq!(plan.cwd.as_deref(), Some("/home/user/project"));
    }

    #[test]
    fn test_manifest() {
        let p = ClaudeProvider::new();
        let m = p.manifest();
        assert_eq!(m.id, "claude");
        assert_eq!(m.name, "Claude Code");
    }

    #[test]
    fn test_resolver_count() {
        let p = ClaudeProvider::new();
        assert_eq!(p.resolvers().len(), 2, "should have JSONL + text resolvers");
    }

    // History-based status detection tests
    #[test]
    fn test_parse_status_tool_result() {
        // tool_result message → Thinking (user answered)
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"123"}]}}"#;
        assert_eq!(
            ClaudeJsonlResolver::parse_status(line),
            Some(AgentStatus::Thinking)
        );
    }

    #[test]
    fn test_is_ask_user_question() {
        let line = r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","name":"AskUserQuestion"}]}}"#;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(ClaudeJsonlResolver::is_ask_user_question(&v));

        // Non-interactive tool
        let line = r#"{"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","name":"Read"}]}}"#;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(!ClaudeJsonlResolver::is_ask_user_question(&v));

        // User message
        let line = r#"{"type":"user","message":{"content":[]}}"#;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(!ClaudeJsonlResolver::is_ask_user_question(&v));
    }

    #[test]
    fn test_is_tool_result() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"123"}]}}"#;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(ClaudeJsonlResolver::is_tool_result(&v));

        // Regular user message
        let line = r#"{"type":"user","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(!ClaudeJsonlResolver::is_tool_result(&v));

        // Assistant message
        let line = r#"{"type":"assistant","message":{"content":[]}}"#;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(!ClaudeJsonlResolver::is_tool_result(&v));
    }

    #[test]
    fn test_parse_status_with_history_unanswered() {
        // AskUserQuestion with no subsequent tool_result → NeedsInput
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_unanswered.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion"}}]}}}}"#
        )
        .unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::NeedsInput));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_answered() {
        // AskUserQuestion followed by tool_result → Thinking
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_answered.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"123"}}]}}}}"#
        )
        .unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::Thinking));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_new_question_after_answer() {
        // tool_result followed by new AskUserQuestion → NeedsInput
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_new_question.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"123"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"tool_use","content":[{{"type":"tool_use","name":"AskUserQuestion"}}]}}}}"#
        )
        .unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::NeedsInput));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_no_interactive_tools() {
        // No AskUserQuestion, last line is end_turn → Waiting
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_no_interactive.jsonl");
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
        let jsonl_path = temp_dir.join("test_empty.jsonl");
        let _file = std::fs::File::create(&jsonl_path).unwrap();

        let status = ClaudeJsonlResolver::parse_status_with_history(&jsonl_path);
        assert_eq!(status, None);

        std::fs::remove_file(jsonl_path).ok();
    }
}
