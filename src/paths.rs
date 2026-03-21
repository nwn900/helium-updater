use crate::common::AppResult;
use std::{env, fs, path::PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub config_path: PathBuf,
    pub state_path: PathBuf,
    pub log_path: PathBuf,
    pub lock_path: PathBuf,
}

impl AppPaths {
    pub fn discover() -> AppResult<Self> {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "LOCALAPPDATA is not available".to_owned())?;

        if local_app_data.as_os_str().is_empty() {
            return Err("LOCALAPPDATA is empty".to_owned());
        }

        let data_dir = local_app_data.join("HeliumUpdater");
        let downloads_dir = data_dir.join("downloads");
        let logs_dir = data_dir.join("logs");

        Ok(Self {
            config_path: data_dir.join("config.json"),
            state_path: data_dir.join("state.json"),
            log_path: logs_dir.join("updater.log"),
            lock_path: data_dir.join("update.lock"),
            data_dir,
            downloads_dir,
            logs_dir,
        })
    }

    pub fn ensure(&self) -> AppResult<()> {
        for directory in [&self.data_dir, &self.downloads_dir, &self.logs_dir] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("failed to create {}: {error}", directory.display()))?;
        }

        Ok(())
    }
}
