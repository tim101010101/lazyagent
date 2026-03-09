use std::collections::BTreeMap;
use std::path::Path;

use tracing::trace;

use crate::protocol::{
    AgentStatus, ExecPlan, LineMatcher, Provider, ProviderManifest, StatusResolver, StatusRule,
    TextMatchResolver,
};

static CLAUDE_TEXT_RULES: &[StatusRule] = &[
    StatusRule {
        status: AgentStatus::Thinking,
        matcher: LineMatcher::EndsWith("thinking)"),
    },
    StatusRule {
        status: AgentStatus::Thinking,
        matcher: LineMatcher::Contains("esc to interrupt"),
    },
    StatusRule {
        status: AgentStatus::Waiting,
        matcher: LineMatcher::StartsWith("❯"),
    },
];

pub struct ClaudeProvider {
    resolvers: Vec<Box<dyn StatusResolver>>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let resolvers: Vec<Box<dyn StatusResolver>> = vec![Box::new(TextMatchResolver::new(
            CLAUDE_TEXT_RULES,
            20,
            AgentStatus::Unknown,
        ))];

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
    fn test_detect_status_waiting_after_clear() {
        let p = ClaudeProvider::new();
        // After /clear: prompt near top, many trailing blank lines
        let mut output = String::from(
            "Claude Code v2.1.70\n\n❯ /clear\n  (no content)\n\n❯\u{a0}\n────────── ▪▪▪ ─\n  ⏵⏵ bypass permissions on\n",
        );
        // Simulate many trailing blank lines from capture-pane
        for _ in 0..40 {
            output.push('\n');
        }
        assert_eq!(p.detect_status(&output), AgentStatus::Waiting);
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
