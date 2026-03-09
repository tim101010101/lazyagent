use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime};

use rayon::prelude::*;
use tracing::{debug, trace, warn};

use crate::protocol::{
    AgentSession, AgentStatus, ExecPlan, Provider, SessionKind, SessionSource,
};

const SESSION_PREFIX: &str = "la/";

pub struct TmuxController;

impl TmuxController {
    pub fn detect() -> bool {
        std::env::var("TMUX").is_ok()
    }

    pub fn tmux_available() -> bool {
        Command::new("tmux")
            .arg("-V")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn discover_sessions(providers: &[Box<dyn Provider>]) -> Vec<AgentSession> {
        let start = Instant::now();
        let output = match Command::new("tmux")
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name} #{pane_id} #{pane_pid} #{pane_current_command} #{pane_current_path} #{session_created}",
            ])
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                warn!(stderr = %stderr, "tmux list-panes failed");
                return Vec::new();
            }
            Err(e) => {
                warn!(err = %e, "tmux list-panes failed");
                return Vec::new();
            }
        };

        // Build process tree once for all panes
        let proc_tree = build_process_tree();

        // Phase 1: collect matched panes (fast, in-memory only)
        struct MatchedPane {
            session_name: String,
            pane_id: String,
            pane_path: String,
            session_created: String,
            provider_id: String,
            kind: SessionKind,
        }

        let mut matched: Vec<MatchedPane> = Vec::new();

        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(6, ' ').collect();
            if parts.len() < 5 {
                continue;
            }

            let session_name = parts[0];
            let pane_id = parts[1];
            let pane_pid = parts[2];
            let pane_cmd = parts[3];
            let pane_path = parts[4];
            let session_created = parts.get(5).unwrap_or(&"");

            // Check if pane command directly matches a provider
            let mut matched_provider: Option<String> = None;
            for provider in providers {
                trace!(pane_cmd, provider_id = %provider.manifest().id, "checking direct match");
                if provider.match_process(pane_cmd) {
                    matched_provider = Some(provider.manifest().id);
                    break;
                }
            }

            // If pane command doesn't match, check descendant processes via in-memory tree
            if matched_provider.is_none() {
                let descendants = find_descendant_commands(pane_pid, &proc_tree);
                trace!(pane_pid, descendant_count = descendants.len(), "checking descendants");
                'outer: for child_cmd in &descendants {
                    for provider in providers {
                        trace!(child_cmd, provider_id = %provider.manifest().id, "checking descendant");
                        if provider.match_process(child_cmd) {
                            matched_provider = Some(provider.manifest().id);
                            break 'outer;
                        }
                    }
                }
            }

            let provider_id = match matched_provider {
                Some(id) => id,
                None => continue,
            };

            let kind = if session_name.starts_with(SESSION_PREFIX) {
                SessionKind::Managed
            } else {
                SessionKind::Discovered
            };

            matched.push(MatchedPane {
                session_name: session_name.to_string(),
                pane_id: pane_id.to_string(),
                pane_path: pane_path.to_string(),
                session_created: session_created.to_string(),
                provider_id,
                kind,
            });
        }

        debug!(count = matched.len(), "matched panes for status detection");

        // Phase 2: parallel capture-pane + git root resolution
        let sessions: Vec<AgentSession> = matched
            .par_iter()
            .map(|m| {
                let status = capture_pane_status(&m.pane_id, providers, &m.provider_id);
                let git_root = resolve_git_root(Path::new(&m.pane_path));
                trace!(pane_id = %m.pane_id, git_root = ?git_root, "resolved git root");

                let started_at = m
                    .session_created
                    .parse::<u64>()
                    .ok()
                    .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));

                AgentSession {
                    kind: m.kind.clone(),
                    tmux_session: m.session_name.clone(),
                    tmux_pane: m.pane_id.clone(),
                    provider: m.provider_id.clone(),
                    cwd: PathBuf::from(&m.pane_path),
                    status,
                    started_at,
                    source: SessionSource::Local,
                    git_root,
                }
            })
            .collect();

        let elapsed_ms = start.elapsed().as_millis();
        debug!(elapsed_ms, total = sessions.len(), "discovery complete");
        sessions
    }

    pub fn spawn_session(plan: &ExecPlan, provider_id: &str, dir_name: &str) -> anyhow::Result<String> {
        let session_name = format!("{}{}/{}", SESSION_PREFIX, provider_id, dir_name);
        debug!(session = %session_name, provider_id, dir_name, "spawning tmux session");
        let cwd = plan.cwd.as_deref().unwrap_or(".");

        let mut cmd_parts = vec![plan.program.clone()];
        cmd_parts.extend(plan.args.clone());

        let shell_cmd = cmd_parts
            .iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ");

        let mut tmux_args = vec![
            "new-session".to_string(),
            "-d".to_string(),
            "-s".to_string(),
            session_name.clone(),
            "-c".to_string(),
            cwd.to_string(),
            "--".to_string(),
        ];

        // Build env wrapper if needed
        if plan.env.is_empty() {
            tmux_args.push("sh".to_string());
            tmux_args.push("-c".to_string());
            tmux_args.push(shell_cmd);
        } else {
            let mut env_parts = vec!["env".to_string()];
            for (k, v) in &plan.env {
                env_parts.push(format!("{k}={v}"));
            }
            env_parts.push("sh".to_string());
            env_parts.push("-c".to_string());
            env_parts.push(shell_cmd);

            let full_cmd = env_parts
                .iter()
                .map(|s| shell_escape(s))
                .collect::<Vec<_>>()
                .join(" ");

            tmux_args.push("sh".to_string());
            tmux_args.push("-c".to_string());
            tmux_args.push(full_cmd);
        }

        let output = Command::new("tmux")
            .args(&tmux_args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux new-session failed: {stderr}");
        }

        debug!(session = %session_name, "tmux session spawned");
        Ok(session_name)
    }

    pub fn attach_command(session_name: &str) -> Command {
        let mut cmd = Command::new("tmux");
        cmd.args(["attach-session", "-t", session_name]);
        cmd
    }

    /// Capture the full visible content of a tmux pane with ANSI colors.
    pub fn capture_pane(pane_id: &str) -> Option<String> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-p", "-e", "-t", pane_id])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Send named keys to a tmux pane (e.g. "Enter", "BSpace", "C-c").
    pub fn send_keys(pane_id: &str, keys: &[&str]) -> anyhow::Result<()> {
        let mut args = vec!["send-keys", "-t", pane_id];
        args.extend(keys);

        let output = Command::new("tmux").args(&args).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux send-keys failed: {stderr}");
        }
        Ok(())
    }

    /// Send literal text to a tmux pane (characters sent as-is).
    pub fn send_text(pane_id: &str, text: &str) -> anyhow::Result<()> {
        let output = Command::new("tmux")
            .args(["send-keys", "-t", pane_id, "-l", text])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux send-keys -l failed: {stderr}");
        }
        Ok(())
    }

    pub fn kill_session(session_name: &str) -> anyhow::Result<()> {
        debug!(session = %session_name, "killing tmux session");
        let output = Command::new("tmux")
            .args(["kill-session", "-t", session_name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(session = %session_name, stderr = %stderr, "tmux kill-session failed");
            anyhow::bail!("tmux kill-session failed: {stderr}");
        }

        Ok(())
    }
}

/// Build a process tree from a single `ps -eo` call.
/// Returns ppid → [(pid, comm)].
fn build_process_tree() -> HashMap<String, Vec<(String, String)>> {
    let output = match Command::new("ps")
        .args(["-eo", "pid=,ppid=,comm="])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return HashMap::new(),
    };

    let mut tree: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let pid = parts[0].to_string();
            let ppid = parts[1].to_string();
            let comm = parts[2..].join(" ");
            tree.entry(ppid).or_default().push((pid, comm));
        }
    }
    debug!(proc_tree_size = tree.len(), "process tree built");
    tree
}

/// Walk the process tree in-memory (BFS) to find descendant command names.
fn find_descendant_commands(pid: &str, tree: &HashMap<String, Vec<(String, String)>>) -> Vec<String> {
    let mut result = Vec::new();
    let mut queue = vec![pid.to_string()];

    while let Some(current) = queue.pop() {
        if let Some(children) = tree.get(&current) {
            for (child_pid, comm) in children {
                result.push(comm.clone());
                queue.push(child_pid.clone());
            }
        }
    }

    result
}

/// Resolve git root directory name for a path. Returns None if not a git repo.
fn resolve_git_root(cwd: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let full = String::from_utf8_lossy(&o.stdout).trim().to_string();
            Path::new(&full)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or(full)
        })
}

fn capture_pane_status(
    pane_id: &str,
    providers: &[Box<dyn Provider>],
    provider_id: &str,
) -> AgentStatus {
    let output = match Command::new("tmux")
        .args(["capture-pane", "-p", "-t", pane_id])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            debug!(pane_id, "capture-pane failed");
            return AgentStatus::Unknown;
        }
    };

    for provider in providers {
        if provider.manifest().id == provider_id {
            let status = provider.detect_status(&output);
            debug!(pane_id, provider_id, ?status, "detected status");
            return status;
        }
    }

    AgentStatus::Unknown
}

fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_simple() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("/usr/bin/claude"), "/usr/bin/claude");
    }

    #[test]
    fn test_shell_escape_special_chars() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_resolve_git_root_returns_dir_name() {
        // This test runs against the actual repo we're in
        let cwd = std::env::current_dir().expect("cwd");
        let result = resolve_git_root(&cwd);
        assert!(result.is_some(), "should detect git root in project dir");
        // Should be just the dir name, not the full path
        let name = result.unwrap();
        assert!(!name.contains('/'), "should be dir name only, got: {name}");
    }

    #[test]
    fn test_resolve_git_root_non_git_dir() {
        let result = resolve_git_root(Path::new("/tmp"));
        assert!(result.is_none(), "non-git dir should return None");
    }
}
