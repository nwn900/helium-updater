use crate::{
    common::AppResult,
    config::Config,
    github::{ReleaseAsset, ReleaseInfo, fetch_latest_release},
    helium::{
        InstalledHelium, NativeArchitecture, close_running, detect_installed_helium, is_running,
    },
    logging,
    paths::AppPaths,
    powershell, scheduler,
    state::State,
    versioning::{compare_versions, parse_version},
};
use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug)]
pub struct DashboardSnapshot {
    pub automatic_updates_enabled: bool,
    pub scheduled_task_present: bool,
    pub installed_version: Option<String>,
    pub installed_product_version: Option<String>,
    pub latest_release_tag: Option<String>,
    pub latest_product_version: Option<String>,
    pub latest_release_published_at: Option<String>,
    pub last_checked_at: Option<String>,
    pub update_available: Option<bool>,
    pub status_message: String,
    pub architecture_label: String,
    pub is_installed: bool,
    pub pending_update_notification: Option<String>,
}

impl DashboardSnapshot {
    pub fn installed_label(&self) -> &str {
        self.installed_version.as_deref().unwrap_or("Not installed")
    }

    pub fn latest_label(&self) -> &str {
        self.latest_release_tag
            .as_deref()
            .unwrap_or("Not checked yet")
    }

    pub fn last_checked_label(&self) -> &str {
        self.last_checked_at.as_deref().unwrap_or("Never")
    }
}

#[derive(Clone)]
pub struct AppService {
    paths: AppPaths,
}

impl AppService {
    pub fn new() -> AppResult<Self> {
        let paths = AppPaths::discover()?;
        paths.ensure()?;
        Ok(Self { paths })
    }

    pub fn initial_snapshot(&self) -> DashboardSnapshot {
        let config = Config::load(&self.paths).unwrap_or_default();
        let installed = detect_installed_helium().ok().flatten();
        let state = State::load(&self.paths).unwrap_or_default();
        let scheduled_task_present = scheduler::daily_task_exists().unwrap_or(false);

        let release = fetch_latest_release().ok();
        let update_available = release.as_ref().map(|r| {
            self.compute_update_available(installed.as_ref(), r, &state)
        });

        self.compose_snapshot(
            config,
            state,
            installed,
            release,
            update_available,
            scheduled_task_present,
        )
    }

    pub fn startup_refresh(&self) -> AppResult<DashboardSnapshot> {
        let config = Config::load(&self.paths)?;
        if let Err(error) = self.sync_scheduler(&config) {
            self.record_error(&format!("Task Scheduler sync failed: {error}"));
        }

        let installed = detect_installed_helium()?;
        let mut state = State::load(&self.paths)?;
        let release = fetch_latest_release()?;
        let update_available =
            Some(self.compute_update_available(installed.as_ref(), &release, &state));
        let current_time = current_timestamp()?;
        let scheduled_task_present = scheduler::daily_task_exists().unwrap_or(false);

        state.last_checked_at = Some(current_time);
        state.last_seen_release_tag = Some(release.tag_name.clone());
        state.last_seen_product_version = release.chromium_version.clone();
        state.latest_release_published_at = Some(release.published_at.clone());
        state.installed_display_version = installed
            .as_ref()
            .and_then(InstalledHelium::current_version);
        state.installed_product_version = installed
            .as_ref()
            .and_then(|item| item.product_version.clone());
        state.last_status_message = Some(match update_available {
            Some(true) if installed.is_some() => {
                format!("Helium {} is available to install.", release.tag_name)
            }
            Some(true) => format!("Helium {} is ready to install.", release.tag_name),
            _ if installed.is_some() => "Helium is up to date.".to_owned(),
            _ => format!(
                "Helium is not installed. Latest release: {}.",
                release.tag_name
            ),
        });
        state.last_error = None;
        state.save(&self.paths)?;

        Ok(self.compose_snapshot(
            config,
            state,
            installed,
            Some(release),
            update_available,
            scheduled_task_present,
        ))
    }

    pub fn set_automatic_updates(&self, enabled: bool) -> AppResult<DashboardSnapshot> {
        let mut config = Config::load(&self.paths)?;
        config.automatic_updates_enabled = enabled;
        config.save(&self.paths)?;
        self.sync_scheduler(&config)?;

        let mut snapshot = self.initial_snapshot();
        snapshot.automatic_updates_enabled = enabled;
        snapshot.scheduled_task_present = enabled;
        snapshot.status_message = if enabled {
            "Automatic daily updates are enabled.".to_owned()
        } else {
            "Automatic daily updates are disabled.".to_owned()
        };

        Ok(snapshot)
    }

    pub fn check_for_updates(&self) -> AppResult<DashboardSnapshot> {
        self.startup_refresh()
    }

    pub fn dismiss_pending_notification(&self) -> AppResult<DashboardSnapshot> {
        let mut state = State::load(&self.paths)?;
        state.pending_update_notification = None;
        state.save(&self.paths)?;
        Ok(self.initial_snapshot())
    }

    pub fn delete_scheduled_task(&self) -> AppResult<DashboardSnapshot> {
        let mut config = Config::load(&self.paths)?;
        config.automatic_updates_enabled = false;
        config.save(&self.paths)?;
        scheduler::remove_daily_task()?;

        let mut snapshot = self.initial_snapshot();
        snapshot.automatic_updates_enabled = false;
        snapshot.scheduled_task_present = false;
        snapshot.status_message =
            "The scheduled task was deleted and automatic updates were disabled.".to_owned();
        Ok(snapshot)
    }

    pub fn install_or_update_now(&self) -> AppResult<DashboardSnapshot> {
        self.with_operation_lock(|service| {
            logging::info(
                &service.paths,
                "Starting a manual Helium install or update.",
            );

            let mut state = State::load(&service.paths)?;
            let installed = detect_installed_helium()?;
            let release = fetch_latest_release()?;
            let update_available =
                service.compute_update_available(installed.as_ref(), &release, &state);
            let should_install = !installed
                .as_ref()
                .is_some_and(InstalledHelium::is_installed)
                || update_available;

            if !should_install {
                state.last_checked_at = Some(current_timestamp()?);
                state.last_status_message = Some("Helium is already up to date.".to_owned());
                state.last_error = None;
                state.save(&service.paths)?;
                return service.startup_refresh();
            }

            if let Some(helium) = installed.as_ref() {
                if is_running(helium)? {
                    logging::warn(&service.paths, "Helium is running; attempting to close it for manual install.");
                    close_running(helium)?;
                }
            }

            let asset = release.select_installer_asset(NativeArchitecture::detect())?;
            let installer_path = service.download_asset(&asset)?;
            service.run_installer(&installer_path)?;
            let _ = fs::remove_file(&installer_path);
            thread::sleep(Duration::from_secs(2));

            let refreshed_install = detect_installed_helium()?;

            state.last_checked_at = Some(current_timestamp()?);
            state.last_seen_release_tag = Some(release.tag_name.clone());
            state.last_seen_product_version = release.chromium_version.clone();
            state.latest_release_published_at = Some(release.published_at.clone());
            state.installed_display_version = refreshed_install
                .as_ref()
                .and_then(InstalledHelium::current_version);
            state.installed_product_version = refreshed_install
                .as_ref()
                .and_then(|item| item.product_version.clone());
            state.last_status_message = Some(match refreshed_install.as_ref() {
                Some(helium) => format!(
                    "Helium {} is installed.",
                    helium
                        .current_version()
                        .unwrap_or_else(|| release.tag_name.clone())
                ),
                None => format!("Helium {} installer finished.", release.tag_name),
            });
            state.last_error = None;
            state.pending_update_notification = None;
            state.save(&service.paths)?;

            logging::info(&service.paths, "Manual Helium install or update completed.");

            service.startup_refresh()
        })
    }

    pub fn run_background_update(&self) -> AppResult<()> {
        self.with_operation_lock(|service| {
            logging::info(&service.paths, "Background updater run started.");

            let config = Config::load(&service.paths)?;
            if !config.automatic_updates_enabled {
                logging::info(
                    &service.paths,
                    "Automatic updates are disabled; background run skipped.",
                );
                return Ok(());
            }

            if let Err(error) = service.sync_scheduler(&config) {
                logging::warn(
                    &service.paths,
                    format!("Task Scheduler sync failed during background run: {error}"),
                );
            }

            let installed = detect_installed_helium()?;
            let Some(installed) = installed else {
                logging::info(
                    &service.paths,
                    "Helium is not installed; background mode will not perform a fresh install.",
                );
                return Ok(());
            };

            let mut state = State::load(&service.paths)?;
            let release = fetch_latest_release()?;

            if !service.compute_update_available(Some(&installed), &release, &state) {
                logging::info(&service.paths, "Helium is already up to date.");
                state.last_checked_at = Some(current_timestamp()?);
                state.last_seen_release_tag = Some(release.tag_name);
                state.last_seen_product_version = release.chromium_version;
                state.latest_release_published_at = Some(release.published_at);
                state.installed_display_version = installed.current_version();
                state.installed_product_version = installed.product_version.clone();
                state.last_status_message = Some("Helium is up to date.".to_owned());
                state.last_error = None;
                state.save(&service.paths)?;
                return Ok(());
            }

            if is_running(&installed)? {
                if config.close_running_helium {
                    logging::warn(&service.paths, "Helium is running; the updater will close it.");
                    close_running(&installed)?;
                } else {
                    logging::warn(&service.paths, "Helium is running; background update skipped.");
                    state.pending_update_notification = Some(format!(
                        "Helium {} is available but could not install because Helium is running. Open the updater to install manually.",
                        release.tag_name
                    ));
                    state.last_checked_at = Some(current_timestamp()?);
                    state.last_seen_release_tag = Some(release.tag_name);
                    state.last_seen_product_version = release.chromium_version;
                    state.latest_release_published_at = Some(release.published_at);
                    state.installed_display_version = installed.current_version();
                    state.installed_product_version = installed.product_version.clone();
                    state.last_error = None;
                    state.save(&service.paths)?;
                    return Ok(());
                }
            }

            let asset = release.select_installer_asset(NativeArchitecture::detect())?;
            let installer_path = service.download_asset(&asset)?;
            service.run_installer(&installer_path)?;
            let _ = fs::remove_file(&installer_path);
            thread::sleep(Duration::from_secs(2));

            let refreshed_install = detect_installed_helium()?;
            state.last_checked_at = Some(current_timestamp()?);
            state.last_seen_release_tag = Some(release.tag_name.clone());
            state.last_seen_product_version = release.chromium_version.clone();
            state.latest_release_published_at = Some(release.published_at.clone());
            state.installed_display_version = refreshed_install
                .as_ref()
                .and_then(InstalledHelium::current_version);
            state.installed_product_version = refreshed_install
                .as_ref()
                .and_then(|item| item.product_version.clone());
            state.last_status_message = Some(match refreshed_install.as_ref() {
                Some(helium) => format!(
                    "Background update installed Helium {}.",
                    helium.current_version().unwrap_or(release.tag_name)
                ),
                None => "Background update completed.".to_owned(),
            });
            state.last_error = None;
            state.save(&service.paths)?;

            logging::info(&service.paths, "Background updater run completed.");
            Ok(())
        })
    }

    pub fn record_error(&self, message: &str) {
        logging::error(&self.paths, message);

        if let Ok(mut state) = State::load(&self.paths) {
            state.last_error = Some(message.to_owned());
            let _ = state.save(&self.paths);
        }
    }

    fn compose_snapshot(
        &self,
        config: Config,
        state: State,
        installed: Option<InstalledHelium>,
        release: Option<ReleaseInfo>,
        update_available: Option<bool>,
        scheduled_task_present: bool,
    ) -> DashboardSnapshot {
        let status_message = state
            .last_error
            .clone()
            .or(state.last_status_message.clone())
            .unwrap_or_else(|| "Ready.".to_owned());

        DashboardSnapshot {
            automatic_updates_enabled: config.automatic_updates_enabled,
            scheduled_task_present,
            installed_version: installed
                .as_ref()
                .and_then(InstalledHelium::current_version)
                .or(state.installed_display_version.clone()),
            installed_product_version: installed
                .as_ref()
                .and_then(|item| item.product_version.clone())
                .or(state.installed_product_version.clone()),
            latest_release_tag: release
                .as_ref()
                .map(|item| item.tag_name.clone())
                .or(state.last_seen_release_tag.clone()),
            latest_product_version: release
                .as_ref()
                .and_then(|item| item.chromium_version.clone())
                .or(state.last_seen_product_version.clone()),
            latest_release_published_at: release
                .as_ref()
                .map(|item| item.published_at.clone())
                .or(state.latest_release_published_at.clone()),
            last_checked_at: state.last_checked_at.clone(),
            update_available,
            status_message,
            architecture_label: NativeArchitecture::detect().label().to_owned(),
            is_installed: installed
                .as_ref()
                .is_some_and(InstalledHelium::is_installed),
            pending_update_notification: state.pending_update_notification.clone(),
        }
    }

    fn sync_scheduler(&self, config: &Config) -> AppResult<()> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to locate the updater executable: {error}"))?;

        if config.automatic_updates_enabled {
            scheduler::ensure_daily_task(&executable)?;
            logging::info(&self.paths, "Automatic daily update task is registered.");
        } else {
            scheduler::remove_daily_task()?;
            logging::info(&self.paths, "Automatic daily update task is removed.");
        }

        Ok(())
    }

    fn compute_update_available(
        &self,
        installed: Option<&InstalledHelium>,
        release: &ReleaseInfo,
        state: &State,
    ) -> bool {
        let Some(installed) = installed else {
            return true;
        };

        let installed_chromium_candidate = installed.product_version.as_deref().or_else(|| {
            installed
                .display_version
                .as_deref()
                .filter(|value| looks_like_chromium(value))
        });

        if let Some(ordering) = compare_versions(
            installed_chromium_candidate,
            release.chromium_version.as_deref(),
        ) {
            return ordering.is_lt();
        }

        if let Some(ordering) = compare_versions(
            installed.display_version.as_deref(),
            Some(&release.tag_name),
        ) {
            return ordering.is_lt();
        }

        if let Some(last_seen_tag) = state.last_seen_release_tag.as_deref() {
            return last_seen_tag != release.tag_name;
        }

        true
    }

    fn download_asset(&self, asset: &ReleaseAsset) -> AppResult<PathBuf> {
        self.paths.ensure()?;
        let destination = self.paths.downloads_dir.join(&asset.name);

        logging::info(
            &self.paths,
            format!("Downloading {} to {}.", asset.name, destination.display()),
        );

        let script = format!(
            r#"
$headers = @{{
    'User-Agent' = 'HeliumBrowserUpdater/0.3.0'
    'Accept' = 'application/octet-stream'
}}
Invoke-WebRequest -Headers $headers -Uri '{url}' -OutFile '{destination}'
"#,
            url = powershell::literal(&asset.download_url),
            destination = powershell::literal(&destination.to_string_lossy()),
        );

        powershell::run_script(&script).map(|_| ())?;
        self.verify_digest(asset, &destination)?;
        Ok(destination)
    }

    fn verify_digest(&self, asset: &ReleaseAsset, path: &Path) -> AppResult<()> {
        let Some(digest) = asset.digest.as_deref() else {
            return Ok(());
        };

        let Some(expected) = digest.strip_prefix("sha256:") else {
            return Ok(());
        };

        let script = format!(
            "(Get-FileHash -LiteralPath '{}' -Algorithm SHA256).Hash.ToLowerInvariant()",
            powershell::literal(&path.to_string_lossy())
        );

        let actual = powershell::run_script(&script)?.to_ascii_lowercase();
        if actual != expected.to_ascii_lowercase() {
            return Err(format!(
                "downloaded file hash mismatch for {} (expected sha256 {}, got {})",
                asset.name, expected, actual
            ));
        }

        Ok(())
    }

    fn run_installer(&self, installer_path: &Path) -> AppResult<()> {
        let script = format!(
            r#"
$process = Start-Process -FilePath '{installer}' -ArgumentList '/S' -Wait -PassThru
[pscustomobject]@{{ exit_code = $process.ExitCode }} | ConvertTo-Json -Compress
"#,
            installer = powershell::literal(&installer_path.to_string_lossy()),
        );

        let result: InstallerResult = powershell::run_script_json(&script)?;
        if result.exit_code != 0 {
            return Err(format!(
                "Helium installer exited with code {}",
                result.exit_code
            ));
        }

        Ok(())
    }

    fn with_operation_lock<T>(
        &self,
        operation: impl FnOnce(&Self) -> AppResult<T>,
    ) -> AppResult<T> {
        let _guard = OperationGuard::acquire(&self.paths.lock_path)?;
        operation(self)
    }
}

fn current_timestamp() -> AppResult<String> {
    powershell::run_script("Get-Date -Format 'yyyy-MM-dd HH:mm:ss'")
}

struct OperationGuard {
    path: PathBuf,
}

impl OperationGuard {
    fn acquire(path: &Path) -> AppResult<Self> {
        if let Ok(metadata) = fs::metadata(path) {
            let stale = metadata
                .modified()
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_some_and(|age| age > Duration::from_secs(12 * 60 * 60));

            if stale {
                let _ = fs::remove_file(path);
            }
        }

        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| "another Helium updater operation is already running".to_owned())?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Deserialize)]
struct InstallerResult {
    exit_code: i32,
}

fn looks_like_chromium(version: &str) -> bool {
    parse_version(version)
        .map(|parts| parts.0[0] >= 100)
        .unwrap_or(false)
}
