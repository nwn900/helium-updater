# Helium Browser Updater

Windows GUI updater for the Helium browser, written in Rust.

## What it does

- Shows the currently installed Helium version.
- Checks the latest release from `imputnet/helium-windows`.
- Downloads and installs the latest Windows Helium build.
- Creates a daily scheduled task so updates can run automatically in the background.
- Lets the user disable or re-enable the automatic updater from the GUI.
- Never launches the browser itself after checks or installs.

## GUI features

- `Automatic daily updates` toggle.
- `Check for updates` button.
- `Download and install Helium` button when Helium is missing.
- `Download and install update` button when a newer release is available.
- Status area with the last check time and updater messages.

## Build

```powershell
cargo build --release
```

The Windows executable is created at:

```text
target\release\helium-updater.exe
```

## Background mode

The app registers a Task Scheduler job that runs:

```text
helium-updater.exe --background
```

That background mode only auto-updates an existing Helium installation. If Helium is not installed yet, the GUI offers the install button instead of forcing a background install.

## Data files

The updater stores its runtime files in:

```text
%LOCALAPPDATA%\HeliumUpdater
```

This includes:

- `config.json`
- `state.json`
- `downloads\`
- `logs\updater.log`
