pub mod keys;
pub mod layout;
pub mod theme;
pub mod timing;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::GroupingMode;

pub use keys::{KeyBindings, KeysConfig};
pub use layout::{LayoutConfig, SidebarConfig};
pub use theme::ThemeConfig;
pub use timing::TimingConfig;

const CONFIG_DIR: &str = "lazyagent";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    pub grouping_mode: Option<String>,
    #[serde(default)]
    pub group: Vec<CustomGroup>,
    #[serde(default)]
    pub timing: TimingConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub sidebar: SidebarConfig,
    #[serde(default)]
    pub keys: KeysConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomGroup {
    pub name: String,
    pub patterns: Vec<String>,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(CONFIG_DIR).join(CONFIG_FILE))
}

pub fn load_config() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    match toml::from_str(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("lazyagent: config parse error: {e}, using defaults");
            Config::default()
        }
    }
}

pub fn save_grouping_mode(mode: &GroupingMode) -> anyhow::Result<()> {
    let Some(path) = config_path() else {
        anyhow::bail!("could not determine config directory");
    };

    // Load existing config to preserve custom groups
    let mut config = if let Ok(content) = std::fs::read_to_string(&path) {
        toml::from_str::<Config>(&content).unwrap_or_default()
    } else {
        Config::default()
    };

    config.grouping_mode = Some(mode.as_str().to_string());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, toml::to_string_pretty(&config)?)?;
    Ok(())
}

impl Config {
    pub fn grouping_mode(&self) -> GroupingMode {
        match self.grouping_mode.as_deref() {
            Some("git") => GroupingMode::GitRoot,
            Some("custom") => GroupingMode::Custom,
            _ => GroupingMode::GitRoot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_config_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.grouping_mode, None);
        assert!(cfg.group.is_empty());
        assert_eq!(cfg.timing.poll_tick_ms, 100);
        assert_eq!(cfg.layout.sidebar_percent, 25);
        assert_eq!(cfg.sidebar.local_marker, "●");
        assert_eq!(cfg.keys.quit, "q");
        assert!(cfg.theme.styles.is_empty());
    }

    #[test]
    fn test_grouping_mode_variants() {
        let cfg: Config = toml::from_str(r#"grouping_mode = "git""#).unwrap();
        assert_eq!(cfg.grouping_mode(), GroupingMode::GitRoot);

        let cfg: Config = toml::from_str(r#"grouping_mode = "custom""#).unwrap();
        assert_eq!(cfg.grouping_mode(), GroupingMode::Custom);

        let cfg: Config = toml::from_str(r#"grouping_mode = "unknown""#).unwrap();
        assert_eq!(cfg.grouping_mode(), GroupingMode::GitRoot); // default fallback
    }

    #[test]
    fn test_backward_compat_old_format() {
        // Old config with only grouping_mode + groups
        let toml_str = r#"
grouping_mode = "custom"

[[group]]
name = "work"
patterns = ["*/work/*"]
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.grouping_mode(), GroupingMode::Custom);
        assert_eq!(cfg.group.len(), 1);
        assert_eq!(cfg.group[0].name, "work");
        // New sections default
        assert_eq!(cfg.timing.refresh_interval_ms, 2000);
        assert_eq!(cfg.layout.sidebar_percent, 25);
    }

    #[test]
    fn test_full_config_all_sections() {
        let toml_str = r#"
grouping_mode = "git"

[[group]]
name = "personal"
patterns = ["*/personal/*"]

[timing]
refresh_interval_ms = 3000
poll_tick_ms = 150

[layout]
sidebar_percent = 30
main_percent = 45
sidebar_2col_percent = 35

[sidebar]
local_marker = "L"
remote_marker = "R"

[keys]
quit = "x"
down = "Down"

[theme.title]
fg = "green"
bold = true
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.grouping_mode(), GroupingMode::GitRoot);
        assert_eq!(cfg.group[0].name, "personal");
        assert_eq!(cfg.timing.refresh_interval_ms, 3000);
        assert_eq!(cfg.timing.poll_tick_ms, 150);
        assert_eq!(cfg.layout.sidebar_percent, 30);
        assert_eq!(cfg.layout.main_percent, 45);
        assert_eq!(cfg.sidebar.local_marker, "L");
        assert_eq!(cfg.sidebar.remote_marker, "R");
        assert_eq!(cfg.keys.quit, "x");
        assert_eq!(cfg.keys.down, "Down");
        assert!(cfg.theme.styles.contains_key("title"));
    }

    #[test]
    fn test_partial_sections() {
        let toml_str = r#"
[timing]
refresh_interval_ms = 5000

[keys]
quit = "Q"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.timing.refresh_interval_ms, 5000);
        assert_eq!(cfg.timing.poll_tick_ms, 100); // default
        assert_eq!(cfg.keys.quit, "Q");
        assert_eq!(cfg.keys.down, "j"); // default
        assert_eq!(cfg.layout.sidebar_percent, 25); // default
    }

    #[test]
    fn test_invalid_toml_returns_default() {
        let result: Result<Config, _> = toml::from_str("{{invalid");
        assert!(result.is_err());
        // load_config would use Config::default() on parse error
        let default = Config::default();
        assert_eq!(default.timing.poll_tick_ms, 100);
    }
}
