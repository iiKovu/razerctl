use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Persisted user configuration.
///
/// Saved to `~/.config/blackshark/config.toml` on every change (debounced).
/// Restored to the headset on every device connect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub sidetone: u8,
    pub eq_preset: u8,
    pub thx_enabled: bool,
    pub anc_enabled: bool,
    pub anc_level: u8,             // 1–4
    pub power_savings_minutes: u8, // 0=off, 15/30/45/60
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sidetone: 0,
            eq_preset: 0,
            thx_enabled: false,
            anc_enabled: false,
            anc_level: 1,
            power_savings_minutes: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

pub fn config_path() -> Result<PathBuf> {
    resolve_config_path(
        std::env::var_os("BLACKSHARK_DATA_DIR").map(PathBuf::from),
        dirs::config_dir(),
    )
}

fn resolve_config_path(
    portable_data_dir: Option<PathBuf>,
    config_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(base) = portable_data_dir {
        return Ok(base.join("config.toml"));
    }

    let base = config_dir.context("could not determine config directory")?;
    Ok(base.join("blackshark").join("config.toml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).context("failed to parse config.toml")
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(config).context("failed to serialise config")?;
    std::fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_data_directory_takes_precedence() {
        let path = resolve_config_path(
            Some(PathBuf::from("/portable/data")),
            Some(PathBuf::from("/home/test/.config")),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("/portable/data/config.toml"));
    }

    #[test]
    fn xdg_config_directory_remains_the_fallback() {
        let path = resolve_config_path(None, Some(PathBuf::from("/home/test/.config"))).unwrap();

        assert_eq!(
            path,
            PathBuf::from("/home/test/.config/blackshark/config.toml")
        );
    }

    #[test]
    fn missing_all_config_directories_is_an_error() {
        assert!(resolve_config_path(None, None).is_err());
    }
}
