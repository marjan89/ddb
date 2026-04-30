use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_adb_path")]
    pub adb_path: String,
    #[serde(default = "default_scrcpy_path")]
    pub scrcpy_path: String,
    #[serde(default)]
    pub default_device: Option<String>,
}

fn default_adb_path() -> String {
    "adb".to_string()
}

fn default_scrcpy_path() -> String {
    "scrcpy".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            adb_path: default_adb_path(),
            scrcpy_path: default_scrcpy_path(),
            default_device: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("invalid config: {e}"))
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| format!("serialize config: {e}"))?;
        fs::write(&path, text).map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    pub fn dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".config/ddb")
    }

    pub fn path() -> PathBuf {
        Self::dir().join("config.toml")
    }
}
