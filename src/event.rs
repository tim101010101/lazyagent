use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};

pub enum AppEvent {
    Key(KeyEvent),
    Tick,
}

pub fn poll_event(tick_rate: Duration) -> std::io::Result<Option<AppEvent>> {
    if event::poll(tick_rate)? {
        if let Event::Key(key) = event::read()? {
            return Ok(Some(AppEvent::Key(key)));
        }
    }
    Ok(Some(AppEvent::Tick))
}
