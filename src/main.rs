mod app;
mod event;
mod protocol;
mod provider;
mod tmux;
mod tui;

use std::io::IsTerminal;
use std::process::Command;
use std::time::Duration;

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use app::App;
use provider::claude::ClaudeProvider;
use tmux::TmuxController;
use tui::layout::AppLayout;
use tui::theme::Theme;

fn main() -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("lazyagent requires an interactive terminal (TTY). Please run it directly in a terminal emulator.");
    }

    // Auto-start tmux if not already inside and tmux is available
    let no_tmux = std::env::var("LAZYAGENT_NO_TMUX").is_ok();
    if !no_tmux && std::env::var("TMUX").is_err() && TmuxController::tmux_available() {
        #[cfg(unix)]
        TmuxController::auto_start(); // never returns
    }

    // Restore terminal on panic
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = tui::restore();
        default_panic(info);
    }));

    // Register providers
    let providers: Vec<Box<dyn protocol::Provider>> = vec![Box::new(ClaudeProvider::new())];

    let mut app = App::new(providers);
    let mut terminal = tui::init()?;

    while app.running {
        let tmux_mode = app.tmux_mode();
        // Use vertical stack only when agent pane is open (lazyagent pane is narrow)
        let tmux_narrow = tmux_mode && app.agent_running;

        // Draw
        terminal.draw(|frame| {
            let layout = if tmux_narrow {
                AppLayout::tmux(frame.area(), app.show_detail)
            } else {
                AppLayout::new(frame.area(), app.show_detail)
            };

            // Sidebar
            tui::sidebar::render(
                frame,
                layout.sidebar,
                &app.sidebar_items,
                app.selected_index,
                app.panel == app::Panel::Sidebar,
            );

            // Main area (only when not in narrow tmux mode)
            if !tmux_narrow {
                render_main(frame, layout.main, &app);
            }

            // Detail panel
            if let Some(detail_area) = layout.detail {
                tui::detail::render(frame, detail_area, app.current_detail.as_ref());
            }

            // Help bar
            tui::help::render_with_mode(
                frame,
                layout.help_bar,
                app.search_mode,
                &app.search_query,
                tmux_mode,
                app.agent_running,
            );
        })?;

        // Handle events
        if let Some(ev) = event::poll_event(Duration::from_millis(100))? {
            match ev {
                event::AppEvent::Key(key) => {
                    // Check for resume action before normal key handling
                    if key.code == crossterm::event::KeyCode::Enter && !app.search_mode {
                        if tmux_mode {
                            // Tmux mode: launch agent in split pane
                            if let Some(plan) = app.exec_plan_for_selected() {
                                if let Some(ref mut tmux) = app.tmux {
                                    match tmux.launch_agent(&plan) {
                                        Ok(()) => {
                                            app.agent_running = true;
                                            app.show_detail = false;
                                            let _ = tmux.focus_agent();
                                        }
                                        Err(e) => {
                                            app.error_message = Some(format!("tmux: {e}"));
                                        }
                                    }
                                }
                                continue;
                            }
                        } else {
                            // Fallback: suspend-exec mode
                            if let Some(action) = app.resume_selected() {
                                tui::restore()?;
                                let mut cmd = Command::new(&action.program);
                                cmd.args(&action.args);
                                if let Some(cwd) = &action.cwd {
                                    cmd.current_dir(cwd);
                                }
                                cmd.env_remove("CLAUDECODE");
                                let _ = cmd.status();
                                terminal = tui::init()?;
                                app.refresh_sessions();
                                continue;
                            }
                        }
                    }
                    app.handle_key(key);
                }
                event::AppEvent::Tick => {
                    // In tmux mode, check if agent pane is still alive
                    if tmux_mode && app.agent_running {
                        let alive = app.tmux.as_ref().map_or(false, |t| t.is_agent_alive());
                        if !alive {
                            app.agent_running = false;
                            app.refresh_sessions();
                        }
                    }
                }
            }
        }
    }

    // Kill agent pane on quit
    if let Some(ref mut tmux) = app.tmux {
        tmux.kill_agent();
    }

    tui::restore()?;
    Ok(())
}

fn render_main(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" LazyAgent ")
        .title_style(Theme::title())
        .borders(Borders::ALL)
        .border_style(Theme::border_unfocused());

    let content = if let Some(ref msg) = app.health_message {
        vec![
            Line::from(""),
            Line::from(Span::styled(msg.as_str(), Theme::error())),
        ]
    } else if let Some(ref err) = app.error_message {
        vec![
            Line::from(""),
            Line::from(Span::styled(err.as_str(), Theme::error())),
        ]
    } else if app.sidebar_items.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No sessions found.",
                Theme::label(),
            )),
            Line::from(Span::styled(
                "  Start a Claude Code session first.",
                Theme::label(),
            )),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Press Enter to resume selected session",
                Theme::label(),
            )),
            Line::from(Span::styled(
                "  The agent will run in this terminal",
                Theme::label(),
            )),
        ]
    };

    let paragraph = Paragraph::new(content).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod e2e_tests;
