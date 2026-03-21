use crate::{paths::AppPaths, powershell};
use std::{
    fs::OpenOptions,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn info(paths: &AppPaths, message: impl AsRef<str>) {
    write_line(paths, "INFO", message.as_ref());
}

pub fn warn(paths: &AppPaths, message: impl AsRef<str>) {
    write_line(paths, "WARN", message.as_ref());
}

pub fn error(paths: &AppPaths, message: impl AsRef<str>) {
    write_line(paths, "ERROR", message.as_ref());
}

fn write_line(paths: &AppPaths, level: &str, message: &str) {
    let _ = paths.ensure();

    let timestamp = powershell::run_script("Get-Date -Format 'yyyy-MM-dd HH:mm:ss'")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_owned())
        });

    let line = format!("[{timestamp}] [{level}] {message}");

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log_path)
    {
        let _ = writeln!(file, "{line}");
    }
}
