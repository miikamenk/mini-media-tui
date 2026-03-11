use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct ConfigSource {
    pub block_id: String,
    pub player_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub sources: Vec<ConfigSource>,
    pub refresh_interval_secs: u64,
}

pub fn get_config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|p| p.join("mini-media").join("config.json"))
}

pub fn load_config() -> Config {
    if let Some(path) = get_config_path() {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str(&content) {
                    return config;
                }
            }
        }
    }
    Config {
        sources: vec![
            ConfigSource {
                block_id: "media_1".to_string(),
                player_id: "test".to_string(),
            },
            ConfigSource {
                block_id: "media_2".to_string(),
                player_id: "firefox".to_string(),
            },
        ],
        refresh_interval_secs: 1,
    }
}

pub fn save_config(config: &Config) -> Result<()> {
    if let Some(path) = get_config_path() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(config)?;
        std::fs::write(path, content)?;
    }
    Ok(())
}
