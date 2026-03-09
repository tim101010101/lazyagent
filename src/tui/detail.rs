use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::protocol::{DetailBlock, SessionDetail};
use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, detail: Option<&SessionDetail>) {
    let block = Block::default()
        .title(" Detail ")
        .title_style(Theme::title())
        .borders(Borders::ALL)
        .border_style(Theme::border_unfocused());

    let detail = match detail {
        Some(d) => d,
        None => {
            let msg = Paragraph::new("Select a session to view details")
                .style(Theme::label())
                .block(block);
            frame.render_widget(msg, area);
            return;
        }
    };

    frame.render_widget(block, area);

    // Inner area for content
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.height < 2 {
        return;
    }

    // Render detail blocks stacked vertically
    let mut y = inner.y;
    for detail_block in &detail.detail_blocks {
        if y >= inner.y + inner.height {
            break;
        }
        let remaining = Rect::new(inner.x, y, inner.width, inner.y + inner.height - y);
        let used = render_block(frame, remaining, detail_block);
        y += used + 1; // +1 for spacing
    }
}

fn render_block(frame: &mut Frame, area: Rect, block: &DetailBlock) -> u16 {
    match block {
        DetailBlock::KeyValue { title, pairs } => render_kv(frame, area, title, pairs),
        DetailBlock::Metrics { title, items } => render_metrics(frame, area, title, items),
        DetailBlock::Text { title, content } => render_text(frame, area, title, content),
        DetailBlock::Unknown => 0,
    }
}

fn render_kv(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    pairs: &[crate::protocol::KvPair],
) -> u16 {
    if area.height < 2 {
        return 0;
    }

    // Title line
    let title_line = Line::from(Span::styled(title, Theme::title()));
    frame.render_widget(Paragraph::new(title_line), Rect::new(area.x, area.y, area.width, 1));

    let mut y = area.y + 1;
    for pair in pairs {
        if y >= area.y + area.height {
            break;
        }
        let line = Line::from(vec![
            Span::styled(format!(" {}: ", pair.key), Theme::label()),
            Span::styled(&pair.value, Theme::value()),
        ]);
        frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
        y += 1;
    }

    y - area.y
}

fn render_metrics(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[crate::protocol::MetricItem],
) -> u16 {
    if area.height < 2 {
        return 0;
    }

    let title_line = Line::from(Span::styled(title, Theme::title()));
    frame.render_widget(Paragraph::new(title_line), Rect::new(area.x, area.y, area.width, 1));

    let mut y = area.y + 1;
    for item in items {
        if y >= area.y + area.height {
            break;
        }

        let formatted_value = format_number(item.value);
        let line = Line::from(vec![
            Span::styled(format!(" {}: ", item.label), Theme::label()),
            Span::styled(formatted_value, Theme::metric_value()),
        ]);
        frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
        y += 1;

        // Render progress bar if max_value is set
        if let Some(max) = item.max_value {
            if y < area.y + area.height && max > 0 {
                let ratio = (item.value as f64 / max as f64).min(1.0);
                let bar_width = area.width.saturating_sub(2) as usize;
                let filled = (bar_width as f64 * ratio) as usize;
                let empty = bar_width - filled;
                let pct = (ratio * 100.0) as u16;

                let bar = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        "\u{2501}".repeat(filled),
                        Theme::progress_filled(),
                    ),
                    Span::styled(
                        "\u{2591}".repeat(empty),
                        Theme::progress_empty(),
                    ),
                    Span::styled(format!(" {}%", pct), Theme::label()),
                ]);
                frame.render_widget(Paragraph::new(bar), Rect::new(area.x, y, area.width, 1));
                y += 1;
            }
        }
    }

    y - area.y
}

fn render_text(frame: &mut Frame, area: Rect, title: &str, content: &str) -> u16 {
    if area.height < 2 {
        return 0;
    }

    let title_line = Line::from(Span::styled(title, Theme::title()));
    frame.render_widget(Paragraph::new(title_line), Rect::new(area.x, area.y, area.width, 1));

    let lines: Vec<Line> = content
        .lines()
        .map(|l| Line::from(Span::styled(format!(" {}", l), Theme::normal())))
        .collect();
    let height = lines.len().min((area.height - 1) as usize) as u16;
    let para = Paragraph::new(lines);
    frame.render_widget(para, Rect::new(area.x, area.y + 1, area.width, height));

    1 + height
}

fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
