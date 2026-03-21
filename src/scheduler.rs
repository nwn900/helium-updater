use crate::{common::AppResult, powershell};
use std::path::Path;

pub const TASK_NAME: &str = "Helium Browser Updater";
const DAILY_TIME: &str = "10:00AM";

pub fn ensure_daily_task(executable: &Path) -> AppResult<()> {
    let executable = executable
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", executable.display()))?;

    let script = format!(
        r#"
$taskName = '{task_name}'
$exePath = '{exe_path}'
$action = New-ScheduledTaskAction -Execute $exePath -Argument '--background'
$trigger = New-ScheduledTaskTrigger -Daily -At '{daily_time}'
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
$principal = New-ScheduledTaskPrincipal -UserId ([System.Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Description 'Checks Helium releases daily and installs updates automatically.' -Force | Out-Null
"#,
        task_name = powershell::literal(TASK_NAME),
        exe_path = powershell::literal(&executable.to_string_lossy()),
        daily_time = DAILY_TIME,
    );

    powershell::run_script(&script).map(|_| ())
}

pub fn remove_daily_task() -> AppResult<()> {
    let script = format!(
        r#"
$taskName = '{task_name}'
$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
if ($task) {{
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
}}
"#,
        task_name = powershell::literal(TASK_NAME),
    );

    powershell::run_script(&script).map(|_| ())
}

pub fn daily_task_exists() -> AppResult<bool> {
    let script = format!(
        r#"
$taskName = '{task_name}'
$task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
[pscustomobject]@{{ exists = [bool]$task }} | ConvertTo-Json -Compress
"#,
        task_name = powershell::literal(TASK_NAME),
    );

    let output = powershell::run_script_json::<TaskPresence>(&script)?;
    Ok(output.exists)
}

#[derive(serde::Deserialize)]
struct TaskPresence {
    exists: bool,
}
