use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::protocol::{AgentSession, AgentStatus, SessionSource};
use crate::tui::sidebar::shorten_path;
use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, session: Option<&AgentSession>, theme: &Theme) {
    let block = Block::default()
        .title(" Detail ")
        .title_style(theme.title)
        .borders(Borders::ALL)
        .border_style(theme.border_unfocused);

    let session = match session {
        Some(s) => s,
        None => {
            let msg = Paragraph::new("Select a session to view details")
                .style(theme.label)
                .block(block);
            frame.render_widget(msg, area);
            return;
        }
    };

    frame.render_widget(block, area);

    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.height < 2 {
        return;
    }

    let status_str = match session.status {
        AgentStatus::Thinking => "thinking",
        AgentStatus::Waiting => "waiting",
        AgentStatus::NeedsInput => "needs input",
        AgentStatus::Idle => "idle",
        AgentStatus::Error => "error",
        AgentStatus::Unknown => "unknown",
    };

    let source_str = match &session.source {
        SessionSource::Local => "local".to_string(),
        SessionSource::Remote { host } => format!("remote ({})", host),
    };

    let uptime = session
        .started_at
        .and_then(|t| t.elapsed().ok())
        .map(|d| {
            let secs = d.as_secs();
            if secs < 60 {
                format!("{}s", secs)
            } else if secs < 3600 {
                format!("{}m {}s", secs / 60, secs % 60)
            } else {
                format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
            }
        })
        .unwrap_or_else(|| "-".into());

    let pairs = vec![
        ("Provider", session.provider.clone()),
        ("Status", status_str.to_string()),
        ("CWD", shorten_path(&session.cwd.to_string_lossy())),
        ("Session", session.tmux_session.clone()),
        ("Source", source_str),
        ("Uptime", uptime),
    ];

    let mut y = inner.y;
    // Title
    let title_line = Line::from(Span::styled("Session Info", theme.title));
    frame.render_widget(
        Paragraph::new(title_line),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;

    for (key, value) in &pairs {
        if y >= inner.y + inner.height {
            break;
        }
        let line = Line::from(vec![
            Span::styled(format!(" {}: ", key), theme.label),
            Span::styled(value.as_str(), theme.value),
        ]);
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }
}
