use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::protocol::ExecPlan;

const ENV_NO_TMUX: &str = "LAZYAGENT_NO_TMUX";

pub struct TmuxController {
    #[allow(dead_code)]
    self_pane_id: String,
    agent_pane_id: Option<String>,
}

impl TmuxController {
    /// Returns Some if running inside tmux, None otherwise.
    pub fn detect() -> Option<Self> {
        if std::env::var(ENV_NO_TMUX).is_ok() {
            return None;
        }
        if std::env::var("TMUX").is_err() {
            return None;
        }
        let output = Command::new("tmux")
            .args(["display-message", "-p", "#{pane_id}"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if pane_id.is_empty() {
            return None;
        }
        Some(Self {
            self_pane_id: pane_id,
            agent_pane_id: None,
        })
    }

    /// Returns true if tmux binary is available on PATH.
    pub fn tmux_available() -> bool {
        Command::new("tmux")
            .arg("-V")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Exec into a new tmux session running lazyagent. Never returns on success.
    #[cfg(unix)]
    pub fn auto_start() -> ! {
        let exe = std::env::current_exe().unwrap_or_else(|_| "lazyagent".into());
        let exe_str = exe.to_str().unwrap_or("lazyagent");
        let err = Command::new("tmux")
            .args(["new-session", "-s", "lazyagent", "--", exe_str])
            .exec();
        eprintln!("failed to exec tmux: {err}");
        std::process::exit(1);
    }

    /// Launch agent command in a split pane to the right (~70% width).
    pub fn launch_agent(&mut self, plan: &ExecPlan) -> anyhow::Result<()> {
        // Kill existing agent pane if any
        self.kill_agent();

        let mut cmd_parts = vec![plan.program.clone()];
        cmd_parts.extend(plan.args.clone());
        let shell_cmd = cmd_parts
            .iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ");

        let cwd = plan
            .cwd
            .as_deref()
            .unwrap_or(".");

        let mut tmux_cmd = Command::new("tmux");
        tmux_cmd.args([
            "split-window",
            "-h",
            "-l", "70%",
            "-d",
            "-P",
            "-F", "#{pane_id}",
            "-c", cwd,
            "--",
        ]);

        // Build env-setting wrapper: env -u CLAUDECODE <extra_env> <shell_cmd>
        let mut env_parts = vec!["env".to_string(), "-u".to_string(), "CLAUDECODE".to_string()];
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

        tmux_cmd.args(["sh", "-c", &full_cmd]);

        let output = tmux_cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux split-window failed: {stderr}");
        }

        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        self.agent_pane_id = Some(pane_id);
        Ok(())
    }

    /// Kill the agent pane if it exists.
    pub fn kill_agent(&mut self) {
        if let Some(ref pane_id) = self.agent_pane_id.take() {
            let _ = Command::new("tmux")
                .args(["kill-pane", "-t", pane_id])
                .output();
        }
    }

    /// Check if the agent pane is still alive.
    pub fn is_agent_alive(&self) -> bool {
        let pane_id = match &self.agent_pane_id {
            Some(id) => id,
            None => return false,
        };
        let output = Command::new("tmux")
            .args(["list-panes", "-F", "#{pane_id}"])
            .output();
        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.lines().any(|line| line.trim() == pane_id)
            }
            Err(_) => false,
        }
    }

    /// Focus the agent pane.
    pub fn focus_agent(&self) -> anyhow::Result<()> {
        if let Some(ref pane_id) = self.agent_pane_id {
            let output = Command::new("tmux")
                .args(["select-pane", "-t", pane_id])
                .output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("tmux select-pane failed: {stderr}");
            }
        }
        Ok(())
    }

    /// Focus back to self pane.
    #[allow(dead_code)]
    pub fn focus_self(&self) -> anyhow::Result<()> {
        let output = Command::new("tmux")
            .args(["select-pane", "-t", &self.self_pane_id])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tmux select-pane failed: {stderr}");
        }
        Ok(())
    }
}

/// Simple shell escaping: wrap in single quotes, escape existing single quotes.
fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/') {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}
