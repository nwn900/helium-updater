use crate::{common::AppResult, powershell};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum NativeArchitecture {
    X64,
    Arm64,
}

impl NativeArchitecture {
    pub fn detect() -> Self {
        let native_architecture = std::env::var("PROCESSOR_ARCHITEW6432")
            .or_else(|_| std::env::var("PROCESSOR_ARCHITECTURE"))
            .unwrap_or_else(|_| "AMD64".to_owned());

        if native_architecture.to_ascii_uppercase().contains("ARM64") {
            Self::Arm64
        } else {
            Self::X64
        }
    }

    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Arm64 => &["arm64", "aarch64"],
            Self::X64 => &["x64", "x86_64", "amd64", "win64"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X64 => "x64",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct InstalledHelium {
    pub install_root: Option<PathBuf>,
    pub executable_path: Option<PathBuf>,
    pub display_version: Option<String>,
    pub product_version: Option<String>,
}

impl InstalledHelium {
    pub fn is_installed(&self) -> bool {
        self.install_root.is_some()
            || self.executable_path.is_some()
            || self.display_version.is_some()
            || self.product_version.is_some()
    }

    pub fn current_version(&self) -> Option<String> {
        self.display_version
            .clone()
            .or_else(|| self.product_version.clone())
    }
}

pub fn detect_installed_helium() -> AppResult<Option<InstalledHelium>> {
    let script = r#"
function Normalize-VersionString {
    param([string]$Version)
    if ([string]::IsNullOrWhiteSpace($Version)) { return $null }
    $match = [regex]::Match($Version, '\d+(?:\.\d+){1,3}')
    if (-not $match.Success) { return $null }
    $match.Value
}

function Convert-VersionObject {
    param([string]$Version)
    $normalized = Normalize-VersionString -Version $Version
    if (-not $normalized) { return $null }
    $parts = $normalized.Split('.')
    while ($parts.Count -lt 4) { $parts += '0' }
    if ($parts.Count -gt 4) { $parts = $parts[0..3] }
    [Version]($parts -join '.')
}

function Get-FileProductVersion {
    param([string]$Path)
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return $null }
    try {
        $item = Get-Item -LiteralPath $Path
        $productVersion = Normalize-VersionString -Version $item.VersionInfo.ProductVersion
        if ($productVersion) { return $productVersion }
        $fileVersion = Normalize-VersionString -Version $item.VersionInfo.FileVersion
        if ($fileVersion) { return $fileVersion }
    } catch {
    }
    return $null
}

$uninstallRoots = @(
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
)

$entries = foreach ($root in $uninstallRoots) {
    if (-not (Test-Path -LiteralPath $root)) { continue }

    Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            $item = Get-ItemProperty -LiteralPath $_.PSPath
            if ($item.DisplayName -eq 'Helium') {
                [pscustomobject]@{
                    InstallLocation = $item.InstallLocation
                    DisplayVersion = $item.DisplayVersion
                    UninstallString = $item.UninstallString
                }
            }
        } catch {
        }
    }
}

$entry = $entries |
    Sort-Object -Property @{ Expression = { Convert-VersionObject -Version $_.DisplayVersion }; Descending = $true } |
    Select-Object -First 1

if (-not $entry) { return }

$installRoot = $entry.InstallLocation
if ([string]::IsNullOrWhiteSpace([string]$installRoot)) { $installRoot = $null }

$exeCandidates = @()
if ($installRoot) {
    $exeCandidates += (Join-Path $installRoot 'helium.exe')
    $exeCandidates += (Join-Path $installRoot 'chrome.exe')
    $exeCandidates += (Join-Path $installRoot 'Application\helium.exe')
    $exeCandidates += (Join-Path $installRoot 'Application\chrome.exe')
}

$exePath = $exeCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

if (-not $exePath -and $entry.UninstallString -match '"([^"]*setup\.exe)"') {
    $setupPath = $Matches[1]
    $candidateRoot = Split-Path -Path (Split-Path -Path (Split-Path -Path $setupPath -Parent) -Parent) -Parent
    $exeCandidates = @(
        (Join-Path $candidateRoot 'helium.exe'),
        (Join-Path $candidateRoot 'chrome.exe'),
        (Join-Path $candidateRoot 'Application\helium.exe'),
        (Join-Path $candidateRoot 'Application\chrome.exe')
    )
    $exePath = $exeCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if ($exePath -and -not $installRoot) { $installRoot = $candidateRoot }
}

[pscustomobject]@{
    install_root = $installRoot
    executable_path = $exePath
    display_version = Normalize-VersionString -Version $entry.DisplayVersion
    product_version = Get-FileProductVersion -Path $exePath
} | ConvertTo-Json -Compress
"#;

    let output = powershell::run_script(script)?;
    if output.trim().is_empty() {
        return Ok(None);
    }

    let detected = serde_json::from_str::<DetectedHelium>(&output)
        .map_err(|error| format!("failed to decode installed Helium metadata: {error}"))?;

    Ok(Some(InstalledHelium {
        install_root: detected.install_root.map(PathBuf::from),
        executable_path: detected.executable_path.map(PathBuf::from),
        display_version: detected.display_version,
        product_version: detected.product_version,
    }))
}

pub fn is_running(helium: &InstalledHelium) -> AppResult<bool> {
    let Some(install_root) = helium.install_root.as_ref() else {
        return Ok(false);
    };

    let script = format!(
        r#"
$installRoot = '{install_root}'
$prefix = ([IO.Path]::GetFullPath($installRoot)).TrimEnd('\') + '\'
$processes = foreach ($name in @('helium.exe', 'chrome.exe')) {{
    Get-CimInstance -ClassName Win32_Process -Filter "Name = '$name'" -ErrorAction SilentlyContinue
}}
$running = @($processes | Where-Object {{
    $_.ExecutablePath -and ([IO.Path]::GetFullPath($_.ExecutablePath)).StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)
}})
[pscustomobject]@{{ running = [bool]($running.Count -gt 0) }} | ConvertTo-Json -Compress
"#,
        install_root = powershell::literal(&install_root.to_string_lossy()),
    );

    let result: RunningState = powershell::run_script_json(&script)?;
    Ok(result.running)
}

pub fn close_running(helium: &InstalledHelium) -> AppResult<()> {
    let Some(install_root) = helium.install_root.as_ref() else {
        return Ok(());
    };

    let script = format!(
        r#"
$installRoot = '{install_root}'
$prefix = ([IO.Path]::GetFullPath($installRoot)).TrimEnd('\') + '\'
$processes = foreach ($name in @('helium.exe', 'chrome.exe')) {{
    Get-CimInstance -ClassName Win32_Process -Filter "Name = '$name'" -ErrorAction SilentlyContinue
}}
$running = @($processes | Where-Object {{
    $_.ExecutablePath -and ([IO.Path]::GetFullPath($_.ExecutablePath)).StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)
}})
if ($running.Count -gt 0) {{
    $running | Select-Object -ExpandProperty ProcessId | Stop-Process -Force
    Start-Sleep -Seconds 2
}}
"#,
        install_root = powershell::literal(&install_root.to_string_lossy()),
    );

    powershell::run_script(&script).map(|_| ())
}

pub fn launch(helium: &InstalledHelium) -> AppResult<()> {
    let exe_path_buf;
    let exe_path: &std::path::Path = match helium.executable_path.as_ref() {
        Some(path) => path,
        None => {
            let install_root = helium.install_root.as_ref()
                .ok_or_else(|| "no Helium executable path available to restart the browser".to_owned())?;
            exe_path_buf = install_root.join("helium.exe");
            &exe_path_buf
        }
    };

    let script = format!(
        r#"
Start-Process -FilePath '{exe_path}' -WindowStyle Minimized
"#,
        exe_path = powershell::literal(&exe_path.to_string_lossy()),
    );

    powershell::run_script(&script).map(|_| ())
}

#[derive(Debug, Deserialize)]
struct DetectedHelium {
    install_root: Option<String>,
    executable_path: Option<String>,
    display_version: Option<String>,
    product_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunningState {
    running: bool,
}
