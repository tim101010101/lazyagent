use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

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
            _ => return Vec::new(),
        };

        // Build process tree once for all panes
        let proc_tree = build_process_tree();

        let mut sessions = Vec::new();

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
                if provider.match_process(pane_cmd) {
                    matched_provider = Some(provider.manifest().id);
                    break;
                }
            }

            // If pane command doesn't match, check descendant processes via in-memory tree
            if matched_provider.is_none() {
                let descendants = find_descendant_commands(pane_pid, &proc_tree);
                'outer: for child_cmd in &descendants {
                    for provider in providers {
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

            // Detect status via capture-pane
            let status = capture_pane_status(pane_id, providers, &provider_id);

            let started_at = session_created
                .parse::<u64>()
                .ok()
                .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));

            sessions.push(AgentSession {
                kind,
                tmux_session: session_name.to_string(),
                tmux_pane: pane_id.to_string(),
                provider: provider_id,
                cwd: PathBuf::from(pane_path),
                status,
                started_at,
                source: SessionSource::Local,
            });
        }

        sessions
    }

    pub fn spawn_session(plan: &ExecPlan, provider_id: &str, dir_name: &str) -> anyhow::Result<String> {
        let session_name = format!("{}{}/{}", SESSION_PREFIX, provider_id, dir_name);
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

    pub fn kill_session(session_name: &str) -> anyhow::Result<()> {
        let output = Command::new("tmux")
            .args(["kill-session", "-t", session_name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
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

fn capture_pane_status(
    pane_id: &str,
    providers: &[Box<dyn Provider>],
    provider_id: &str,
) -> AgentStatus {
    let output = match Command::new("tmux")
        .args(["capture-pane", "-p", "-t", pane_id, "-S", "-5"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return AgentStatus::Unknown,
    };

    for provider in providers {
        if provider.manifest().id == provider_id {
            return provider.detect_status(&output);
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
}
