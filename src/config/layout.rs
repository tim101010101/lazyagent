use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LayoutConfig {
    #[serde(default = "default_sidebar_percent")]
    pub sidebar_percent: u16,
    #[serde(default = "default_main_percent")]
    pub main_percent: u16,
    #[serde(default = "default_sidebar_2col_percent")]
    pub sidebar_2col_percent: u16,
}

fn default_sidebar_percent() -> u16 {
    25
}
fn default_main_percent() -> u16 {
    50
}
fn default_sidebar_2col_percent() -> u16 {
    30
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            sidebar_percent: default_sidebar_percent(),
            main_percent: default_main_percent(),
            sidebar_2col_percent: default_sidebar_2col_percent(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SidebarConfig {
    #[serde(default = "default_local_marker")]
    pub local_marker: String,
    #[serde(default = "default_remote_marker")]
    pub remote_marker: String,
}

fn default_local_marker() -> String {
    "●".into()
}
fn default_remote_marker() -> String {
    "◆".into()
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            local_marker: default_local_marker(),
            remote_marker: default_remote_marker(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_defaults() {
        let l = LayoutConfig::default();
        assert_eq!(l.sidebar_percent, 25);
        assert_eq!(l.main_percent, 50);
        assert_eq!(l.sidebar_2col_percent, 30);
    }

    #[test]
    fn test_sidebar_defaults() {
        let s = SidebarConfig::default();
        assert_eq!(s.local_marker, "●");
        assert_eq!(s.remote_marker, "◆");
    }

    #[test]
    fn test_partial_toml() {
        let toml_str = r#"sidebar_percent = 35"#;
        let l: LayoutConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(l.sidebar_percent, 35);
        assert_eq!(l.main_percent, 50); // default
    }
}
