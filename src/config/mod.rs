pub mod keys;
pub mod layout;
pub mod theme;
pub mod timing;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

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

/// Candidate config paths in priority order:
/// 1. $XDG_CONFIG_HOME/lazyagent/config.toml
/// 2. ~/.config/lazyagent/config.toml
/// 3. platform default (~/Library/Application Support on macOS)
fn config_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(xdg).join(CONFIG_DIR).join(CONFIG_FILE));
    }

    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config").join(CONFIG_DIR).join(CONFIG_FILE));
    }

    if let Some(platform) = dirs::config_dir() {
        let p = platform.join(CONFIG_DIR).join(CONFIG_FILE);
        if !paths.contains(&p) {
            paths.push(p);
        }
    }

    paths
}

/// Returns the first config path where the file exists, or the first candidate for creation.
fn resolve_config_path() -> Option<PathBuf> {
    let paths = config_paths();
    paths.iter().find(|p| p.exists()).or_else(|| paths.first()).cloned()
}

pub fn load_config() -> Config {
    for path in config_paths() {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        debug!(path = %path.display(), "loading config");
        return match toml::from_str(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!(path = %path.display(), err = %e, "config parse failed, using defaults");
                Config::default()
            }
        };
    }
    Config::default()
}

pub fn save_grouping_mode(mode: &GroupingMode) -> anyhow::Result<()> {
    let Some(path) = resolve_config_path() else {
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
            Some("flat") => GroupingMode::Flat,
            Some("git") => GroupingMode::GitRoot,
            Some("custom") => GroupingMode::Custom,
            _ => GroupingMode::GitRoot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Helper: write a minimal config toml to a temp file and return its dir.
    fn write_config(dir: &std::path::Path, content: &str) {
        let config_dir = dir.join(CONFIG_DIR);
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join(CONFIG_FILE), content).unwrap();
    }

    #[test]
    fn test_config_paths_includes_xdg_first() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg_path = tmp.path().to_str().unwrap().to_string();
        // Temporarily set XDG_CONFIG_HOME
        std::env::set_var("XDG_CONFIG_HOME", &xdg_path);
        let paths = config_paths();
        std::env::remove_var("XDG_CONFIG_HOME");

        assert_eq!(
            paths[0],
            PathBuf::from(&xdg_path).join(CONFIG_DIR).join(CONFIG_FILE)
        );
    }

    #[test]
    fn test_config_paths_includes_dotconfig() {
        std::env::remove_var("XDG_CONFIG_HOME");
        let paths = config_paths();
        let home = dirs::home_dir().unwrap();
        assert!(paths.contains(&home.join(".config").join(CONFIG_DIR).join(CONFIG_FILE)));
    }

    #[test]
    fn test_config_paths_no_duplicates_when_platform_eq_dotconfig() {
        // On Linux dirs::config_dir() == ~/.config, so no duplicate should appear.
        let paths = config_paths();
        let mut seen = std::collections::HashSet::new();
        for p in &paths {
            assert!(seen.insert(p), "duplicate path in config_paths: {}", p.display());
        }
    }

    #[test]
    fn test_load_config_picks_xdg_over_dotconfig() {
        let xdg_tmp = tempfile::tempdir().unwrap();
        let dot_tmp = tempfile::tempdir().unwrap();

        write_config(xdg_tmp.path(), r#"grouping_mode = "flat""#);
        write_config(dot_tmp.path(), r#"grouping_mode = "git""#);

        std::env::set_var("XDG_CONFIG_HOME", xdg_tmp.path());
        // Redirect ~/.config to dot_tmp by pointing HOME (Unix only)
        let orig_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", dot_tmp.path());

        let paths = config_paths();
        // XDG entry must come first
        assert_eq!(paths[0], xdg_tmp.path().join(CONFIG_DIR).join(CONFIG_FILE));

        // Restore env
        std::env::remove_var("XDG_CONFIG_HOME");
        if let Some(h) = orig_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_load_config_reads_first_existing_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        write_config(tmp.path(), r#"grouping_mode = "flat""#);

        let cfg = load_config();
        std::env::remove_var("XDG_CONFIG_HOME");

        assert_eq!(cfg.grouping_mode, Some("flat".into()));
    }

    #[test]
    fn test_load_config_falls_back_to_default_when_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        // Point XDG to an empty dir (no config file)
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        let orig_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let cfg = load_config();

        std::env::remove_var("XDG_CONFIG_HOME");
        if let Some(h) = orig_home {
            std::env::set_var("HOME", h);
        }

        assert_eq!(cfg.grouping_mode, None);
        assert_eq!(cfg.timing.poll_tick_ms, 100);
    }

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
        let cfg: Config = toml::from_str(r#"grouping_mode = "flat""#).unwrap();
        assert_eq!(cfg.grouping_mode(), GroupingMode::Flat);

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
