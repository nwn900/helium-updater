use crate::{common::AppResult, paths::AppPaths};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub automatic_updates_enabled: bool,
    pub close_running_helium: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            automatic_updates_enabled: true,
            close_running_helium: false,
        }
    }
}

impl Config {
    pub fn load(paths: &AppPaths) -> AppResult<Self> {
        if !paths.config_path.exists() {
            let config = Self::default();
            config.save(paths)?;
            return Ok(config);
        }

        let raw = fs::read_to_string(&paths.config_path)
            .map_err(|error| format!("failed to read {}: {error}", paths.config_path.display()))?;

        serde_json::from_str::<Self>(&raw)
            .map_err(|error| format!("failed to parse {}: {error}", paths.config_path.display()))
    }

    pub fn save(&self, paths: &AppPaths) -> AppResult<()> {
        paths.ensure()?;

        let raw = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize config: {error}"))?;
        fs::write(&paths.config_path, raw)
            .map_err(|error| format!("failed to write {}: {error}", paths.config_path.display()))
    }
}
