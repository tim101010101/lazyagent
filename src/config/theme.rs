use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ThemeConfig {
    #[serde(default)]
    pub preset: Option<String>,
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

pub fn preset_styles(name: &str) -> HashMap<String, StyleDef> {
    match name {
        "nord" => nord(),
        "catppuccin-mocha" => catppuccin_mocha(),
        "gruvbox-dark" => gruvbox_dark(),
        "tokyonight" => tokyonight(),
        "ayu-dark" => ayu_dark(),
        _ => HashMap::new(),
    }
}

fn s(fg: &str) -> StyleDef {
    StyleDef { fg: Some(fg.into()), bg: None, bold: false }
}

fn sb(fg: &str) -> StyleDef {
    StyleDef { fg: Some(fg.into()), bg: None, bold: true }
}

fn sbg(fg: &str, bg: &str) -> StyleDef {
    StyleDef { fg: Some(fg.into()), bg: Some(bg.into()), bold: true }
}

fn nord() -> HashMap<String, StyleDef> {
    let mut m = HashMap::new();
    m.insert("title".into(),             sb("#88C0D0"));
    m.insert("border_focused".into(),    s("#88C0D0"));
    m.insert("border_unfocused".into(),  s("#434C5E"));
    m.insert("source_header".into(),     sb("#81A1C1"));
    m.insert("project_header".into(),    sb("#EBCB8B"));
    m.insert("status_thinking".into(),   s("#EBCB8B"));
    m.insert("status_active".into(),     s("#A3BE8C"));
    m.insert("status_needs_input".into(),s("#B48EAD"));
    m.insert("status_error".into(),      s("#BF616A"));
    m.insert("status_unknown".into(),    s("#4C566A"));
    m.insert("error".into(),             s("#BF616A"));
    m.insert("selected".into(),          sbg("#D8DEE9", "#3B4252"));
    m.insert("selected_bar".into(),      s("#88C0D0"));
    m.insert("normal".into(),            s("#D8DEE9"));
    m.insert("key_action".into(),        s("#D8DEE9"));
    m.insert("label".into(),             s("#4C566A"));
    m.insert("key_hint".into(),          s("#4C566A"));
    m.insert("value".into(),             s("#D8DEE9"));
    m.insert("passthrough_border".into(),s("#B48EAD"));
    m.insert("passthrough_indicator".into(), sb("#B48EAD"));
    m
}

fn catppuccin_mocha() -> HashMap<String, StyleDef> {
    let mut m = HashMap::new();
    m.insert("title".into(),             sb("#89DCEB"));
    m.insert("border_focused".into(),    s("#89DCEB"));
    m.insert("border_unfocused".into(),  s("#45475A"));
    m.insert("source_header".into(),     sb("#89B4FA"));
    m.insert("project_header".into(),    sb("#F9E2AF"));
    m.insert("status_thinking".into(),   s("#F9E2AF"));
    m.insert("status_active".into(),     s("#A6E3A1"));
    m.insert("status_needs_input".into(),s("#CBA6F7"));
    m.insert("status_error".into(),      s("#F38BA8"));
    m.insert("status_unknown".into(),    s("#6C7086"));
    m.insert("error".into(),             s("#F38BA8"));
    m.insert("selected".into(),          sbg("#CDD6F4", "#313244"));
    m.insert("selected_bar".into(),      s("#89DCEB"));
    m.insert("normal".into(),            s("#CDD6F4"));
    m.insert("key_action".into(),        s("#CDD6F4"));
    m.insert("label".into(),             s("#6C7086"));
    m.insert("key_hint".into(),          s("#6C7086"));
    m.insert("value".into(),             s("#CDD6F4"));
    m.insert("passthrough_border".into(),s("#CBA6F7"));
    m.insert("passthrough_indicator".into(), sb("#CBA6F7"));
    m
}

fn gruvbox_dark() -> HashMap<String, StyleDef> {
    let mut m = HashMap::new();
    m.insert("title".into(),             sb("#83A598"));
    m.insert("border_focused".into(),    s("#83A598"));
    m.insert("border_unfocused".into(),  s("#504945"));
    m.insert("source_header".into(),     sb("#458588"));
    m.insert("project_header".into(),    sb("#D79921"));
    m.insert("status_thinking".into(),   s("#D79921"));
    m.insert("status_active".into(),     s("#98971A"));
    m.insert("status_needs_input".into(),s("#B16286"));
    m.insert("status_error".into(),      s("#CC241D"));
    m.insert("status_unknown".into(),    s("#928374"));
    m.insert("error".into(),             s("#CC241D"));
    m.insert("selected".into(),          sbg("#EBDBB2", "#3C3836"));
    m.insert("selected_bar".into(),      s("#83A598"));
    m.insert("normal".into(),            s("#EBDBB2"));
    m.insert("key_action".into(),        s("#EBDBB2"));
    m.insert("label".into(),             s("#928374"));
    m.insert("key_hint".into(),          s("#928374"));
    m.insert("value".into(),             s("#EBDBB2"));
    m.insert("passthrough_border".into(),s("#B16286"));
    m.insert("passthrough_indicator".into(), sb("#B16286"));
    m
}

fn tokyonight() -> HashMap<String, StyleDef> {
    let mut m = HashMap::new();
    m.insert("title".into(),             sb("#7DCFFF"));
    m.insert("border_focused".into(),    s("#7DCFFF"));
    m.insert("border_unfocused".into(),  s("#3B4261"));
    m.insert("source_header".into(),     sb("#7AA2F7"));
    m.insert("project_header".into(),    sb("#E0AF68"));
    m.insert("status_thinking".into(),   s("#E0AF68"));
    m.insert("status_active".into(),     s("#9ECE6A"));
    m.insert("status_needs_input".into(),s("#BB9AF7"));
    m.insert("status_error".into(),      s("#F7768E"));
    m.insert("status_unknown".into(),    s("#565F89"));
    m.insert("error".into(),             s("#F7768E"));
    m.insert("selected".into(),          sbg("#C0CAF5", "#283457"));
    m.insert("selected_bar".into(),      s("#7DCFFF"));
    m.insert("normal".into(),            s("#C0CAF5"));
    m.insert("key_action".into(),        s("#C0CAF5"));
    m.insert("label".into(),             s("#565F89"));
    m.insert("key_hint".into(),          s("#565F89"));
    m.insert("value".into(),             s("#C0CAF5"));
    m.insert("passthrough_border".into(),s("#BB9AF7"));
    m.insert("passthrough_indicator".into(), sb("#BB9AF7"));
    m
}

fn ayu_dark() -> HashMap<String, StyleDef> {
    let mut m = HashMap::new();
    m.insert("title".into(),             sb("#39BAE6"));
    m.insert("border_focused".into(),    s("#39BAE6"));
    m.insert("border_unfocused".into(),  s("#2D3640"));
    m.insert("source_header".into(),     sb("#59C2FF"));
    m.insert("project_header".into(),    sb("#FFB454"));
    m.insert("status_thinking".into(),   s("#FFB454"));
    m.insert("status_active".into(),     s("#7FD962"));
    m.insert("status_needs_input".into(),s("#D2A6FF"));
    m.insert("status_error".into(),      s("#FF3333"));
    m.insert("status_unknown".into(),    s("#5C6773"));
    m.insert("error".into(),             s("#FF3333"));
    m.insert("selected".into(),          sbg("#BFBDB6", "#273747"));
    m.insert("selected_bar".into(),      s("#39BAE6"));
    m.insert("normal".into(),            s("#BFBDB6"));
    m.insert("key_action".into(),        s("#BFBDB6"));
    m.insert("label".into(),             s("#5C6773"));
    m.insert("key_hint".into(),          s("#5C6773"));
    m.insert("value".into(),             s("#BFBDB6"));
    m.insert("passthrough_border".into(),s("#D2A6FF"));
    m.insert("passthrough_indicator".into(), sb("#D2A6FF"));
    m
}

impl ThemeConfig {
    pub fn get_style(&self, key: &str, default: Style) -> Style {
        let mut merged = self.preset.as_deref()
            .map(preset_styles)
            .unwrap_or_default();
        merged.extend(self.styles.clone());
        merged.get(key).map(|d| d.to_style()).unwrap_or(default)
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
        // Should match hardcoded Nord defaults
        assert_eq!(theme.title.fg, Some(Color::Rgb(136, 192, 208)));
        assert_eq!(theme.selected.bg, Some(Color::Rgb(59, 66, 82)));
        assert_eq!(theme.error.fg, Some(Color::Rgb(191, 97, 106)));
        assert_eq!(theme.status_thinking.fg, Some(Color::Rgb(235, 203, 139)));
    }

    #[test]
    fn test_theme_from_config_partial_override() {
        let toml_str = r##"
[title]
fg = "green"
bold = false

[error]
fg = "#ff8800"
"##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        assert_eq!(theme.title.fg, Some(Color::Green));
        assert_eq!(theme.error.fg, Some(Color::Rgb(255, 136, 0)));
        // Unoverridden: still defaults
        assert_eq!(theme.selected.bg, Some(Color::Rgb(59, 66, 82)));
        assert_eq!(theme.normal.fg, Some(Color::Gray));
    }

    #[test]
    fn test_preset_nord_loads() {
        let toml_str = r#"
preset = "nord"
"#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        assert_eq!(theme.title.fg, Some(Color::Rgb(136, 192, 208))); // #88C0D0
        assert_eq!(theme.border_focused.fg, Some(Color::Rgb(136, 192, 208)));
        assert_eq!(theme.status_active.fg, Some(Color::Rgb(163, 190, 140))); // #A3BE8C
    }

    #[test]
    fn test_preset_override() {
        let toml_str = r##"
preset = "nord"

[title]
fg = "#ff0000"
"##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        // Override wins
        assert_eq!(theme.title.fg, Some(Color::Rgb(255, 0, 0)));
        // Preset base still applies to other slots
        assert_eq!(theme.border_focused.fg, Some(Color::Rgb(136, 192, 208)));
    }

    #[test]
    fn test_unknown_preset_falls_back() {
        let toml_str = r#"
preset = "nonexistent"
"#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        // Should fall back to hardcoded Nord defaults
        assert_eq!(theme.title.fg, Some(Color::Rgb(136, 192, 208)));
        assert_eq!(theme.selected.bg, Some(Color::Rgb(59, 66, 82)));
    }

    #[test]
    fn test_backward_compat() {
        let toml_str = r#"
[title]
fg = "red"
"#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        let theme = crate::tui::theme::Theme::from_config(&cfg);
        // No preset field → same as today
        assert_eq!(theme.title.fg, Some(Color::Red));
        assert_eq!(theme.selected.bg, Some(Color::Rgb(59, 66, 82)));
    }
}
