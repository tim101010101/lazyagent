mod app;
mod bg;
mod config;
mod event;
mod protocol;
mod provider;
mod session;
mod tmux;
mod tui;

use std::io::IsTerminal;
use std::time::Duration;

use ansi_to_tui::IntoText;
use ratatui::{
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use app::App;
use bg::BgRequest;
use provider::claude::ClaudeProvider;
use session::SessionManager;
use tmux::TmuxController;
use tui::layout::AppLayout;
use tui::theme::Theme;

const REFRESH_INTERVAL_TICKS: u32 = 20; // 20 * 100ms = 2s
const PREVIEW_INTERVAL_TICKS: u32 = 2; // 2 * 100ms = 200ms

fn main() -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("lazyagent requires an interactive terminal (TTY)");
    }

    if !TmuxController::tmux_available() {
        anyhow::bail!("lazyagent requires tmux. Please install tmux and try again.");
    }

    // Restore terminal on panic
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = tui::restore();
        default_panic(info);
    }));

    // Providers for main thread (attach/spawn)
    let providers: Vec<Box<dyn protocol::Provider>> = vec![Box::new(ClaudeProvider::new())];
    let session_manager = SessionManager::new(providers);
    let mut app = App::new(session_manager);
    app.load_config();

    // Background worker with its own provider instances
    let bg_providers: Vec<Box<dyn protocol::Provider>> = vec![Box::new(ClaudeProvider::new())];
    let (bg_tx, bg_rx, bg_handle) = bg::spawn_worker(bg_providers);

    // Trigger initial refresh
    let _ = bg_tx.send(BgRequest::Refresh);

    let mut terminal = tui::init()?;
    let mut tick_counter: u32 = 0;
    let mut preview_counter: u32 = 0;

    while app.running {
        // Drain all pending bg updates (non-blocking)
        while let Ok(update) = bg_rx.try_recv() {
            app.apply_bg_update(update);
        }

        // Send pending preview request
        if let Some(pane_id) = app.pending_preview.take() {
            let _ = bg_tx.send(BgRequest::Capture { pane_id });
        }

        // Draw
        terminal.draw(|frame| {
            let layout = AppLayout::new(frame.area(), app.show_detail);

            // Sidebar
            tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                &app.sessions,
                app.selected_index,
                true,
                &app.grouping_mode,
            );

            // Main area — pane preview
            render_main(frame, layout.main, &app);

            // Detail panel
            if let Some(detail_area) = layout.detail {
                tui::detail::render(frame, detail_area, app.selected_session());
            }

            // Help bar
            tui::help::render(
                frame,
                layout.help_bar,
                app.search_mode,
                &app.search_query,
                app.confirm_kill.is_some(),
            );
        })?;

        // Handle events
        if let Some(ev) = event::poll_event(Duration::from_millis(100))? {
            match ev {
                event::AppEvent::Key(key) => {
                    if key.code == crossterm::event::KeyCode::Enter
                        && !app.search_mode
                        && app.confirm_kill.is_none()
                    {
                        // Attach to selected session
                        if let Some(session) = app.selected_session() {
                            let mut cmd = app.session_manager().attach_command(session);
                            tui::restore()?;
                            let _ = cmd.status();
                            terminal = tui::init()?;
                            let _ = bg_tx.send(BgRequest::Refresh);
                            continue;
                        }
                    } else if key.code == crossterm::event::KeyCode::Char('n')
                        && !app.search_mode
                        && app.confirm_kill.is_none()
                    {
                        // Spawn new session
                        if let Some(provider_id) = app.default_provider_id() {
                            let cwd = std::env::current_dir().unwrap_or_default();
                            match app.session_manager().spawn(&provider_id, &cwd) {
                                Ok(session_name) => {
                                    let mut cmd = TmuxController::attach_command(&session_name);
                                    tui::restore()?;
                                    let _ = cmd.status();
                                    terminal = tui::init()?;
                                    let _ = bg_tx.send(BgRequest::Refresh);
                                }
                                Err(e) => {
                                    app.error_message = Some(format!("spawn failed: {e}"));
                                }
                            }
                            continue;
                        }
                    }
                    app.handle_key(key);

                    // Send capture immediately on navigation (don't wait for next tick)
                    if let Some(pane_id) = app.pending_preview.take() {
                        let _ = bg_tx.send(BgRequest::Capture { pane_id });
                    }
                }
                event::AppEvent::Tick => {
                    tick_counter += 1;
                    preview_counter += 1;

                    if tick_counter >= REFRESH_INTERVAL_TICKS {
                        tick_counter = 0;
                        let _ = bg_tx.send(BgRequest::Refresh);
                        preview_counter = 0;
                    } else if preview_counter >= PREVIEW_INTERVAL_TICKS {
                        preview_counter = 0;
                        app.refresh_preview();
                    }
                }
            }
        }
    }

    // Shutdown background worker
    let _ = bg_tx.send(BgRequest::Shutdown);
    let _ = bg_handle.join();

    tui::restore()?;
    Ok(())
}

fn render_main(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let title = app
        .selected_session()
        .map(|s| format!(" {} — {} ", s.provider, s.cwd.display()))
        .unwrap_or_else(|| " LazyAgent ".into());

    let block = Block::default()
        .title(title)
        .title_style(Theme::title())
        .borders(Borders::ALL)
        .border_style(Theme::border_unfocused());

    if let Some(ref preview) = app.pane_preview {
        // Parse ANSI escape codes into styled ratatui Text
        let text: Text = preview
            .as_bytes()
            .into_text()
            .unwrap_or_else(|_| Text::raw(preview));

        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    } else if let Some(ref err) = app.error_message {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(err.as_str(), Theme::error())),
        ];
        let paragraph = Paragraph::new(content).block(block);
        frame.render_widget(paragraph, area);
    } else if app.sessions.is_empty() {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No active agent sessions found.",
                Theme::label(),
            )),
            Line::from(Span::styled(
                "  Press 'n' to start a new session.",
                Theme::label(),
            )),
        ];
        let paragraph = Paragraph::new(content).block(block);
        frame.render_widget(paragraph, area);
    } else {
        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Select a session to preview",
                Theme::label(),
            )),
        ];
        let paragraph = Paragraph::new(content).block(block);
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod e2e_tests;
