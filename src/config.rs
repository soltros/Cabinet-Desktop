use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_url: String,
    pub username: String,
}

fn path() -> Result<PathBuf, String> {
    let dirs = ProjectDirs::from("info", "soltros", "CabinetDesktop")
        .ok_or_else(|| "Unable to determine the configuration directory".to_string())?;
    fs::create_dir_all(dirs.config_dir()).map_err(|e| e.to_string())?;
    Ok(dirs.config_dir().join("config.json"))
}

pub fn load() -> AppConfig {
    let Ok(path) = path() else {
        return AppConfig::default();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return AppConfig::default();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path()?, bytes).map_err(|e| e.to_string())
}
