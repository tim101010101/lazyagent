use std::path::PathBuf;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};

use crate::app::{App, SidebarItem};
use crate::protocol::{
    AgentSession, AgentStatus, ExecPlan, Provider, ProviderManifest, SessionKind, SessionSource,
};
use crate::session::SessionManager;
use crate::tui::layout::AppLayout;

// ===== Mock Provider =====

struct MockProvider;

impl Provider for MockProvider {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            id: "mock".into(),
            name: "Mock".into(),
        }
    }

    fn detect_status(&self, pane_output: &str) -> AgentStatus {
        if pane_output.contains("Error") {
            AgentStatus::Error
        } else if pane_output.contains("thinking") {
            AgentStatus::Thinking
        } else if pane_output.contains(">") {
            AgentStatus::Waiting
        } else {
            AgentStatus::Unknown
        }
    }

    fn match_process(&self, process_name: &str) -> bool {
        process_name == "mock-agent"
    }

    fn exec_plan(&self, cwd: &std::path::Path) -> ExecPlan {
        ExecPlan {
            program: "mock-agent".into(),
            args: vec![],
            cwd: Some(cwd.to_string_lossy().into()),
            env: std::collections::BTreeMap::new(),
        }
    }
}

// ===== Helpers =====

fn make_session(
    provider: &str,
    cwd: &str,
    status: AgentStatus,
    tmux_session: &str,
    source: SessionSource,
    started_secs_ago: u64,
) -> AgentSession {
    AgentSession {
        kind: SessionKind::Managed,
        tmux_session: tmux_session.into(),
        tmux_pane: "%0".into(),
        provider: provider.into(),
        cwd: PathBuf::from(cwd),
        status,
        started_at: Some(SystemTime::now() - std::time::Duration::from_secs(started_secs_ago)),
        source,
    }
}

fn make_app(sessions: Vec<AgentSession>) -> App {
    let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider)];
    let sm = SessionManager::with_sessions(providers, sessions);
    let mut app = App::new(sm);
    app.refresh_sessions(); // populate from mock sessions
    app
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

// ===== Tests =====

#[test]
fn test_sessions_grouped_by_project() {
    let app = make_app(vec![
        make_session("mock", "/code/proj-a/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
        make_session("mock", "/code/proj-a/api", AgentStatus::Thinking, "la/mock/api", SessionSource::Local, 200),
        make_session("mock", "/code/proj-b/web", AgentStatus::Idle, "la/mock/web", SessionSource::Local, 300),
    ]);

    // Should have source header + project headers + sessions
    let source_headers = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::SourceHeader(_))).count();
    let project_headers = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::ProjectHeader(_))).count();
    let session_items = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::Session(_))).count();

    assert_eq!(source_headers, 1, "should have 1 source header (local)");
    assert!(project_headers >= 2, "should have project headers for grouping");
    assert_eq!(session_items, 3, "should have 3 sessions");
}

#[test]
fn test_navigation_skips_headers() {
    let mut app = make_app(vec![
        make_session("mock", "/code/proj-a/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
        make_session("mock", "/code/proj-b/web", AgentStatus::Idle, "la/mock/web", SessionSource::Local, 200),
    ]);

    // Navigate down repeatedly
    for _ in 0..10 {
        app.handle_key(key(KeyCode::Char('j')));
        assert!(
            matches!(app.sidebar_items.get(app.selected_index), Some(SidebarItem::Session(_))),
            "j landed on non-session at index {}",
            app.selected_index
        );
    }

    // Navigate up repeatedly
    for _ in 0..10 {
        app.handle_key(key(KeyCode::Char('k')));
        assert!(
            matches!(app.sidebar_items.get(app.selected_index), Some(SidebarItem::Session(_))),
            "k landed on non-session at index {}",
            app.selected_index
        );
    }
}

#[test]
fn test_search_filters_by_cwd() {
    let mut app = make_app(vec![
        make_session("mock", "/code/proj-a/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
        make_session("mock", "/code/proj-b/web", AgentStatus::Idle, "la/mock/web", SessionSource::Local, 200),
    ]);

    app.handle_key(key(KeyCode::Char('/')));
    assert!(app.search_mode);

    for c in "proj-a".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    let session_count = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::Session(_))).count();
    assert_eq!(session_count, 1);
}

#[test]
fn test_quit_keys() {
    let mut app = make_app(vec![]);
    assert!(app.running);
    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.running);

    let mut app2 = make_app(vec![]);
    app2.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(!app2.running);
}

#[test]
fn test_empty_state() {
    let app = make_app(vec![]);
    assert!(app.sidebar_items.is_empty());
    assert!(app.sessions.is_empty());
    assert!(app.selected_session().is_none());
    assert!(app.error_message.is_none());
}

#[test]
fn test_status_icons_render() {
    let app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
        make_session("mock", "/code/api", AgentStatus::Thinking, "la/mock/api", SessionSource::Local, 200),
        make_session("mock", "/code/web", AgentStatus::Error, "la/mock/web", SessionSource::Local, 300),
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
                &app.sessions,
                app.selected_index,
                true,
            );
        })
        .unwrap();
}

#[test]
fn test_detail_shows_selected_session() {
    let app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
    ]);

    let session = app.selected_session();
    assert!(session.is_some());
    assert_eq!(session.unwrap().provider, "mock");
    assert_eq!(session.unwrap().cwd, PathBuf::from("/code/app"));

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let layout = AppLayout::new(frame.area(), true);
            if let Some(detail_area) = layout.detail {
                crate::tui::detail::render(frame, detail_area, app.selected_session());
            }
        })
        .unwrap();
}

#[test]
fn test_kill_confirmation_flow() {
    let mut app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
    ]);

    assert!(app.confirm_kill.is_none());

    // Press 'd' to initiate kill
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.confirm_kill.is_some());

    // Press 'n' to cancel
    app.handle_key(key(KeyCode::Char('n')));
    assert!(app.confirm_kill.is_none());

    // Press 'd' again
    app.handle_key(key(KeyCode::Char('d')));
    assert!(app.confirm_kill.is_some());

    // Other keys during confirm should cancel
    app.handle_key(key(KeyCode::Esc));
    assert!(app.confirm_kill.is_none());
}

#[test]
fn test_cjk_rendering_safety() {
    let app = make_app(vec![
        make_session("mock", "/code/修复图片/app", AgentStatus::Unknown, "la/mock/app", SessionSource::Local, 100),
    ]);

    let backend = TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let layout = AppLayout::new(frame.area(), false);
            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                &app.sessions,
                app.selected_index,
                true,
            );
        })
        .unwrap();
}

#[test]
fn test_g_and_shift_g_navigation() {
    let mut app = make_app(vec![
        make_session("mock", "/code/proj/a", AgentStatus::Waiting, "la/mock/a", SessionSource::Local, 100),
        make_session("mock", "/code/proj/b", AgentStatus::Waiting, "la/mock/b", SessionSource::Local, 200),
        make_session("mock", "/code/proj/c", AgentStatus::Waiting, "la/mock/c", SessionSource::Local, 300),
    ]);

    // G goes to bottom
    app.handle_key(key(KeyCode::Char('G')));
    assert!(matches!(
        app.sidebar_items.get(app.selected_index),
        Some(SidebarItem::Session(_))
    ));

    // g goes to top (first session, not header)
    app.handle_key(key(KeyCode::Char('g')));
    assert!(matches!(
        app.sidebar_items.get(app.selected_index),
        Some(SidebarItem::Session(_))
    ));
}

#[test]
fn test_toggle_detail_panel() {
    let mut app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
    ]);

    assert!(app.show_detail);
    app.handle_key(key(KeyCode::Char('h')));
    assert!(!app.show_detail);
    app.handle_key(key(KeyCode::Char('l')));
    assert!(app.show_detail);
}

#[test]
fn test_search_escape_clears() {
    let mut app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
        make_session("mock", "/code/api", AgentStatus::Idle, "la/mock/api", SessionSource::Local, 200),
    ]);

    app.handle_key(key(KeyCode::Char('/')));
    app.handle_key(key(KeyCode::Char('x')));
    app.handle_key(key(KeyCode::Esc));

    assert!(!app.search_mode);
    assert!(app.search_query.is_empty());

    let session_count = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::Session(_))).count();
    assert_eq!(session_count, 2);
}

#[test]
fn test_render_narrow_terminal() {
    let app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
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
                &app.sessions,
                app.selected_index,
                true,
            );
        })
        .unwrap();
}
