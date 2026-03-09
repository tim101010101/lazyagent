use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::GroupingMode;

const CONFIG_DIR: &str = "lazyagent";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    pub grouping_mode: Option<String>,
    #[serde(default)]
    pub group: Vec<CustomGroup>,
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
    toml::from_str(&content).unwrap_or_default()
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
