use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    pub api_key: String,
    pub api_secret: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fmp_api_key: Option<String>,
}

pub fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("could not resolve OS config dir")?;
    Ok(dir.join("alpaca-tui").join("credentials.json"))
}

pub fn load_credentials() -> Result<Credentials> {
    let path = config_path()?;
    let data = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&data)?)
}

pub fn save_credentials(creds: &Credentials) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(creds)?;
    fs::write(&path, data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn delete_credentials() {
    if let Ok(path) = config_path() {
        let _ = fs::remove_file(path);
    }
}
