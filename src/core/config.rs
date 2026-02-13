use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub download_directory: String,
    pub editor: String,
    pub auto_refresh: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            download_directory: dirs::download_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .into_owned(),
            editor: std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string()),
            auto_refresh: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::get_path();
        if let Ok(content) = fs::read_to_string(config_path) {
            toml::from_str(&content).unwrap_or_default()
        } else {
            let default = Self::default();
            let _ = default.save();
            default
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::get_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    fn get_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("d-link")
            .join("config.toml")
    }
}
