use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::theme::Theme;

#[allow(dead_code)]
pub fn render(frame: &mut Frame, area: Rect, theme: &Theme) {
    let version = env!("CARGO_PKG_VERSION");
    let text = format!("LazyAgent v{} ", version);
    let p = Paragraph::new(Line::from(Span::styled(text, theme.label)))
        .alignment(Alignment::Right);
    frame.render_widget(p, area);
}
