use crate::{common::AppResult, paths::AppPaths};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub last_checked_at: Option<String>,
    pub last_seen_release_tag: Option<String>,
    pub last_seen_product_version: Option<String>,
    pub latest_release_published_at: Option<String>,
    pub installed_display_version: Option<String>,
    pub installed_product_version: Option<String>,
    pub last_status_message: Option<String>,
    pub last_error: Option<String>,
    pub pending_update_notification: Option<String>,
}

impl State {
    pub fn load(paths: &AppPaths) -> AppResult<Self> {
        if !paths.state_path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&paths.state_path)
            .map_err(|error| format!("failed to read {}: {error}", paths.state_path.display()))?;

        serde_json::from_str::<Self>(&raw)
            .map_err(|error| format!("failed to parse {}: {error}", paths.state_path.display()))
    }

    pub fn save(&self, paths: &AppPaths) -> AppResult<()> {
        paths.ensure()?;

        let raw = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize state: {error}"))?;
        fs::write(&paths.state_path, raw)
            .map_err(|error| format!("failed to write {}: {error}", paths.state_path.display()))
    }
}
