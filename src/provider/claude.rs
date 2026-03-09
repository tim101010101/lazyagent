use std::collections::BTreeMap;
use std::path::Path;

use crate::protocol::{AgentStatus, ExecPlan, Provider, ProviderManifest};

pub struct ClaudeProvider;

impl ClaudeProvider {
    pub fn new() -> Self {
        Self
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
        process_name == "claude" || process_name == "claude-code"
    }

    fn detect_status(&self, pane_output: &str) -> AgentStatus {
        let lines: Vec<&str> = pane_output.lines().collect();
        let tail: Vec<&str> = lines.iter().rev().take(20).copied().collect();

        // Scan bottom of pane for Claude Code UI patterns
        let mut has_prompt = false;
        let mut has_thinking = false;

        for line in &tail {
            let trimmed = line.trim();

            // Claude Code prompt: line starting with ❯ (U+276F)
            if trimmed.starts_with('❯') {
                has_prompt = true;
            }

            // Active thinking: status line with "thinking)" at end
            // e.g. "✽ Gitifying… (1m 7s · ↑ 856 tokens · thinking)"
            if trimmed.ends_with("thinking)") {
                has_thinking = true;
            }

            // Active tool execution: "esc to interrupt" in status bar
            if trimmed.contains("esc to interrupt") {
                has_thinking = true;
            }
        }

        if has_thinking {
            return AgentStatus::Thinking;
        }

        if has_prompt {
            return AgentStatus::Waiting;
        }

        AgentStatus::Unknown
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
    fn test_detect_status_waiting() {
        let p = ClaudeProvider::new();
        // Claude Code TUI: prompt line with ❯, status bar below
        let output = "✻ Worked for 2m 10s\n\n────────── ▪▪▪ ─\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions\n";
        assert_eq!(p.detect_status(output), AgentStatus::Waiting);
    }

    #[test]
    fn test_detect_status_thinking() {
        let p = ClaudeProvider::new();
        let output = "✽ Gitifying… (1m 7s · ↑ 856 tokens · thinking)\n\n────────── ▪▪▪ ─\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions · esc to interrupt\n";
        assert_eq!(p.detect_status(output), AgentStatus::Thinking);
    }

    #[test]
    fn test_detect_status_error() {
        let p = ClaudeProvider::new();
        // Tool output containing "error:" should NOT trigger Error status
        let output = "  error: nix eval failed\n\n✻ Worked for 1m\n\n────────── ▪▪▪ ─\n❯\u{a0}\n";
        assert_eq!(p.detect_status(output), AgentStatus::Waiting);
    }

    #[test]
    fn test_detect_status_unknown() {
        let p = ClaudeProvider::new();
        assert_eq!(p.detect_status("random output"), AgentStatus::Unknown);
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
}
