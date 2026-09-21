use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_url: Option<String>,
    pub username: Option<String>,
}

fn config_path() -> Result<PathBuf, String> {
    let dirs = ProjectDirs::from("info", "soltros", "Cabinet")
        .ok_or_else(|| "Could not resolve application configuration directory".to_string())?;
    fs::create_dir_all(dirs.config_dir()).map_err(|e| e.to_string())?;
    Ok(dirs.config_dir().join("desktop.json"))
}

pub fn load() -> AppConfig {
    let Ok(path) = config_path() else { return AppConfig::default(); };
    let Ok(data) = fs::read_to_string(path) else { return AppConfig::default(); };
    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    let data = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}
