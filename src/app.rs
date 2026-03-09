use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::bg::BgUpdate;
use crate::protocol::{AgentSession, SessionSource};
use crate::session::SessionManager;

#[derive(Debug, Clone)]
pub enum SidebarItem {
    SourceHeader(String),
    ProjectHeader(String),
    Session(usize), // index into sessions vec
}

pub struct App {
    pub running: bool,
    pub show_detail: bool,
    pub search_mode: bool,
    pub search_query: String,
    pub sessions: Vec<AgentSession>,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_index: usize,
    pub error_message: Option<String>,
    pub confirm_kill: Option<usize>,
    pub pane_preview: Option<String>,
    pub preview_cache: HashMap<String, String>,
    pub pending_preview: Option<String>,
    session_manager: SessionManager,
}

impl App {
    pub fn new(session_manager: SessionManager) -> Self {
        App {
            running: true,
            show_detail: true,
            search_mode: false,
            search_query: String::new(),
            sessions: Vec::new(),
            sidebar_items: Vec::new(),
            selected_index: 0,
            error_message: None,
            confirm_kill: None,
            pane_preview: None,
            preview_cache: HashMap::new(),
            pending_preview: None,
            session_manager,
        }
    }

    /// Update sessions list from externally-provided data. No subprocess calls.
    pub fn update_sessions(&mut self, mut sessions: Vec<AgentSession>) {
        // Filter by search query
        if !self.search_query.is_empty() {
            let q = self.search_query.to_lowercase();
            sessions.retain(|s| {
                s.provider.to_lowercase().contains(&q)
                    || s.cwd.to_string_lossy().to_lowercase().contains(&q)
            });
        }

        // Sort by started_at descending (most recent first)
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        // Group by source → cwd parent
        self.sidebar_items.clear();

        // Collect sources
        let mut local_sessions: Vec<(usize, &AgentSession)> = Vec::new();
        let mut remote_groups: std::collections::BTreeMap<String, Vec<(usize, &AgentSession)>> =
            std::collections::BTreeMap::new();

        for (i, session) in sessions.iter().enumerate() {
            match &session.source {
                SessionSource::Local => local_sessions.push((i, session)),
                SessionSource::Remote { host } => {
                    remote_groups
                        .entry(host.clone())
                        .or_default()
                        .push((i, session));
                }
            }
        }

        // Build local section
        if !local_sessions.is_empty() {
            self.sidebar_items
                .push(SidebarItem::SourceHeader("local".into()));
            self.build_project_groups(&local_sessions);
        }

        // Build remote sections
        for (host, group) in &remote_groups {
            self.sidebar_items
                .push(SidebarItem::SourceHeader(host.clone()));
            self.build_project_groups(group);
        }

        self.sessions = sessions;

        // Adjust selection
        if self.selected_index >= self.sidebar_items.len() {
            self.selected_index = self.sidebar_items.len().saturating_sub(1);
        }
        self.skip_to_nearest_session();
        self.update_preview_from_cache();
    }

    /// Apply a background update (sessions or preview).
    pub fn apply_bg_update(&mut self, update: BgUpdate) {
        match update {
            BgUpdate::Sessions(sessions) => self.update_sessions(sessions),
            BgUpdate::Preview { pane_id, content } => {
                self.preview_cache.insert(pane_id, content);
                self.update_preview_from_cache();
            }
        }
    }

    /// Set pane_preview from cache for current selection. Request bg capture if missing.
    fn update_preview_from_cache(&mut self) {
        if let Some(session) = self.selected_session() {
            let pane_id = session.tmux_pane.clone();
            if let Some(cached) = self.preview_cache.get(&pane_id) {
                self.pane_preview = Some(cached.clone());
                self.pending_preview = None;
            } else {
                // Show stale preview while waiting
                self.pending_preview = Some(pane_id);
            }
        } else {
            self.pane_preview = None;
            self.pending_preview = None;
        }
    }

    /// Legacy: poll sessions via SessionManager (used in tests).
    pub fn refresh_sessions(&mut self) {
        let sessions = self.session_manager.poll();
        self.update_sessions(sessions);
    }

    pub fn refresh_preview(&mut self) {
        // Request preview for current selection via pending_preview
        if let Some(session) = self.selected_session() {
            self.pending_preview = Some(session.tmux_pane.clone());
        }
    }

    fn build_project_groups(&mut self, sessions: &[(usize, &AgentSession)]) {
        let mut by_project: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();

        for (idx, session) in sessions {
            let parent = session
                .cwd
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into());
            by_project.entry(parent).or_default().push(*idx);
        }

        for (project, indices) in &by_project {
            self.sidebar_items
                .push(SidebarItem::ProjectHeader(project.clone()));
            for &idx in indices {
                self.sidebar_items.push(SidebarItem::Session(idx));
            }
        }
    }

    fn skip_to_nearest_session(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        if !matches!(
            self.sidebar_items.get(self.selected_index),
            Some(SidebarItem::Session(_))
        ) {
            // Move forward to find a session
            for i in self.selected_index..self.sidebar_items.len() {
                if matches!(self.sidebar_items[i], SidebarItem::Session(_)) {
                    self.selected_index = i;
                    return;
                }
            }
            // Try backward
            for i in (0..self.selected_index).rev() {
                if matches!(self.sidebar_items[i], SidebarItem::Session(_)) {
                    self.selected_index = i;
                    return;
                }
            }
        }
    }

    pub fn selected_session(&self) -> Option<&AgentSession> {
        match self.sidebar_items.get(self.selected_index) {
            Some(SidebarItem::Session(idx)) => self.sessions.get(*idx),
            _ => None,
        }
    }

    pub fn selected_session_index(&self) -> Option<usize> {
        match self.sidebar_items.get(self.selected_index) {
            Some(SidebarItem::Session(idx)) => Some(*idx),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        // Kill confirmation mode
        if self.confirm_kill.is_some() {
            match key.code {
                KeyCode::Char('y') => {
                    if let Some(idx) = self.confirm_kill.take() {
                        if let Some(session) = self.sessions.get(idx) {
                            if let Err(e) = self.session_manager.kill(session) {
                                self.error_message = Some(format!("kill failed: {e}"));
                            }
                            self.refresh_sessions();
                        }
                    }
                }
                _ => {
                    self.confirm_kill = None;
                }
            }
            return;
        }

        if self.search_mode {
            self.handle_search_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('g') => self.move_to_top(),
            KeyCode::Char('G') => self.move_to_bottom(),
            KeyCode::Char('l') => self.show_detail = true,
            KeyCode::Char('h') => self.show_detail = false,
            KeyCode::Char('d') => {
                if let Some(idx) = self.selected_session_index() {
                    self.confirm_kill = Some(idx);
                }
            }
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
            }
            KeyCode::Char('r') => self.refresh_sessions(),
            // Enter and 'n' handled in main.rs
            KeyCode::Enter | KeyCode::Char('n') => {}
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.refresh_sessions();
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.refresh_sessions();
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.refresh_sessions();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.refresh_sessions();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.sidebar_items.is_empty() {
            return;
        }

        let len = self.sidebar_items.len() as i32;
        let mut new_idx = (self.selected_index as i32 + delta).clamp(0, len - 1) as usize;

        // Skip headers
        if !matches!(
            self.sidebar_items.get(new_idx),
            Some(SidebarItem::Session(_))
        ) {
            let next = (new_idx as i32 + delta).clamp(0, len - 1) as usize;
            if matches!(self.sidebar_items.get(next), Some(SidebarItem::Session(_))) {
                new_idx = next;
            } else {
                return;
            }
        }

        self.selected_index = new_idx;
        self.update_preview_from_cache();
    }

    fn move_to_top(&mut self) {
        self.selected_index = 0;
        self.skip_to_nearest_session();
        self.update_preview_from_cache();
    }

    fn move_to_bottom(&mut self) {
        if !self.sidebar_items.is_empty() {
            self.selected_index = self.sidebar_items.len() - 1;
            // If last item is a header, move up
            if !matches!(
                self.sidebar_items.get(self.selected_index),
                Some(SidebarItem::Session(_))
            ) {
                for i in (0..self.selected_index).rev() {
                    if matches!(self.sidebar_items[i], SidebarItem::Session(_)) {
                        self.selected_index = i;
                        break;
                    }
                }
            }
            self.update_preview_from_cache();
        }
    }

    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    pub fn default_provider_id(&self) -> Option<String> {
        self.session_manager
            .providers()
            .first()
            .map(|p| p.manifest().id)
    }
}
