use crossterm::event::KeyCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeysConfig {
    #[serde(default = "default_quit")]
    pub quit: String,
    #[serde(default = "default_down")]
    pub down: String,
    #[serde(default = "default_up")]
    pub up: String,
    #[serde(default = "default_top")]
    pub top: String,
    #[serde(default = "default_bottom")]
    pub bottom: String,
    #[serde(default = "default_detail_show")]
    pub detail_show: String,
    #[serde(default = "default_detail_hide")]
    pub detail_hide: String,
    #[serde(default = "default_kill")]
    pub kill: String,
    #[serde(default = "default_search")]
    pub search: String,
    #[serde(default = "default_refresh")]
    pub refresh: String,
    #[serde(default = "default_cycle_group")]
    pub cycle_group: String,
    #[serde(default = "default_passthrough")]
    pub passthrough: String,
    #[serde(default = "default_attach")]
    pub attach: String,
    #[serde(default = "default_new_session")]
    pub new_session: String,
}

fn default_quit() -> String { "q".into() }
fn default_down() -> String { "j".into() }
fn default_up() -> String { "k".into() }
fn default_top() -> String { "g".into() }
fn default_bottom() -> String { "G".into() }
fn default_detail_show() -> String { "l".into() }
fn default_detail_hide() -> String { "h".into() }
fn default_kill() -> String { "d".into() }
fn default_search() -> String { "/".into() }
fn default_refresh() -> String { "r".into() }
fn default_cycle_group() -> String { "Tab".into() }
fn default_passthrough() -> String { "i".into() }
fn default_attach() -> String { "Enter".into() }
fn default_new_session() -> String { "n".into() }

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            quit: default_quit(),
            down: default_down(),
            up: default_up(),
            top: default_top(),
            bottom: default_bottom(),
            detail_show: default_detail_show(),
            detail_hide: default_detail_hide(),
            kill: default_kill(),
            search: default_search(),
            refresh: default_refresh(),
            cycle_group: default_cycle_group(),
            passthrough: default_passthrough(),
            attach: default_attach(),
            new_session: default_new_session(),
        }
    }
}

/// Precomputed KeyCode bindings from string config.
#[derive(Debug, Clone)]
pub struct KeyBindings {
    pub quit: KeyCode,
    pub down: KeyCode,
    pub up: KeyCode,
    pub top: KeyCode,
    pub bottom: KeyCode,
    pub detail_show: KeyCode,
    pub detail_hide: KeyCode,
    pub kill: KeyCode,
    pub search: KeyCode,
    pub refresh: KeyCode,
    pub cycle_group: KeyCode,
    pub passthrough: KeyCode,
    pub attach: KeyCode,
    pub new_session: KeyCode,
}

impl KeyBindings {
    pub fn from_config(keys: &KeysConfig) -> Self {
        Self {
            quit: parse_key(&keys.quit, KeyCode::Char('q')),
            down: parse_key(&keys.down, KeyCode::Char('j')),
            up: parse_key(&keys.up, KeyCode::Char('k')),
            top: parse_key(&keys.top, KeyCode::Char('g')),
            bottom: parse_key(&keys.bottom, KeyCode::Char('G')),
            detail_show: parse_key(&keys.detail_show, KeyCode::Char('l')),
            detail_hide: parse_key(&keys.detail_hide, KeyCode::Char('h')),
            kill: parse_key(&keys.kill, KeyCode::Char('d')),
            search: parse_key(&keys.search, KeyCode::Char('/')),
            refresh: parse_key(&keys.refresh, KeyCode::Char('r')),
            cycle_group: parse_key(&keys.cycle_group, KeyCode::Tab),
            passthrough: parse_key(&keys.passthrough, KeyCode::Char('i')),
            attach: parse_key(&keys.attach, KeyCode::Enter),
            new_session: parse_key(&keys.new_session, KeyCode::Char('n')),
        }
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::from_config(&KeysConfig::default())
    }
}

/// Parse a key string to KeyCode. Supports:
/// - Single char: "q", "/", "G"
/// - Named keys: "Tab", "Enter", "Esc", "Backspace", "Space"
/// - Arrow keys: "Up", "Down", "Left", "Right"
/// - Modifiers: "Ctrl-d", "Alt-x"
pub fn parse_key(s: &str, fallback: KeyCode) -> KeyCode {
    // Handle modifier prefixes
    if let Some(rest) = s.strip_prefix("Ctrl-") {
        if let Some(c) = rest.chars().next() {
            if rest.len() == c.len_utf8() {
                return KeyCode::Char(c);
            }
        }
        eprintln!("lazyagent: invalid key '{s}', using default");
        return fallback;
    }
    if let Some(rest) = s.strip_prefix("Alt-") {
        if let Some(c) = rest.chars().next() {
            if rest.len() == c.len_utf8() {
                return KeyCode::Char(c);
            }
        }
        eprintln!("lazyagent: invalid key '{s}', using default");
        return fallback;
    }

    match s {
        "Tab" => KeyCode::Tab,
        "Enter" => KeyCode::Enter,
        "Esc" | "Escape" => KeyCode::Esc,
        "Backspace" => KeyCode::Backspace,
        "Space" => KeyCode::Char(' '),
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        s if s.len() == 1 => KeyCode::Char(s.chars().next().unwrap()),
        _ => {
            eprintln!("lazyagent: unknown key '{s}', using default");
            fallback
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_char() {
        assert_eq!(parse_key("q", KeyCode::Char('x')), KeyCode::Char('q'));
        assert_eq!(parse_key("/", KeyCode::Char('x')), KeyCode::Char('/'));
        assert_eq!(parse_key("G", KeyCode::Char('x')), KeyCode::Char('G'));
    }

    #[test]
    fn test_parse_key_named() {
        assert_eq!(parse_key("Tab", KeyCode::Char('x')), KeyCode::Tab);
        assert_eq!(parse_key("Enter", KeyCode::Char('x')), KeyCode::Enter);
        assert_eq!(parse_key("Esc", KeyCode::Char('x')), KeyCode::Esc);
        assert_eq!(parse_key("Space", KeyCode::Char('x')), KeyCode::Char(' '));
    }

    #[test]
    fn test_parse_key_ctrl() {
        assert_eq!(
            parse_key("Ctrl-d", KeyCode::Char('x')),
            KeyCode::Char('d')
        );
    }

    #[test]
    fn test_parse_key_invalid_fallback() {
        assert_eq!(
            parse_key("InvalidKey", KeyCode::Char('x')),
            KeyCode::Char('x')
        );
    }

    #[test]
    fn test_default_bindings() {
        let b = KeyBindings::default();
        assert_eq!(b.quit, KeyCode::Char('q'));
        assert_eq!(b.down, KeyCode::Char('j'));
        assert_eq!(b.cycle_group, KeyCode::Tab);
        assert_eq!(b.attach, KeyCode::Enter);
    }

    #[test]
    fn test_custom_bindings() {
        let mut cfg = KeysConfig::default();
        cfg.quit = "x".into();
        cfg.down = "Tab".into();
        let b = KeyBindings::from_config(&cfg);
        assert_eq!(b.quit, KeyCode::Char('x'));
        assert_eq!(b.down, KeyCode::Tab);
    }

    #[test]
    fn test_parse_key_alt() {
        assert_eq!(parse_key("Alt-x", KeyCode::Char('z')), KeyCode::Char('x'));
        assert_eq!(parse_key("Alt-q", KeyCode::Char('z')), KeyCode::Char('q'));
    }

    #[test]
    fn test_parse_key_arrows() {
        assert_eq!(parse_key("Up", KeyCode::Char('x')), KeyCode::Up);
        assert_eq!(parse_key("Down", KeyCode::Char('x')), KeyCode::Down);
        assert_eq!(parse_key("Left", KeyCode::Char('x')), KeyCode::Left);
        assert_eq!(parse_key("Right", KeyCode::Char('x')), KeyCode::Right);
    }

    #[test]
    fn test_parse_key_special() {
        assert_eq!(parse_key("Home", KeyCode::Char('x')), KeyCode::Home);
        assert_eq!(parse_key("End", KeyCode::Char('x')), KeyCode::End);
        assert_eq!(parse_key("PageUp", KeyCode::Char('x')), KeyCode::PageUp);
        assert_eq!(parse_key("PageDown", KeyCode::Char('x')), KeyCode::PageDown);
        assert_eq!(parse_key("Delete", KeyCode::Char('x')), KeyCode::Delete);
        assert_eq!(parse_key("Insert", KeyCode::Char('x')), KeyCode::Insert);
        assert_eq!(parse_key("Backspace", KeyCode::Char('x')), KeyCode::Backspace);
    }

    #[test]
    fn test_parse_key_escape_variants() {
        assert_eq!(parse_key("Esc", KeyCode::Char('x')), KeyCode::Esc);
        assert_eq!(parse_key("Escape", KeyCode::Char('x')), KeyCode::Esc);
    }

    #[test]
    fn test_parse_key_invalid_ctrl_fallback() {
        // Ctrl-xyz is multi-char, should fallback
        assert_eq!(parse_key("Ctrl-xyz", KeyCode::Char('z')), KeyCode::Char('z'));
    }

    #[test]
    fn test_parse_key_invalid_alt_fallback() {
        assert_eq!(parse_key("Alt-abc", KeyCode::Char('z')), KeyCode::Char('z'));
    }

    #[test]
    fn test_parse_key_case_sensitive() {
        // "tab" != "Tab" — lowercase multi-char should fallback
        assert_eq!(parse_key("tab", KeyCode::Char('x')), KeyCode::Char('x'));
        assert_eq!(parse_key("enter", KeyCode::Char('x')), KeyCode::Char('x'));
    }

    #[test]
    fn test_all_14_bindings_from_config() {
        let cfg = KeysConfig::default();
        let b = KeyBindings::from_config(&cfg);
        assert_eq!(b.quit, KeyCode::Char('q'));
        assert_eq!(b.down, KeyCode::Char('j'));
        assert_eq!(b.up, KeyCode::Char('k'));
        assert_eq!(b.top, KeyCode::Char('g'));
        assert_eq!(b.bottom, KeyCode::Char('G'));
        assert_eq!(b.detail_show, KeyCode::Char('l'));
        assert_eq!(b.detail_hide, KeyCode::Char('h'));
        assert_eq!(b.kill, KeyCode::Char('d'));
        assert_eq!(b.search, KeyCode::Char('/'));
        assert_eq!(b.refresh, KeyCode::Char('r'));
        assert_eq!(b.cycle_group, KeyCode::Tab);
        assert_eq!(b.passthrough, KeyCode::Char('i'));
        assert_eq!(b.attach, KeyCode::Enter);
        assert_eq!(b.new_session, KeyCode::Char('n'));
    }

    #[test]
    fn test_keys_from_toml() {
        let toml_str = r#"
quit = "x"
down = "Down"
up = "Up"
kill = "Delete"
"#;
        let cfg: KeysConfig = toml::from_str(toml_str).unwrap();
        let b = KeyBindings::from_config(&cfg);
        assert_eq!(b.quit, KeyCode::Char('x'));
        assert_eq!(b.down, KeyCode::Down);
        assert_eq!(b.up, KeyCode::Up);
        assert_eq!(b.kill, KeyCode::Delete);
        // Unset keys use defaults
        assert_eq!(b.search, KeyCode::Char('/'));
        assert_eq!(b.attach, KeyCode::Enter);
    }
}
