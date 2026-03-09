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
use std::time::{Duration, Instant};

use ansi_to_tui::IntoText;
use crossterm::event::{KeyCode, KeyModifiers};
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
const PASSTHROUGH_PREVIEW_TICKS: u32 = 1; // 1 * 100ms = 100ms
const DOUBLE_ESC_TIMEOUT: Duration = Duration::from_millis(300);

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
                app.passthrough_mode,
            );
        })?;

        // Handle events
        if let Some(ev) = event::poll_event(Duration::from_millis(100))? {
            match ev {
                event::AppEvent::Key(key) => {
                    // Passthrough mode: forward keys to tmux pane
                    if app.passthrough_mode {
                        if let Some(session) = app.selected_session() {
                            let pane_id = session.tmux_pane.clone();
                            handle_passthrough_key(&mut app, key, &pane_id);
                        } else {
                            app.exit_passthrough();
                        }

                        if let Some(pane_id) = app.pending_preview.take() {
                            let _ = bg_tx.send(BgRequest::Capture { pane_id });
                        }
                        continue;
                    }

                    if key.code == KeyCode::Enter
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
                    } else if key.code == KeyCode::Char('n')
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
                    } else {
                        let preview_interval = if app.passthrough_mode {
                            PASSTHROUGH_PREVIEW_TICKS
                        } else {
                            PREVIEW_INTERVAL_TICKS
                        };
                        if preview_counter >= preview_interval {
                            preview_counter = 0;
                            app.refresh_preview();
                        }
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

/// Handle a key event in passthrough mode: double-Esc exits, everything else forwarded.
fn handle_passthrough_key(app: &mut App, key: crossterm::event::KeyEvent, pane_id: &str) {
    if key.code == KeyCode::Esc {
        if let Some(first_esc) = app.last_esc_time {
            if first_esc.elapsed() < DOUBLE_ESC_TIMEOUT {
                // Double-Esc: exit passthrough (don't send second Esc)
                app.exit_passthrough();
                return;
            }
        }
        // First Esc (or timeout expired): record time, send pending Esc if any
        if app.last_esc_time.is_some() {
            // Previous Esc timed out — send it now
            let _ = TmuxController::send_keys(pane_id, &["Escape"]);
        }
        app.last_esc_time = Some(Instant::now());
        return;
    }

    // Non-Esc key: flush pending Esc if any, then send this key
    if app.last_esc_time.take().is_some() {
        let _ = TmuxController::send_keys(pane_id, &["Escape"]);
    }

    send_key_to_tmux(key, pane_id);
    // Trigger immediate preview refresh
    app.refresh_preview();
}

/// Map a crossterm KeyEvent to tmux send-keys and dispatch.
fn send_key_to_tmux(key: crossterm::event::KeyEvent, pane_id: &str) {
    let has_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char(c) if has_ctrl => {
            let tmux_key = format!("C-{c}");
            let _ = TmuxController::send_keys(pane_id, &[&tmux_key]);
        }
        KeyCode::Char(c) => {
            let _ = TmuxController::send_text(pane_id, &c.to_string());
        }
        KeyCode::Enter => {
            let _ = TmuxController::send_keys(pane_id, &["Enter"]);
        }
        KeyCode::Backspace => {
            let _ = TmuxController::send_keys(pane_id, &["BSpace"]);
        }
        KeyCode::Tab => {
            let _ = TmuxController::send_keys(pane_id, &["Tab"]);
        }
        KeyCode::Esc => {
            let _ = TmuxController::send_keys(pane_id, &["Escape"]);
        }
        KeyCode::Up => {
            let _ = TmuxController::send_keys(pane_id, &["Up"]);
        }
        KeyCode::Down => {
            let _ = TmuxController::send_keys(pane_id, &["Down"]);
        }
        KeyCode::Left => {
            let _ = TmuxController::send_keys(pane_id, &["Left"]);
        }
        KeyCode::Right => {
            let _ = TmuxController::send_keys(pane_id, &["Right"]);
        }
        KeyCode::Home => {
            let _ = TmuxController::send_keys(pane_id, &["Home"]);
        }
        KeyCode::End => {
            let _ = TmuxController::send_keys(pane_id, &["End"]);
        }
        KeyCode::PageUp => {
            let _ = TmuxController::send_keys(pane_id, &["PageUp"]);
        }
        KeyCode::PageDown => {
            let _ = TmuxController::send_keys(pane_id, &["PageDown"]);
        }
        KeyCode::Delete => {
            let _ = TmuxController::send_keys(pane_id, &["DC"]);
        }
        _ => {} // Unsupported keys silently ignored
    }
}

fn render_main(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let base_title = app
        .selected_session()
        .map(|s| format!(" {} — {} ", s.provider, s.cwd.display()))
        .unwrap_or_else(|| " LazyAgent ".into());

    let title = if app.passthrough_mode {
        format!(" PASSTHROUGH | {}", base_title.trim())
    } else {
        base_title
    };

    let border_style = if app.passthrough_mode {
        Theme::passthrough_border()
    } else {
        Theme::border_unfocused()
    };

    let block = Block::default()
        .title(title)
        .title_style(Theme::title())
        .borders(Borders::ALL)
        .border_style(border_style);

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
