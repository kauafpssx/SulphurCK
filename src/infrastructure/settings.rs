//! Filesystem adapter for application settings.

use crate::domain::config::AppConfig;
use std::path::{Path, PathBuf};

const DASHBOARD_KEY_FILE: &str = "dashboard.key";

/// Default settings location under the current user's configuration directory.
pub fn default_settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sulphurck")
        .join("settings.json")
}

/// Returns the persistent dashboard access key, creating a cryptographically
/// random key the first time the dashboard is started.
///
/// The key deliberately lives in the user's local configuration directory,
/// rather than in the repository or an application config committed to Git.
pub fn dashboard_access_key() -> anyhow::Result<String> {
    let path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sulphurck")
        .join(DASHBOARD_KEY_FILE);

    if path.exists() {
        let key = std::fs::read_to_string(&path)?.trim().to_owned();
        if !key.is_empty() {
            return Ok(key);
        }
    }

    let key = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("dashboard key path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(path, &key)?;
    Ok(key)
}

/// Loads settings from disk, returning the domain defaults when no file exists.
pub fn load(path: &Path) -> anyhow::Result<AppConfig> {
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    } else {
        Ok(AppConfig::default())
    }
}

/// Persists an application configuration through the filesystem adapter.
pub fn save(config: &AppConfig, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(config)?;
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_domain_defaults() {
        let path = std::env::temp_dir().join("sulphurck-missing-settings.json");
        let config = load(&path).expect("a missing settings file should use defaults");
        assert_eq!(config.general.request_timeout, 10);
    }
}
