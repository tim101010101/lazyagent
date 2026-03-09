use std::path::PathBuf;
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};

use crate::app::{App, GroupingMode, SidebarItem};
use crate::config::CustomGroup;
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
        git_root: None,
    }
}

fn make_session_with_root(
    provider: &str,
    cwd: &str,
    status: AgentStatus,
    tmux_session: &str,
    source: SessionSource,
    started_secs_ago: u64,
    git_root: Option<&str>,
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
        git_root: git_root.map(|s| s.to_string()),
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
fn test_flat_mode_no_group_headers() {
    let mut app = make_app(vec![
        make_session("mock", "/code/proj-a/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
        make_session("mock", "/code/proj-a/api", AgentStatus::Thinking, "la/mock/api", SessionSource::Local, 200),
        make_session("mock", "/code/proj-b/web", AgentStatus::Idle, "la/mock/web", SessionSource::Local, 300),
    ]);

    app.grouping_mode = GroupingMode::Flat;
    app.rebuild_sidebar();

    assert_eq!(app.grouping_mode, GroupingMode::Flat);
    let source_headers = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::SourceHeader(_))).count();
    let group_headers = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::GroupHeader(_))).count();
    let session_items = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::Session(_))).count();

    assert_eq!(source_headers, 1, "should have 1 source header (local)");
    assert_eq!(group_headers, 0, "flat mode should have no group headers");
    assert_eq!(session_items, 3, "should have 3 sessions");
}

#[test]
fn test_git_root_mode_groups_by_root() {
    let mut app = make_app(vec![
        make_session_with_root("mock", "/code/repo-a/src", AgentStatus::Waiting, "la/mock/src", SessionSource::Local, 100, Some("repo-a")),
        make_session_with_root("mock", "/code/repo-a/lib", AgentStatus::Thinking, "la/mock/lib", SessionSource::Local, 200, Some("repo-a")),
        make_session_with_root("mock", "/code/repo-b/app", AgentStatus::Idle, "la/mock/app", SessionSource::Local, 300, Some("repo-b")),
    ]);

    app.grouping_mode = GroupingMode::GitRoot;
    app.rebuild_sidebar();

    let group_headers: Vec<_> = app.sidebar_items.iter()
        .filter_map(|i| if let SidebarItem::GroupHeader(name) = i { Some(name.clone()) } else { None })
        .collect();
    assert_eq!(group_headers, vec!["repo-a", "repo-b"]);

    let session_items = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::Session(_))).count();
    assert_eq!(session_items, 3);
}

#[test]
fn test_git_root_mode_ungrouped_bucket() {
    let mut app = make_app(vec![
        make_session_with_root("mock", "/code/repo-a/src", AgentStatus::Waiting, "la/mock/src", SessionSource::Local, 100, Some("repo-a")),
        make_session_with_root("mock", "/tmp/scratch", AgentStatus::Idle, "la/mock/scratch", SessionSource::Local, 200, None),
    ]);

    app.grouping_mode = GroupingMode::GitRoot;
    app.rebuild_sidebar();

    let group_headers: Vec<_> = app.sidebar_items.iter()
        .filter_map(|i| if let SidebarItem::GroupHeader(name) = i { Some(name.clone()) } else { None })
        .collect();
    assert!(group_headers.contains(&"repo-a".to_string()));
    assert!(group_headers.contains(&"ungrouped".to_string()));
}

#[test]
fn test_custom_mode_pattern_matching() {
    let mut app = make_app(vec![
        make_session("mock", "/code/work/proj", AgentStatus::Waiting, "la/mock/proj", SessionSource::Local, 100),
        make_session("mock", "/code/personal/blog", AgentStatus::Idle, "la/mock/blog", SessionSource::Local, 200),
        make_session("mock", "/tmp/random", AgentStatus::Unknown, "la/mock/random", SessionSource::Local, 300),
    ]);

    app.custom_groups = vec![
        CustomGroup { name: "Work".into(), patterns: vec!["**/work/**".into()] },
        CustomGroup { name: "Personal".into(), patterns: vec!["**/personal/**".into()] },
    ];
    app.grouping_mode = GroupingMode::Custom;
    app.rebuild_sidebar();

    let group_headers: Vec<_> = app.sidebar_items.iter()
        .filter_map(|i| if let SidebarItem::GroupHeader(name) = i { Some(name.clone()) } else { None })
        .collect();
    assert!(group_headers.contains(&"Work".to_string()));
    assert!(group_headers.contains(&"Personal".to_string()));
    assert!(group_headers.contains(&"other".to_string()), "unmatched should go to 'other'");
}

#[test]
fn test_mode_cycling_skips_custom_when_no_groups() {
    let mut app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
    ]);

    // No custom groups — should skip custom
    app.grouping_mode = GroupingMode::Flat;
    app.rebuild_sidebar();
    assert_eq!(app.grouping_mode, GroupingMode::Flat);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.grouping_mode, GroupingMode::GitRoot);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.grouping_mode, GroupingMode::Flat); // skipped custom
}

#[test]
fn test_mode_cycling_includes_custom_when_groups_exist() {
    let mut app = make_app(vec![
        make_session("mock", "/code/app", AgentStatus::Waiting, "la/mock/app", SessionSource::Local, 100),
    ]);

    app.custom_groups = vec![
        CustomGroup { name: "Work".into(), patterns: vec!["**/code/**".into()] },
    ];

    app.grouping_mode = GroupingMode::Flat;
    app.rebuild_sidebar();

    assert_eq!(app.grouping_mode, GroupingMode::Flat);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.grouping_mode, GroupingMode::GitRoot);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.grouping_mode, GroupingMode::Custom);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.grouping_mode, GroupingMode::Flat);
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
fn test_navigation_skips_headers_git_mode() {
    let mut app = make_app(vec![
        make_session_with_root("mock", "/code/repo-a/src", AgentStatus::Waiting, "la/mock/src", SessionSource::Local, 100, Some("repo-a")),
        make_session_with_root("mock", "/code/repo-b/app", AgentStatus::Idle, "la/mock/app", SessionSource::Local, 200, Some("repo-b")),
    ]);

    app.grouping_mode = GroupingMode::GitRoot;
    app.rebuild_sidebar();

    for _ in 0..10 {
        app.handle_key(key(KeyCode::Char('j')));
        assert!(
            matches!(app.sidebar_items.get(app.selected_index), Some(SidebarItem::Session(_))),
            "j landed on non-session at index {}",
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
            let layout = AppLayout::new(frame.area(), app.show_detail, &app.layout_config);
            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                &app.sessions,
                app.selected_index,
                true,
                &app.grouping_mode,
                app.tick,
                &app.theme,
                &app.sidebar_config,
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
            let layout = AppLayout::new(frame.area(), true, &app.layout_config);
            if let Some(detail_area) = layout.detail {
                crate::tui::detail::render(frame, detail_area, app.selected_session(), &app.theme);
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
            let layout = AppLayout::new(frame.area(), false, &app.layout_config);
            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                &app.sessions,
                app.selected_index,
                true,
                &app.grouping_mode,
                app.tick,
                &app.theme,
                &app.sidebar_config,
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
            let layout = AppLayout::new(frame.area(), false, &app.layout_config);
            crate::tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                &app.sessions,
                app.selected_index,
                true,
                &app.grouping_mode,
                app.tick,
                &app.theme,
                &app.sidebar_config,
            );
        })
        .unwrap();
}

#[test]
fn test_grouping_mode_label() {
    assert_eq!(GroupingMode::Flat.label(), "flat");
    assert_eq!(GroupingMode::GitRoot.label(), "git");
    assert_eq!(GroupingMode::Custom.label(), "custom");
}

#[test]
fn test_git_root_preserved_through_update_sessions() {
    let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider)];
    let sm = SessionManager::with_sessions(providers, vec![]);
    let mut app = App::new(sm);

    // Simulate bg worker delivering sessions with pre-computed git_root
    let sessions = vec![
        make_session_with_root("mock", "/code/repo-a/src", AgentStatus::Waiting, "la/mock/src", SessionSource::Local, 100, Some("repo-a")),
        make_session_with_root("mock", "/code/repo-b/app", AgentStatus::Idle, "la/mock/app", SessionSource::Local, 200, Some("repo-b")),
    ];

    app.grouping_mode = GroupingMode::GitRoot;
    app.update_sessions(sessions);

    // git_root should flow through to sidebar grouping
    let group_headers: Vec<_> = app.sidebar_items.iter()
        .filter_map(|i| if let SidebarItem::GroupHeader(name) = i { Some(name.clone()) } else { None })
        .collect();
    assert_eq!(group_headers, vec!["repo-a", "repo-b"]);
}

#[test]
fn test_git_root_none_becomes_ungrouped_in_update_sessions() {
    let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider)];
    let sm = SessionManager::with_sessions(providers, vec![]);
    let mut app = App::new(sm);

    // Sessions without git_root (e.g. non-git dirs)
    let sessions = vec![
        make_session("mock", "/tmp/scratch", AgentStatus::Waiting, "la/mock/scratch", SessionSource::Local, 100),
    ];

    app.grouping_mode = GroupingMode::GitRoot;
    app.update_sessions(sessions);

    let group_headers: Vec<_> = app.sidebar_items.iter()
        .filter_map(|i| if let SidebarItem::GroupHeader(name) = i { Some(name.clone()) } else { None })
        .collect();
    assert_eq!(group_headers, vec!["ungrouped"]);
}

#[test]
fn test_mixed_git_root_and_none_grouping() {
    let providers: Vec<Box<dyn Provider>> = vec![Box::new(MockProvider)];
    let sm = SessionManager::with_sessions(providers, vec![]);
    let mut app = App::new(sm);

    let sessions = vec![
        make_session_with_root("mock", "/code/repo/src", AgentStatus::Waiting, "la/mock/src", SessionSource::Local, 100, Some("repo")),
        make_session("mock", "/tmp/no-git", AgentStatus::Idle, "la/mock/nogit", SessionSource::Local, 200),
        make_session_with_root("mock", "/code/repo/lib", AgentStatus::Thinking, "la/mock/lib", SessionSource::Local, 300, Some("repo")),
    ];

    app.grouping_mode = GroupingMode::GitRoot;
    app.update_sessions(sessions);

    let group_headers: Vec<_> = app.sidebar_items.iter()
        .filter_map(|i| if let SidebarItem::GroupHeader(name) = i { Some(name.clone()) } else { None })
        .collect();
    assert!(group_headers.contains(&"repo".to_string()));
    assert!(group_headers.contains(&"ungrouped".to_string()));

    // 2 sessions under "repo", 1 under "ungrouped"
    let session_count = app.sidebar_items.iter().filter(|i| matches!(i, SidebarItem::Session(_))).count();
    assert_eq!(session_count, 3);
}
