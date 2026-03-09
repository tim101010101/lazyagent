use std::path::Path;
use std::process::Command;

use crate::protocol::{AgentSession, Provider};
use crate::tmux::TmuxController;

pub struct SessionManager {
    providers: Vec<Box<dyn Provider>>,
    #[cfg(test)]
    mock_sessions: Option<Vec<AgentSession>>,
}

impl SessionManager {
    pub fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        Self {
            providers,
            #[cfg(test)]
            mock_sessions: None,
        }
    }

    #[cfg(test)]
    pub fn with_sessions(providers: Vec<Box<dyn Provider>>, sessions: Vec<AgentSession>) -> Self {
        Self {
            providers,
            mock_sessions: Some(sessions),
        }
    }

    pub fn poll(&self) -> Vec<AgentSession> {
        #[cfg(test)]
        if let Some(ref sessions) = self.mock_sessions {
            return sessions.clone();
        }

        TmuxController::discover_sessions(&self.providers)
    }

    pub fn spawn(&self, provider_id: &str, cwd: &Path) -> anyhow::Result<String> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.manifest().id == provider_id)
            .ok_or_else(|| anyhow::anyhow!("provider not found: {}", provider_id))?;

        let plan = provider.exec_plan(cwd);
        let dir_name = cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "default".into());

        TmuxController::spawn_session(&plan, provider_id, &dir_name)
    }

    pub fn attach_command(&self, session: &AgentSession) -> Command {
        TmuxController::attach_command(&session.tmux_session)
    }

    pub fn kill(&self, session: &AgentSession) -> anyhow::Result<()> {
        TmuxController::kill_session(&session.tmux_session)
    }

    pub fn providers(&self) -> &[Box<dyn Provider>] {
        &self.providers
    }
}
