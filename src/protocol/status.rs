use std::cell::OnceCell;

use crate::protocol::AgentStatus;

/// Context for status resolution, passed to each resolver in the chain.
pub struct ResolveContext {
    pub pane_pid: u32,
    pub pane_cwd: String,
    pub pane_id: String,
    pane_output: OnceCell<String>,
    pub process_start_time: Option<u64>,
}

impl ResolveContext {
    pub fn new(
        pane_pid: u32,
        pane_cwd: String,
        pane_id: String,
        process_start_time: Option<u64>,
    ) -> Self {
        Self {
            pane_pid,
            pane_cwd,
            pane_id,
            pane_output: OnceCell::new(),
            process_start_time,
        }
    }

    /// Lazy fetch pane output via tmux capture-pane. Only called if needed.
    pub fn pane_output(&self) -> &str {
        self.pane_output.get_or_init(|| {
            use std::process::Command;
            Command::new("tmux")
                .args(["capture-pane", "-p", "-t", &self.pane_id])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default()
        })
    }
}

/// Trait for status resolution strategies. Each provider returns an ordered list.
/// First resolver to return Some(status) wins.
pub trait StatusResolver: Send + Sync {
    /// Try to resolve status. None = can't determine, try next resolver.
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus>;
}

/// Text-based status resolver using declarative rules (shared fallback).
pub struct TextMatchResolver {
    rules: &'static [StatusRule],
    scan_lines: usize,
    fallback: AgentStatus,
}

impl TextMatchResolver {
    pub const fn new(
        rules: &'static [StatusRule],
        scan_lines: usize,
        fallback: AgentStatus,
    ) -> Self {
        Self {
            rules,
            scan_lines,
            fallback,
        }
    }
}

impl StatusResolver for TextMatchResolver {
    fn resolve(&self, ctx: &ResolveContext) -> Option<AgentStatus> {
        let output = ctx.pane_output();
        let lines: Vec<&str> = output
            .lines()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .skip_while(|l| l.trim().is_empty())
            .take(self.scan_lines)
            .collect();

        for rule in self.rules {
            for line in &lines {
                if rule.matcher.matches(line) {
                    return Some(rule.status.clone());
                }
            }
        }

        Some(self.fallback.clone())
    }
}

pub struct StatusRule {
    pub status: AgentStatus,
    pub matcher: LineMatcher,
}

pub enum LineMatcher {
    Contains(&'static str),
    StartsWith(&'static str),
    EndsWith(&'static str),
}

impl LineMatcher {
    fn matches(&self, line: &str) -> bool {
        let trimmed = line.trim();
        match self {
            LineMatcher::Contains(s) => trimmed.contains(s),
            LineMatcher::StartsWith(s) => trimmed.starts_with(s),
            LineMatcher::EndsWith(s) => trimmed.ends_with(s),
        }
    }
}
