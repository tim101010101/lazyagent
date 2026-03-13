use std::collections::HashMap;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tracing::{debug, info, trace, warn};

use crate::bg::BgUpdate;
use crate::config::{self, CustomGroup, KeyBindings, LayoutConfig, SidebarConfig, TimingConfig};
use crate::protocol::{AgentSession, SessionSource};
use crate::session::SessionManager;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum GroupingMode {
    Flat,
    GitRoot,
    Custom,
}

impl GroupingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupingMode::Flat => "flat",
            GroupingMode::GitRoot => "git",
            GroupingMode::Custom => "custom",
        }
    }

    #[cfg(test)]
    pub fn label(&self) -> &'static str {
        self.as_str()
    }

    /// Cycle to next mode. Skips Custom if no custom groups configured.
    pub fn cycle(&self, has_custom_groups: bool) -> GroupingMode {
        match self {
            GroupingMode::Flat => GroupingMode::GitRoot,
            GroupingMode::GitRoot => {
                if has_custom_groups {
                    GroupingMode::Custom
                } else {
                    GroupingMode::Flat
                }
            }
            GroupingMode::Custom => GroupingMode::Flat,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SidebarItem {
    SourceHeader(String),
    GroupHeader(String),
    Session(usize), // index into sessions vec
}

pub struct App {
    pub running: bool,
    pub show_detail: bool,
    pub show_help_overlay: bool,
    pub search_mode: bool,
    pub search_query: String,
    pub sessions: Vec<AgentSession>,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_index: usize,
    pub error_message: Option<String>,
    pub confirm_kill: Option<String>,
    pub pane_preview: Option<String>,
    pub preview_cache: HashMap<String, String>,
    pub pending_preview: Option<String>,
    pub grouping_mode: GroupingMode,
    pub custom_groups: Vec<CustomGroup>,
    pub passthrough_mode: bool,
    pub last_esc_time: Option<Instant>,
    pub tick: u64,
    pub theme: Theme,
    pub keys: KeyBindings,
    pub layout_config: LayoutConfig,
    pub sidebar_config: SidebarConfig,
    pub timing: TimingConfig,
    pub refresh_requested: bool,
    pub selected_pane: Option<String>,
    all_sessions: Vec<AgentSession>,
    session_manager: SessionManager,
}

impl App {
    pub fn new(session_manager: SessionManager) -> Self {
        App {
            running: true,
            show_detail: true,
            show_help_overlay: false,
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
            grouping_mode: GroupingMode::GitRoot,
            custom_groups: Vec::new(),
            passthrough_mode: false,
            last_esc_time: None,
            tick: 0,
            theme: Theme::default(),
            keys: KeyBindings::default(),
            layout_config: LayoutConfig::default(),
            sidebar_config: SidebarConfig::default(),
            timing: TimingConfig::default(),
            refresh_requested: false,
            selected_pane: None,
            all_sessions: Vec::new(),
            session_manager,
        }
    }

    /// Load all config sections.
    pub fn load_config(&mut self) {
        let cfg = config::load_config();
        self.grouping_mode = cfg.grouping_mode();
        self.custom_groups = cfg.group;
        self.theme = Theme::from_config(&cfg.theme);
        self.keys = KeyBindings::from_config(&cfg.keys);
        self.layout_config = cfg.layout;
        self.sidebar_config = cfg.sidebar;
        self.timing = cfg.timing;
        // If custom mode but no groups configured, fall back to git
        if self.grouping_mode == GroupingMode::Custom && self.custom_groups.is_empty() {
            self.grouping_mode = GroupingMode::GitRoot;
        }
    }

    /// Update sessions list from externally-provided data. No subprocess calls.
    pub fn update_sessions(&mut self, sessions: Vec<AgentSession>) {
        self.all_sessions = sessions;
        self.apply_filter();
    }

    /// Re-filter and sort from all_sessions, rebuild sidebar, preserve selection.
    pub fn apply_filter(&mut self) {
        let mut sessions = self.all_sessions.clone();

        // Filter by search query
        if !self.search_query.is_empty() {
            let q = self.search_query.to_lowercase();
            sessions.retain(|s| {
                s.provider.to_lowercase().contains(&q)
                    || s.cwd.to_string_lossy().to_lowercase().contains(&q)
            });
            debug!(query = %self.search_query, matches = sessions.len(), "search filtered");
        }

        // Sort by started_at descending (most recent first)
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        debug!(count = sessions.len(), "sessions updated");
        self.sessions = sessions;
        self.rebuild_sidebar();
        self.restore_selection();
    }

    /// Rebuild sidebar items from current sessions without re-polling.
    pub fn rebuild_sidebar(&mut self) {
        self.sidebar_items.clear();

        // Split by source
        let mut local_indices: Vec<usize> = Vec::new();
        let mut remote_groups: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();

        for (i, session) in self.sessions.iter().enumerate() {
            match &session.source {
                SessionSource::Local => local_indices.push(i),
                SessionSource::Remote { host } => {
                    remote_groups.entry(host.clone()).or_default().push(i);
                }
            }
        }

        // Build local section
        if !local_indices.is_empty() {
            self.sidebar_items
                .push(SidebarItem::SourceHeader("local".into()));
            self.build_group_items(&local_indices);
        }

        // Build remote sections
        for (host, indices) in &remote_groups {
            self.sidebar_items
                .push(SidebarItem::SourceHeader(host.clone()));
            self.build_group_items(indices);
        }

        // Adjust selection
        if self.selected_index >= self.sidebar_items.len() {
            self.selected_index = self.sidebar_items.len().saturating_sub(1);
        }
        self.skip_to_nearest_session();
        self.update_preview_from_cache();
    }

    /// Build group items for a set of session indices based on current grouping mode.
    fn build_group_items(&mut self, indices: &[usize]) {
        match self.grouping_mode {
            GroupingMode::Flat => self.build_flat(indices),
            GroupingMode::GitRoot => self.build_git_root_groups(indices),
            GroupingMode::Custom => self.build_custom_groups(indices),
        }
    }

    fn build_flat(&mut self, indices: &[usize]) {
        for &idx in indices {
            self.sidebar_items.push(SidebarItem::Session(idx));
        }
    }

    fn build_git_root_groups(&mut self, indices: &[usize]) {
        let mut by_root: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();

        for &idx in indices {
            let key = self.sessions[idx]
                .git_root
                .clone()
                .unwrap_or_else(|| "ungrouped".into());
            by_root.entry(key).or_default().push(idx);
        }

        for (root, group_indices) in &by_root {
            self.sidebar_items
                .push(SidebarItem::GroupHeader(root.clone()));
            for &idx in group_indices {
                self.sidebar_items.push(SidebarItem::Session(idx));
            }
        }
    }

    fn build_custom_groups(&mut self, indices: &[usize]) {
        let mut grouped: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        let mut other: Vec<usize> = Vec::new();

        for &idx in indices {
            let cwd_str = self.sessions[idx].cwd.to_string_lossy().to_string();
            let mut matched = false;
            for cg in &self.custom_groups {
                for pattern in &cg.patterns {
                    if glob_match::glob_match(pattern, &cwd_str) {
                        grouped.entry(cg.name.clone()).or_default().push(idx);
                        matched = true;
                        break;
                    }
                }
                if matched {
                    break;
                }
            }
            if !matched {
                other.push(idx);
            }
        }

        for (name, group_indices) in &grouped {
            self.sidebar_items
                .push(SidebarItem::GroupHeader(name.clone()));
            for &idx in group_indices {
                self.sidebar_items.push(SidebarItem::Session(idx));
            }
        }

        if !other.is_empty() {
            self.sidebar_items
                .push(SidebarItem::GroupHeader("other".into()));
            for &idx in &other {
                self.sidebar_items.push(SidebarItem::Session(idx));
            }
        }
    }

    /// Restore selected_index from selected_pane after sidebar rebuild.
    fn restore_selection(&mut self) {
        if let Some(ref pane_id) = self.selected_pane {
            for (i, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItem::Session(idx) = item {
                    if let Some(s) = self.sessions.get(*idx) {
                        if s.tmux_pane == *pane_id {
                            self.selected_index = i;
                            self.update_preview_from_cache();
                            return;
                        }
                    }
                }
            }
        }
        // Fallback: clamp to valid range
        if !self.sidebar_items.is_empty() {
            self.selected_index = self.selected_index.min(self.sidebar_items.len() - 1);
        } else {
            self.selected_index = 0;
        }
        self.skip_to_nearest_session();
        // Update selected_pane to match new selection
        if let Some(session) = self.selected_session() {
            self.selected_pane = Some(session.tmux_pane.clone());
        } else {
            self.selected_pane = None;
        }
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
                self.pending_preview = Some(pane_id);
            }
        } else {
            self.pane_preview = None;
            self.pending_preview = None;
        }
    }

    /// Legacy: poll sessions via SessionManager (used in tests).
    #[cfg(test)]
    pub fn refresh_sessions(&mut self) {
        let sessions = self.session_manager.poll();
        self.update_sessions(sessions);
    }

    pub fn refresh_preview(&mut self) {
        if let Some(session) = self.selected_session() {
            self.pending_preview = Some(session.tmux_pane.clone());
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

    #[cfg(test)]
    pub fn selected_session_index(&self) -> Option<usize> {
        match self.sidebar_items.get(self.selected_index) {
            Some(SidebarItem::Session(idx)) => Some(*idx),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        trace!(?key, "handle_key");
        // Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        // Help overlay toggle
        if key.code == KeyCode::Char('?') && !self.search_mode {
            self.show_help_overlay = !self.show_help_overlay;
            return;
        }

        // Close help overlay with Esc
        if self.show_help_overlay && key.code == KeyCode::Esc {
            self.show_help_overlay = false;
            return;
        }

        // Block other keys when help overlay is shown
        if self.show_help_overlay {
            return;
        }

        // Kill confirmation mode
        if self.confirm_kill.is_some() {
            match key.code {
                KeyCode::Char('y') => {
                    if let Some(pane_id) = self.confirm_kill.take() {
                        info!(pane_id = %pane_id, "kill confirmed");
                        if let Some(session) = self.sessions.iter().find(|s| s.tmux_pane == pane_id) {
                            if let Err(e) = self.session_manager.kill(session) {
                                warn!("kill failed: {e}");
                                self.error_message = Some(format!("kill failed: {e}"));
                            }
                            self.refresh_requested = true;
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

        let code = key.code;
        let keys = self.keys.clone();

        if code == keys.quit || code == KeyCode::Esc {
            self.running = false;
        } else if code == keys.down || code == KeyCode::Down {
            self.move_selection(1);
        } else if code == keys.up || code == KeyCode::Up {
            self.move_selection(-1);
        } else if code == keys.top {
            self.move_to_top();
        } else if code == keys.bottom {
            self.move_to_bottom();
        } else if code == keys.detail_show {
            self.show_detail = true;
        } else if code == keys.detail_hide {
            self.show_detail = false;
        } else if code == keys.kill {
            if let Some(session) = self.selected_session() {
                self.confirm_kill = Some(session.tmux_pane.clone());
            }
        } else if code == keys.search {
            self.search_mode = true;
            self.search_query.clear();
        } else if code == keys.refresh {
            self.refresh_requested = true;
        } else if code == keys.cycle_group {
            self.cycle_grouping_mode();
        } else if code == keys.passthrough {
            self.enter_passthrough();
        } else if code == keys.attach || code == keys.new_session {
            // Handled in main.rs
        }
    }

    pub fn enter_passthrough(&mut self) {
        if let Some(session) = self.selected_session() {
            info!(pane_id = %session.tmux_pane, "entered passthrough");
            self.passthrough_mode = true;
            self.last_esc_time = None;
        }
    }

    pub fn exit_passthrough(&mut self) {
        self.passthrough_mode = false;
        self.last_esc_time = None;
    }

    fn cycle_grouping_mode(&mut self) {
        let has_custom = !self.custom_groups.is_empty();
        self.grouping_mode = self.grouping_mode.cycle(has_custom);
        info!(mode = %self.grouping_mode.as_str(), "grouping mode changed");
        self.rebuild_sidebar();
        // Save to config (best-effort)
        let _ = config::save_grouping_mode(&self.grouping_mode);
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.apply_filter();
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.apply_filter();
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.apply_filter();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.apply_filter();
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
        if let Some(session) = self.selected_session() {
            self.selected_pane = Some(session.tmux_pane.clone());
        }
        trace!(selected = self.selected_index, "selection changed");
        self.update_preview_from_cache();
    }

    fn move_to_top(&mut self) {
        self.selected_index = 0;
        self.skip_to_nearest_session();
        if let Some(session) = self.selected_session() {
            self.selected_pane = Some(session.tmux_pane.clone());
        }
        self.update_preview_from_cache();
    }

    fn move_to_bottom(&mut self) {
        if !self.sidebar_items.is_empty() {
            self.selected_index = self.sidebar_items.len() - 1;
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
            if let Some(session) = self.selected_session() {
                self.selected_pane = Some(session.tmux_pane.clone());
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
