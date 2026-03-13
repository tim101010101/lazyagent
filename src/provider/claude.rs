use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tracing::{debug, trace};

use crate::protocol::binding::{SessionBindingStore, SessionKey};
use crate::protocol::{
    AgentStatus, ExecPlan, Provider, ProviderManifest, ResolveContext, StatusResolver,
};

// ===== Shared helpers =====

/// Encode cwd to Claude's project dir format.
/// All non-alphanumeric chars become `-`: /Users/didi/.dotfiles → -Users-didi--dotfiles
fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Resolve the Claude project dir for a given cwd.
fn project_dir_for_cwd(cwd: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let encoded = encode_cwd(cwd);
    let dir = home.join(".claude").join("projects").join(&encoded);
    if dir.is_dir() { Some(dir) } else { None }
}

/// Collect all .jsonl files in a directory.
fn collect_jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }
    files
}

/// Tool names that indicate human-in-the-loop interaction.
const INTERACTIVE_TOOLS: &[&str] = &["AskUserQuestion"];

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
                    if has_interactive_tool(&v) {
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
                    if INTERACTIVE_TOOLS.contains(&name) {
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
    has_interactive_tool(v)
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

/// Check if a user message is a local command record (not real conversation input).
fn is_local_command(v: &serde_json::Value) -> bool {
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    content.contains("<local-command-caveat>")
        || content.contains("<bash-input>")
        || content.contains("<bash-stdout>")
        || content.contains("<bash-stderr>")
        || content.contains("<command-name>")
}

/// Analyze conversation history to determine status.
/// Single reverse pass: tracks AskUserQuestion state + last meaningful event.
fn parse_status_with_history(jsonl_path: &Path) -> Option<AgentStatus> {
    let content = fs::read_to_string(jsonl_path).ok()?;
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        return None;
    }

    let scan_limit = 100.min(lines.len());
    let mut last_ask_idx: Option<usize> = None;
    let mut last_result_idx: Option<usize> = None;
    let mut last_meaningful: Option<AgentStatus> = None;

    for (i, line) in lines.iter().enumerate().rev().take(scan_limit) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

            if last_ask_idx.is_none() && is_ask_user_question(&v) {
                last_ask_idx = Some(i);
            }
            if last_result_idx.is_none() && is_tool_result(&v) {
                last_result_idx = Some(i);
            }

            // First meaningful conversation event (skip local command noise)
            if last_meaningful.is_none()
                && (msg_type == "assistant" || (msg_type == "user" && !is_local_command(&v)))
            {
                last_meaningful = parse_status(line);
            }

            if last_meaningful.is_some()
                && (last_ask_idx.is_some() || last_result_idx.is_some())
            {
                break;
            }
        }
    }

    // NeedsInput takes priority
    match (last_ask_idx, last_result_idx) {
        (Some(_), None) => return Some(AgentStatus::NeedsInput),
        (Some(ask_idx), Some(result_idx)) if ask_idx > result_idx => {
            return Some(AgentStatus::NeedsInput);
        }
        _ => {}
    }

    last_meaningful
}

/// Get file birthtime (creation time) as epoch seconds.
fn file_birthtime_epoch(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let created = meta.created().ok()?;
    created
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Build a SessionKey if both matched_pid and process_start_time are available.
fn make_session_key(ctx: &ResolveContext) -> Option<SessionKey> {
    let pid = ctx.matched_pid?;
    let start = ctx.process_start_time?;
    Some(SessionKey {
        pane_id: ctx.pane_id.clone(),
        matched_pid: pid,
        process_start_time: start,
    })
}

// ===== Resolver 1: BindingResolver =====

struct BindingResolver {
    store: Arc<SessionBindingStore>,
}

impl StatusResolver for BindingResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let key = make_session_key(ctx)?;
        let path = self.store.get(&key)?;

        if !path.exists() {
            debug!(path = %path.display(), "bound jsonl gone, unbinding");
            self.store.unbind(&key);
            return None;
        }

        let status = parse_status_with_history(&path);
        debug!(?status, path = %path.display(), "status from binding");
        status
    }
}

// ===== Resolver 2: JsonlDiscoveryResolver =====

/// Max age (seconds) for a JSONL file to be considered active.
const JSONL_MAX_AGE_SECS: u64 = 300; // 5 minutes

/// Max birthtime delta (seconds) for correlating JSONL to process start.
const BIRTHTIME_TOLERANCE_SECS: u64 = 60;

struct JsonlDiscoveryResolver {
    store: Arc<SessionBindingStore>,
}

impl StatusResolver for JsonlDiscoveryResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let key = make_session_key(ctx)?;

        let project_dir = project_dir_for_cwd(&ctx.pane_cwd)?;
        let all_jsonl = collect_jsonl_files(&project_dir);

        if all_jsonl.is_empty() {
            return None;
        }

        // Filter to recently-modified files only
        let now = SystemTime::now();
        let candidates: Vec<PathBuf> = all_jsonl
            .into_iter()
            .filter(|p| {
                fs::metadata(p)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|mtime| now.duration_since(mtime).ok())
                    .map(|age| age.as_secs() <= JSONL_MAX_AGE_SECS)
                    .unwrap_or(false)
            })
            .collect();

        trace!(
            count = candidates.len(),
            dir = %project_dir.display(),
            "jsonl candidates after mtime filter"
        );

        match candidates.len() {
            0 => None,
            1 => {
                let path = &candidates[0];
                if self.store.is_bound_by_other(path, &key) {
                    trace!(path = %path.display(), "single candidate bound by other session");
                    return None;
                }
                self.store.bind(key, path.clone());
                let status = parse_status_with_history(path);
                debug!(?status, path = %path.display(), "status from jsonl discovery (single)");
                status
            }
            _ => {
                // Multiple candidates: try birthtime correlation
                let start = ctx.process_start_time?;
                let matched: Vec<&PathBuf> = candidates
                    .iter()
                    .filter(|p| {
                        file_birthtime_epoch(p)
                            .map(|bt| {
                                let delta = bt.abs_diff(start);
                                delta <= BIRTHTIME_TOLERANCE_SECS
                            })
                            .unwrap_or(false)
                    })
                    .collect();

                if matched.len() == 1 {
                    let path = matched[0];
                    if self.store.is_bound_by_other(path, &key) {
                        trace!(path = %path.display(), "birthtime match bound by other");
                        return None;
                    }
                    self.store.bind(key, path.clone());
                    let status = parse_status_with_history(path);
                    debug!(?status, path = %path.display(), "status from jsonl discovery (birthtime)");
                    status
                } else {
                    debug!(
                        candidates = candidates.len(),
                        birthtime_matches = matched.len(),
                        "ambiguous jsonl, deferring"
                    );
                    None
                }
            }
        }
    }
}

// ===== Resolver 3: CaffeinateResolver =====

struct CaffeinateResolver;

impl StatusResolver for CaffeinateResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        ctx.matched_pid?;

        let has_caffeinate = ctx
            .process_descendants
            .iter()
            .any(|(_, comm)| comm.contains("caffeinate"));

        if has_caffeinate {
            Some(AgentStatus::Thinking)
        } else {
            Some(AgentStatus::Waiting)
        }
    }
}

// ===== Resolver 4: ClaudePaneResolver (unchanged) =====

struct ClaudePaneResolver;

impl StatusResolver for ClaudePaneResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let output = ctx.pane_output();
        let lines: Vec<&str> = output
            .lines()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(10)
            .collect();

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.contains("esc to interrupt") {
                return Some(AgentStatus::Thinking);
            }
            if trimmed.ends_with("thinking)") {
                return Some(AgentStatus::Thinking);
            }
            if trimmed.contains("Enter to select") {
                return Some(AgentStatus::NeedsInput);
            }
            if trimmed.starts_with('❯') {
                return Some(AgentStatus::Waiting);
            }
        }

        None
    }
}

// ===== Provider =====

pub struct ClaudeProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl ClaudeProvider {
    pub fn new(store: Arc<SessionBindingStore>) -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![
            Box::new(BindingResolver {
                store: Arc::clone(&store),
            }),
            Box::new(JsonlDiscoveryResolver {
                store: Arc::clone(&store),
            }),
            Box::new(CaffeinateResolver),
            Box::new(ClaudePaneResolver),
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

    #[test]
    fn test_match_process() {
        let store = Arc::new(SessionBindingStore::new());
        let p = ClaudeProvider::new(store);
        assert!(p.match_process("claude"));
        assert!(p.match_process("claude-code"));
        assert!(!p.match_process("aider"));
        assert!(!p.match_process("node"));
    }

    #[test]
    fn test_encode_cwd() {
        assert_eq!(
            encode_cwd("/home/user/Code/project"),
            "-home-user-Code-project"
        );
        assert_eq!(encode_cwd("/tmp"), "-tmp");
        assert_eq!(
            encode_cwd("/Users/didi/.dotfiles"),
            "-Users-didi--dotfiles"
        );
    }

    #[test]
    fn test_parse_status_user() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        assert_eq!(parse_status(line), Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_parse_status_assistant_end_turn() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn"}}"#;
        assert_eq!(parse_status(line), Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_parse_status_assistant_tool_use() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"Read","id":"123"}]}}"#;
        assert_eq!(parse_status(line), Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_parse_status_ask_user_question() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"AskUserQuestion","id":"456","input":{}}]}}"#;
        assert_eq!(parse_status(line), Some(AgentStatus::NeedsInput));
    }

    #[test]
    fn test_parse_status_tool_use_no_content() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use"}}"#;
        assert_eq!(parse_status(line), Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_parse_status_progress() {
        let line = r#"{"type":"progress","data":{}}"#;
        assert_eq!(parse_status(line), Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_parse_status_unknown_type() {
        let line = r#"{"type":"system","data":{}}"#;
        assert_eq!(parse_status(line), None);
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

        let status = parse_status_with_history(&jsonl_path);
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

        let status = parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::Waiting));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_parse_status_with_history_skips_local_commands() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_local_cmd_skip.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"end_turn"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","message":{{"content":"<local-command-caveat>ls</local-command-caveat>"}}}}"#
        )
        .unwrap();

        let status = parse_status_with_history(&jsonl_path);
        assert_eq!(status, Some(AgentStatus::Waiting));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_binding_resolver_returns_none_without_key() {
        let store = Arc::new(SessionBindingStore::new());
        let resolver = BindingResolver {
            store: Arc::clone(&store),
        };
        // No matched_pid or process_start_time → None
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, None, vec![]);
        assert_eq!(resolver.resolve(&ctx), None);
    }

    #[test]
    fn test_binding_resolver_cache_hit() {
        use std::io::Write;
        let store = Arc::new(SessionBindingStore::new());
        let temp_dir = std::env::temp_dir();
        let jsonl_path = temp_dir.join("test_binding_hit.jsonl");
        let mut file = std::fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"end_turn"}}}}"#
        )
        .unwrap();

        let key = SessionKey {
            pane_id: "%0".into(),
            matched_pid: 42,
            process_start_time: 1000,
        };
        store.bind(key, jsonl_path.clone());

        let resolver = BindingResolver {
            store: Arc::clone(&store),
        };
        let ctx = ResolveContext::new(1, "/tmp".into(), "%0".into(), Some(1000), Some(42), vec![]);
        let status = resolver.resolve(&ctx);
        assert_eq!(status, Some(AgentStatus::Waiting));

        std::fs::remove_file(jsonl_path).ok();
    }

    #[test]
    fn test_binding_resolver_unbinds_missing_file() {
        let store = Arc::new(SessionBindingStore::new());
        let key = SessionKey {
            pane_id: "%0".into(),
            matched_pid: 42,
            process_start_time: 1000,
        };
        store.bind(key.clone(), PathBuf::from("/nonexistent/file.jsonl"));

        let resolver = BindingResolver {
            store: Arc::clone(&store),
        };
        let ctx = ResolveContext::new(1, "/tmp".into(), "%0".into(), Some(1000), Some(42), vec![]);
        assert_eq!(resolver.resolve(&ctx), None);
        assert!(store.get(&key).is_none());
    }

    #[test]
    fn test_caffeinate_resolver_thinking() {
        let r = CaffeinateResolver;
        let ctx = ResolveContext::new(
            1,
            "/tmp".into(),
            "%0".into(),
            Some(1000),
            Some(42),
            vec![("99".into(), "caffeinate".into())],
        );
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_caffeinate_resolver_waiting() {
        let r = CaffeinateResolver;
        let ctx = ResolveContext::new(
            1,
            "/tmp".into(),
            "%0".into(),
            Some(1000),
            Some(42),
            vec![("99".into(), "node".into())],
        );
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_caffeinate_resolver_no_matched_pid() {
        let r = CaffeinateResolver;
        let ctx = ResolveContext::new(1, "/tmp".into(), "%0".into(), None, None, vec![]);
        assert_eq!(r.resolve(&ctx), None);
    }

    #[test]
    fn test_jsonl_discovery_single_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();

        // Create a fake Claude project dir structure
        let jsonl = dir.path().join("session-abc.jsonl");
        let mut file = std::fs::File::create(&jsonl).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"stop_reason":"end_turn"}}}}"#
        )
        .unwrap();

        // Test collect_jsonl_files directly since we can't easily mock project_dir_for_cwd
        let files = collect_jsonl_files(dir.path());
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_collect_jsonl_files_empty() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_jsonl_files(dir.path());
        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_jsonl_files_multiple() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jsonl"), "{}").unwrap();
        std::fs::write(dir.path().join("b.jsonl"), "{}").unwrap();
        std::fs::write(dir.path().join("c.txt"), "{}").unwrap();

        let files = collect_jsonl_files(dir.path());
        assert_eq!(files.len(), 2);
    }

    fn mock_pane_ctx(output: &str) -> ResolveContext {
        let ctx = ResolveContext::new(1234, "/tmp".into(), "%0".into(), None, None, vec![]);
        let _ = ctx.pane_output.set(output.to_string());
        ctx
    }

    #[test]
    fn test_pane_resolver_waiting() {
        let r = ClaudePaneResolver;
        let ctx = mock_pane_ctx("✻ Worked for 2m\n\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions\n");
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::Waiting));
    }

    #[test]
    fn test_pane_resolver_thinking() {
        let r = ClaudePaneResolver;
        let ctx = mock_pane_ctx("✽ Gitifying… (1m 7s · ↑ 856 tokens · thinking)\n\n  ⏵⏵ bypass permissions · esc to interrupt\n");
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::Thinking));
    }

    #[test]
    fn test_pane_resolver_needs_input() {
        let r = ClaudePaneResolver;
        let ctx = mock_pane_ctx("Which option?\nEnter to select · Tab/Arrow keys to navigate\n");
        assert_eq!(r.resolve(&ctx), Some(AgentStatus::NeedsInput));
    }

    #[test]
    fn test_pane_resolver_no_match() {
        let r = ClaudePaneResolver;
        let ctx = mock_pane_ctx("random output");
        assert_eq!(r.resolve(&ctx), None);
    }
}
