use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};

use crate::app::{App, SidebarItem};
use crate::protocol::{
    Capability, DetailBlock, ExecPlan, HealthStatus, KvPair, ListQuery, ListResponse, MetricItem,
    Provider, ProviderError, ProviderManifest, ResumeMode, SessionDetail, SessionFacts,
    SessionSummary,
};
use crate::tui::layout::AppLayout;

// ===== Mock Provider =====

struct MockProvider {
    sessions: Vec<SessionSummary>,
    details: BTreeMap<String, SessionDetail>,
}

impl MockProvider {
    fn new(sessions: Vec<SessionSummary>) -> Self {
        let mut details = BTreeMap::new();
        for s in &sessions {
            details.insert(
                s.native_id.clone(),
                SessionDetail {
                    summary: s.clone(),
                    facts: SessionFacts {
                        input_tokens: Some(1000),
                        output_tokens: Some(500),
                        model: Some("claude-opus-4-6".into()),
                        ..Default::default()
                    },
                    meta: BTreeMap::new(),
                    detail_blocks: vec![
                        DetailBlock::Metrics {
                            title: "Token Usage".into(),
                            items: vec![MetricItem {
                                label: "Input".into(),
                                value: 1000,
                                unit: "tokens".into(),
                                max_value: None,
                            }],
                        },
                        DetailBlock::KeyValue {
                            title: "Session Info".into(),
                            pairs: vec![KvPair {
                                key: "Model".into(),
                                value: "claude-opus-4-6".into(),
                            }],
                        },
                    ],
                },
            );
        }
        Self { sessions, details }
    }
}

impl Provider for MockProvider {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            id: "mock".into(),
            name: "Mock".into(),
            version: "0.1.0".into(),
            protocol_version: 1,
            capabilities: vec![
                Capability::ListSessions {
                    searchable: true,
                    sortable_fields: vec![],
                },
                Capability::Resume {
                    modes: vec![ResumeMode::ExactId],
                },
            ],
        }
    }

    fn health(&self) -> HealthStatus {
        HealthStatus {
            available: true,
            message: None,
            data_path: Some("/mock/path".into()),
        }
    }

    fn list_sessions(&self, query: &ListQuery) -> Result<ListResponse, ProviderError> {
        let mut items = self.sessions.clone();
        if let Some(ref search) = query.search {
            let s = search.to_lowercase();
            items.retain(|i| i.title.to_lowercase().contains(&s));
        }
        let total = items.len() as u64;
        Ok(ListResponse {
            items,
            next_cursor: None,
            total: Some(total),
            fetched_at: 0,
        })
    }

    fn session_detail(&self, native_id: &str) -> Result<SessionDetail, ProviderError> {
        self.details.get(native_id).cloned().ok_or(ProviderError {
            code: "NOT_FOUND".into(),
            message: "not found".into(),
            retryable: false,
        })
    }

    fn resume_command(&self, native_id: &str) -> Result<ExecPlan, ProviderError> {
        let session = self.sessions.iter().find(|s| s.native_id == native_id);
        Ok(ExecPlan {
            program: "claude".into(),
            args: vec!["--resume".into(), native_id.into()],
            cwd: session.and_then(|s| s.project_path.clone()),
            env: BTreeMap::new(),
            interactive: true,
            needs_approval: false,
        })
    }
}

fn make_session(id: &str, title: &str, project: &str, updated_at: i64) -> SessionSummary {
    SessionSummary {
        provider_id: "mock".into(),
        native_id: id.into(),
        title: title.into(),
        project_path: Some(project.into()),
        created_at: Some(updated_at - 3600_000),
        updated_at: Some(updated_at),
        git_branch: Some("main".into()),
        message_count: Some(10),
    }
}

fn make_app(sessions: Vec<SessionSummary>) -> App {
    let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider::new(sessions))];
    App::new(providers)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

// ===== Tests =====

#[test]
fn test_app_loads_sessions_grouped_by_project() {
    let app = make_app(vec![
        make_session("s1", "Fix auth", "/code/app", 2000),
        make_session("s2", "Add cache", "/code/app", 1000),
        make_session("s3", "Setup CI", "/code/api", 1500),
    ]);

    // Should have project headers + sessions
    assert!(app.sidebar_items.len() >= 5); // 2 headers + 3 sessions

    let has_headers = app
        .sidebar_items
        .iter()
        .any(|i| matches!(i, SidebarItem::ProjectHeader(_)));
    assert!(has_headers);

    // Selected should be on a session, not a header
    assert!(matches!(
        app.sidebar_items.get(app.selected_index),
        Some(SidebarItem::Session(_))
    ));
}

#[test]
fn test_jk_navigation_skips_headers() {
    let mut app = make_app(vec![
        make_session("s1", "Fix auth", "/code/app", 2000),
        make_session("s2", "Setup CI", "/code/api", 1000),
    ]);

    // Navigate down repeatedly
    for _ in 0..10 {
        app.handle_key(key(KeyCode::Char('j')));
        assert!(
            matches!(
                app.sidebar_items.get(app.selected_index),
                Some(SidebarItem::Session(_))
            ),
            "j navigation landed on a header at index {}",
            app.selected_index
        );
    }

    // Navigate back up repeatedly
    for _ in 0..10 {
        app.handle_key(key(KeyCode::Char('k')));
        assert!(
            matches!(
                app.sidebar_items.get(app.selected_index),
                Some(SidebarItem::Session(_))
            ),
            "k navigation landed on a header at index {}",
            app.selected_index
        );
    }
}

#[test]
fn test_detail_loads_on_selection() {
    let app = make_app(vec![make_session("s1", "Fix auth", "/code/app", 2000)]);

    assert!(app.current_detail.is_some());
    let detail = app.current_detail.as_ref().unwrap();
    assert_eq!(detail.summary.native_id, "s1");
    assert_eq!(detail.facts.input_tokens, Some(1000));
}

#[test]
fn test_detail_updates_on_navigation() {
    let mut app = make_app(vec![
        make_session("s1", "Fix auth", "/code/app", 2000),
        make_session("s2", "Add cache", "/code/app", 1000),
    ]);

    let first_id = app
        .current_detail
        .as_ref()
        .map(|d| d.summary.native_id.clone());

    app.handle_key(key(KeyCode::Char('j')));

    let second_id = app
        .current_detail
        .as_ref()
        .map(|d| d.summary.native_id.clone());

    assert_ne!(first_id, second_id);
}

#[test]
fn test_toggle_detail_panel() {
    let mut app = make_app(vec![make_session("s1", "Fix auth", "/code/app", 2000)]);

    assert!(app.show_detail);
    app.handle_key(key(KeyCode::Char('h')));
    assert!(!app.show_detail);
    app.handle_key(key(KeyCode::Char('l')));
    assert!(app.show_detail);
}

#[test]
fn test_search_filters_sessions() {
    let mut app = make_app(vec![
        make_session("s1", "Fix auth bug", "/code/app", 2000),
        make_session("s2", "Add caching", "/code/app", 1000),
    ]);

    app.handle_key(key(KeyCode::Char('/')));
    assert!(app.search_mode);

    for c in "auth".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }

    app.handle_key(key(KeyCode::Enter));
    assert!(!app.search_mode);

    let session_count = app
        .sidebar_items
        .iter()
        .filter(|i| matches!(i, SidebarItem::Session(_)))
        .count();
    assert_eq!(session_count, 1);
}

#[test]
fn test_search_escape_clears() {
    let mut app = make_app(vec![
        make_session("s1", "Fix auth bug", "/code/app", 2000),
        make_session("s2", "Add caching", "/code/app", 1000),
    ]);

    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Esc));

    assert!(!app.search_mode);
    assert!(app.search_query.is_empty());

    let session_count = app
        .sidebar_items
        .iter()
        .filter(|i| matches!(i, SidebarItem::Session(_)))
        .count();
    assert_eq!(session_count, 2);
}

#[test]
fn test_resume_returns_correct_exec_plan_with_cwd() {
    let mut app = make_app(vec![
        make_session("s1", "Fix auth", "/code/app", 2000),
        make_session("s2", "Setup CI", "/code/api", 1000),
    ]);

    let action = app.resume_selected();
    assert!(action.is_some());

    let action = action.unwrap();
    assert_eq!(action.program, "claude");
    assert!(action.args.contains(&"--resume".to_string()));
    assert!(
        action.cwd.is_some(),
        "cwd must be set for claude --resume to find the session"
    );

    // Verify the cwd matches the selected session's project_path
    if let Some(SidebarItem::Session(s)) = app.sidebar_items.get(app.selected_index) {
        assert_eq!(action.cwd, s.project_path);
    }
}

#[test]
fn test_quit_with_q() {
    let mut app = make_app(vec![]);
    assert!(app.running);
    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.running);
}

#[test]
fn test_quit_with_ctrl_c() {
    let mut app = make_app(vec![]);
    assert!(app.running);
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(!app.running);
}

#[test]
fn test_empty_state() {
    let app = make_app(vec![]);
    assert!(app.sidebar_items.is_empty());
    assert!(app.current_detail.is_none());
    assert!(app.error_message.is_none());
}

#[test]
fn test_g_and_shift_g_navigation() {
    let mut app = make_app(vec![
        make_session("s1", "First", "/code/app", 3000),
        make_session("s2", "Second", "/code/app", 2000),
        make_session("s3", "Third", "/code/app", 1000),
    ]);

    // G goes to bottom
    app.handle_key(key(KeyCode::Char('G')));
    assert!(matches!(
        app.sidebar_items.get(app.selected_index),
        Some(SidebarItem::Session(s)) if s.native_id == "s3"
    ));

    // g goes to top (first session, not header)
    app.handle_key(key(KeyCode::Char('g')));
    assert!(matches!(
        app.sidebar_items.get(app.selected_index),
        Some(SidebarItem::Session(s)) if s.native_id == "s1"
    ));
}

#[test]
fn test_render_does_not_panic_with_cjk() {
    let app = make_app(vec![
        make_session("s1", "Fix auth", "/code/app", 2000),
        make_session("s2", "修复图片加载失败显示问题", "/code/app", 1000),
        make_session("s3", "Setup CI", "/code/api", 500),
    ]);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let layout = AppLayout::new(frame.area(), app.show_detail);

            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                app.selected_index,
                true,
            );

            if let Some(detail_area) = layout.detail {
                crate::tui::detail::render(frame, detail_area, app.current_detail.as_ref());
            }

            crate::tui::help::render_with_mode(frame, layout.help_bar, false, "", false, false);
        })
        .unwrap();
}

#[test]
fn test_render_narrow_terminal() {
    let app = make_app(vec![
        make_session("s1", "A very long session title that should be truncated", "/code/app", 2000),
    ]);

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let layout = AppLayout::new(frame.area(), false);

            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                app.selected_index,
                true,
            );
        })
        .unwrap();
}

#[test]
fn test_render_with_detail_collapsed() {
    let app = make_app(vec![make_session("s1", "Fix auth", "/code/app", 2000)]);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let layout = AppLayout::new(frame.area(), false);
            assert!(layout.detail.is_none());

            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                app.selected_index,
                true,
            );
        })
        .unwrap();
}
