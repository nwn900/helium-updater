use crate::common::AppResult;
use serde::de::DeserializeOwned;
use std::{env, path::Path, process::Command};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn run_script(script: &str) -> AppResult<String> {
    let powershell = env::var_os("WINDIR")
        .map(|windir| {
            Path::new(&windir)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|path| path.exists())
        .unwrap_or_else(|| Path::new("powershell.exe").to_path_buf());

    let mut command = Command::new(powershell);
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .map_err(|error| format!("failed to run PowerShell: {error}"))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let details = if !stderr.is_empty() { stderr } else { stdout };

    if details.is_empty() {
        return Err(format!(
            "PowerShell exited with code {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    Err(details)
}

pub fn run_script_json<T: DeserializeOwned>(script: &str) -> AppResult<T> {
    let output = run_script(script)?;
    serde_json::from_str(&output)
        .map_err(|error| format!("failed to decode PowerShell JSON: {error}"))
}

pub fn literal(value: &str) -> String {
    value.replace('\'', "''")
}
