use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ThemeConfig {
    #[serde(flatten)]
    pub styles: HashMap<String, StyleDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StyleDef {
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub bold: bool,
}

impl StyleDef {
    pub fn to_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(ref fg) = self.fg {
            if let Some(color) = parse_color(fg) {
                style = style.fg(color);
            }
        }
        if let Some(ref bg) = self.bg {
            if let Some(color) = parse_color(bg) {
                style = style.bg(color);
            }
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }
}

/// Parse a color string: named colors or "#rrggbb" hex.
pub fn parse_color(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
        eprintln!("lazyagent: invalid hex color '{s}', using default");
        return None;
    }

    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Some(Color::DarkGray),
        "lightred" | "light_red" => Some(Color::LightRed),
        "lightgreen" | "light_green" => Some(Color::LightGreen),
        "lightyellow" | "light_yellow" => Some(Color::LightYellow),
        "lightblue" | "light_blue" => Some(Color::LightBlue),
        "lightmagenta" | "light_magenta" => Some(Color::LightMagenta),
        "lightcyan" | "light_cyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        "reset" => Some(Color::Reset),
        _ => {
            eprintln!("lazyagent: unknown color '{s}', using default");
            None
        }
    }
}

impl ThemeConfig {
    pub fn get_style(&self, key: &str, default: Style) -> Style {
        match self.styles.get(key) {
            Some(def) => def.to_style(),
            None => default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("cyan"), Some(Color::Cyan));
        assert_eq!(parse_color("DarkGray"), Some(Color::DarkGray));
        assert_eq!(parse_color("white"), Some(Color::White));
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("#00ff00"), Some(Color::Rgb(0, 255, 0)));
    }

    #[test]
    fn test_parse_color_invalid() {
        assert_eq!(parse_color("foobar"), None);
        assert_eq!(parse_color("#abc"), None);
    }

    #[test]
    fn test_style_def_to_style() {
        let def = StyleDef {
            fg: Some("cyan".into()),
            bg: None,
            bold: true,
        };
        let style = def.to_style();
        assert_eq!(style.fg, Some(Color::Cyan));
        assert!(style.add_modifier == Modifier::BOLD);
    }

    #[test]
    fn test_theme_config_from_toml() {
        let toml_str = r#"
[title]
fg = "cyan"
bold = true

[selected]
fg = "white"
bg = "darkgray"
bold = true
"#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.styles.contains_key("title"));
        assert!(cfg.styles.contains_key("selected"));
    }

    #[test]
    fn test_parse_color_underscore_variants() {
        assert_eq!(parse_color("dark_gray"), Some(Color::DarkGray));
        assert_eq!(parse_color("light_red"), Some(Color::LightRed));
        assert_eq!(parse_color("light_green"), Some(Color::LightGreen));
        assert_eq!(parse_color("light_blue"), Some(Color::LightBlue));
        assert_eq!(parse_color("light_cyan"), Some(Color::LightCyan));
        assert_eq!(parse_color("light_magenta"), Some(Color::LightMagenta));
        assert_eq!(parse_color("light_yellow"), Some(Color::LightYellow));
    }

    #[test]
    fn test_parse_color_case_insensitive() {
        assert_eq!(parse_color("CYAN"), Some(Color::Cyan));
        assert_eq!(parse_color("Red"), Some(Color::Red));
        assert_eq!(parse_color("DARKGRAY"), Some(Color::DarkGray));
    }

    #[test]
    fn test_parse_color_hex_uppercase() {
        assert_eq!(parse_color("#FF00FF"), Some(Color::Rgb(255, 0, 255)));
    }

    #[test]
    fn test_parse_color_hex_invalid_lengths() {
        assert_eq!(parse_color("#ff"), None);
        assert_eq!(parse_color("#ffff"), None);
        assert_eq!(parse_color("#"), None);
    }

    #[test]
    fn test_style_def_bg_only() {
        let def = StyleDef {
            fg: None,
            bg: Some("red".into()),
            bold: false,
        };
        let style = def.to_style();
        assert_eq!(style.fg, None);
        assert_eq!(style.bg, Some(Color::Red));
    }

    #[test]
    fn test_style_def_invalid_color_fallback() {
        let def = StyleDef {
            fg: Some("nonexistent".into()),
            bg: None,
            bold: false,
        };
        let style = def.to_style();
        assert_eq!(style.fg, None); // invalid → no color set
    }

    #[test]
    fn test_get_style_override() {
        let toml_str = r#"
[title]
fg = "red"
bold = false
"#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let default = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let result = cfg.get_style("title", default);
        assert_eq!(result.fg, Some(Color::Red));
        // bold=false → no modifier
        assert_eq!(result.add_modifier, Modifier::empty());
    }

    #[test]
    fn test_get_style_missing_key_uses_default() {
        let cfg = ThemeConfig::default();
        let default = Style::default().fg(Color::Cyan);
        let result = cfg.get_style("nonexistent", default);
        assert_eq!(result.fg, Some(Color::Cyan));
    }

    #[test]
    fn test_theme_from_config_empty() {
        let cfg = ThemeConfig::default();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        // Should match hardcoded defaults
        assert_eq!(theme.title.fg, Some(Color::Cyan));
        assert_eq!(theme.selected.bg, Some(Color::DarkGray));
        assert_eq!(theme.error.fg, Some(Color::Red));
        assert_eq!(theme.status_thinking.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_theme_from_config_partial_override() {
        let toml_str = r#"
[title]
fg = "green"
bold = false

[error]
fg = "#ff8800"
"#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        assert_eq!(theme.title.fg, Some(Color::Green));
        assert_eq!(theme.error.fg, Some(Color::Rgb(255, 136, 0)));
        // Unoverridden: still defaults
        assert_eq!(theme.selected.bg, Some(Color::DarkGray));
        assert_eq!(theme.normal.fg, Some(Color::Gray));
    }
}
