use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::protocol::{
    ExecPlan, ListQuery, Provider, SessionDetail, SessionSummary,
};
use crate::tmux::TmuxController;

#[derive(Debug, Clone)]
pub enum SidebarItem {
    ProjectHeader(String),
    Session(SessionSummary),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Sidebar,
}

pub struct App {
    pub running: bool,
    pub show_detail: bool,
    pub search_mode: bool,
    pub search_query: String,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_index: usize,
    pub current_detail: Option<SessionDetail>,
    pub panel: Panel,
    pub error_message: Option<String>,
    pub health_message: Option<String>,
    pub tmux: Option<TmuxController>,
    pub agent_running: bool,
    providers: Vec<Box<dyn Provider>>,
}

impl App {
    pub fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        let tmux = TmuxController::detect();
        let mut app = App {
            running: true,
            show_detail: true,
            search_mode: false,
            search_query: String::new(),
            sidebar_items: Vec::new(),
            selected_index: 0,
            current_detail: None,
            panel: Panel::Sidebar,
            error_message: None,
            health_message: None,
            tmux,
            agent_running: false,
            providers,
        };
        app.check_health();
        app.refresh_sessions();
        app
    }

    fn check_health(&mut self) {
        for provider in &self.providers {
            let health = provider.health();
            if !health.available {
                self.health_message = health.message;
                return;
            }
            if let Some(msg) = health.message {
                self.health_message = Some(msg);
            }
        }
    }

    pub fn refresh_sessions(&mut self) {
        let query = ListQuery {
            search: if self.search_query.is_empty() {
                None
            } else {
                Some(self.search_query.clone())
            },
            ..Default::default()
        };

        let mut all_sessions: Vec<SessionSummary> = Vec::new();

        for provider in &self.providers {
            match provider.list_sessions(&query) {
                Ok(response) => {
                    all_sessions.extend(response.items);
                }
                Err(e) => {
                    self.error_message = Some(e.message.clone());
                }
            }
        }

        // Sort by updated_at descending
        all_sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        // Group by project
        let mut groups: BTreeMap<String, Vec<SessionSummary>> = BTreeMap::new();
        for session in all_sessions {
            let project = session
                .project_path
                .clone()
                .unwrap_or_else(|| "Unknown".into());
            groups.entry(project).or_default().push(session);
        }

        // Build sidebar items
        self.sidebar_items.clear();
        for (project, sessions) in &groups {
            self.sidebar_items
                .push(SidebarItem::ProjectHeader(project.clone()));
            for session in sessions {
                self.sidebar_items
                    .push(SidebarItem::Session(session.clone()));
            }
        }

        // Adjust selection
        if self.selected_index >= self.sidebar_items.len() {
            self.selected_index = self.sidebar_items.len().saturating_sub(1);
        }

        // Ensure we're on a session, not a header
        self.skip_to_nearest_session();

        // Load detail for current selection
        self.load_current_detail();
    }

    fn skip_to_nearest_session(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        // If on a header, move down to next session
        if matches!(self.sidebar_items.get(self.selected_index), Some(SidebarItem::ProjectHeader(_))) {
            if self.selected_index + 1 < self.sidebar_items.len() {
                self.selected_index += 1;
            }
        }
    }

    fn load_current_detail(&mut self) {
        if let Some(SidebarItem::Session(summary)) = self.sidebar_items.get(self.selected_index) {
            let native_id = summary.native_id.clone();
            let provider_id = summary.provider_id.clone();

            for provider in &self.providers {
                if provider.manifest().id == provider_id {
                    match provider.session_detail(&native_id) {
                        Ok(detail) => {
                            self.current_detail = Some(detail);
                        }
                        Err(_) => {
                            self.current_detail = None;
                        }
                    }
                    break;
                }
            }
        } else {
            self.current_detail = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
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
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
            }
            KeyCode::Enter => { /* handled in main.rs for suspend/exec */ }
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

        // Skip project headers
        if matches!(self.sidebar_items.get(new_idx), Some(SidebarItem::ProjectHeader(_))) {
            let next = (new_idx as i32 + delta).clamp(0, len - 1) as usize;
            if matches!(self.sidebar_items.get(next), Some(SidebarItem::Session(_))) {
                new_idx = next;
            } else {
                // Can't skip past header — stay where we are
                return;
            }
        }

        if new_idx != self.selected_index {
            self.selected_index = new_idx;
            self.load_current_detail();
        }
    }

    fn move_to_top(&mut self) {
        self.selected_index = 0;
        self.skip_to_nearest_session();
        self.load_current_detail();
    }

    fn move_to_bottom(&mut self) {
        if !self.sidebar_items.is_empty() {
            self.selected_index = self.sidebar_items.len() - 1;
            self.load_current_detail();
        }
    }

    pub fn resume_selected(&mut self) -> Option<ResumeAction> {
        self.exec_plan_for_selected().map(|plan| ResumeAction {
            program: plan.program,
            args: plan.args,
            cwd: plan.cwd,
        })
    }

    pub fn exec_plan_for_selected(&mut self) -> Option<ExecPlan> {
        let (native_id, provider_id) = match self.sidebar_items.get(self.selected_index) {
            Some(SidebarItem::Session(s)) => (s.native_id.clone(), s.provider_id.clone()),
            _ => return None,
        };

        for provider in &self.providers {
            if provider.manifest().id == provider_id {
                match provider.resume_command(&native_id) {
                    Ok(plan) => return Some(plan),
                    Err(e) => {
                        self.error_message = Some(e.message);
                    }
                }
                break;
            }
        }
        None
    }

    pub fn tmux_mode(&self) -> bool {
        self.tmux.is_some()
    }
}

pub struct ResumeAction {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}
