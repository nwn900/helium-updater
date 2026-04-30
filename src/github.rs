use crate::{
    common::AppResult, helium::NativeArchitecture, powershell, versioning::normalize_version_string,
};
use serde::Deserialize;

pub const RELEASES_REPOSITORY_SLUG: &str = "imputnet/helium-windows";
pub const RELEASES_REPOSITORY_URL: &str = "https://github.com/imputnet/helium-windows/releases";

const RELEASES_ENDPOINT: &str =
    "https://api.github.com/repos/imputnet/helium-windows/releases/latest";

#[derive(Clone, Debug)]
pub struct ReleaseAsset {
    pub name: String,
    pub digest: Option<String>,
    pub download_url: String,
}

#[derive(Clone, Debug)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub published_at: String,
    pub chromium_version: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    pub fn select_installer_asset(
        &self,
        architecture: NativeArchitecture,
    ) -> AppResult<ReleaseAsset> {
        let mut ranked_assets = self
            .assets
            .iter()
            .filter(|asset| {
                let name = asset.name.to_ascii_lowercase();
                name.ends_with(".exe") && !name.contains("debug") && !name.contains("symbols")
            })
            .map(|asset| {
                let name = asset.name.to_ascii_lowercase();
                let mut rank = 0_u8;

                if name.contains("installer") {
                    rank += 10;
                }

                if architecture
                    .aliases()
                    .iter()
                    .any(|alias| name.contains(alias))
                {
                    rank += 20;
                }

                (rank, asset.clone())
            })
            .collect::<Vec<_>>();

        ranked_assets
            .sort_by(|left, right| right.0.cmp(&left.0).then(left.1.name.cmp(&right.1.name)));

        ranked_assets
            .into_iter()
            .map(|(_, asset)| asset)
            .next()
            .ok_or_else(|| "could not find a matching Helium Windows installer asset".to_owned())
    }
}

pub fn fetch_latest_release() -> AppResult<ReleaseInfo> {
    let script = format!(
        r#"
$headers = @{{
    'Accept' = 'application/vnd.github+json'
    'User-Agent' = 'HeliumBrowserUpdater/0.3.1'
}}
Invoke-RestMethod -Headers $headers -Uri '{endpoint}' | ConvertTo-Json -Depth 8 -Compress
"#,
        endpoint = RELEASES_ENDPOINT,
    );

    let release: GitHubRelease = powershell::run_script_json(&script)?;
    let tag_name = normalize_version_string(&release.tag_name).unwrap_or(release.tag_name);

    if tag_name.trim().is_empty() {
        return Err("GitHub returned an empty release tag".to_owned());
    }

    Ok(ReleaseInfo {
        tag_name,
        published_at: friendly_timestamp(&release.published_at),
        chromium_version: extract_chromium_version(&release.body),
        assets: release
            .assets
            .into_iter()
            .map(|asset| ReleaseAsset {
                name: asset.name,
                digest: asset.digest,
                download_url: asset.browser_download_url,
            })
            .collect(),
    })
}

fn extract_chromium_version(body: &str) -> Option<String> {
    for line in body.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("chromium") {
            if let Some(version) = normalize_version_string(line) {
                return Some(version);
            }
        }
    }

    None
}

fn friendly_timestamp(value: &str) -> String {
    value.replace('T', " ").replace('Z', " UTC")
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    published_at: String,
    body: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    digest: Option<String>,
    browser_download_url: String,
}
