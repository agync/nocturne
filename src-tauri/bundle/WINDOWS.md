# Building the Windows installers

The Windows installers (NSIS and MSI) are built on a Windows machine. Nothing here is built on
Linux, and v1 does not publish them yet. `windows.json` in this folder is the configuration the
build uses.

## Prerequisites

- Windows 10 or 11, x86_64, with Git for Windows (the build scripts run under Git Bash).
- Rust with the MSVC toolchain (`rustup default stable-x86_64-pc-windows-msvc`) and the Visual
  Studio Build Tools "Desktop development with C++" workload.
- Node.js 22 or newer.
- The WebView2 runtime, to run the built app on the build machine (it ships with current
  Windows 11 and with Microsoft Edge).

The Tauri command line tool is pinned in `ui/package.json` and installed by `npm ci`. It downloads
NSIS and the WiX toolset into its own cache the first time it builds, which is a developer-time
download and never happens when the app runs.

## Build

From Git Bash, at the repository root:

```bash
scripts/package.sh --config windows
```

This fetches the pinned Windows Lemonade archive, verifies its sha256 against `lemonade.lock`,
unpacks it under `target/package/lemonade/windows-x86_64/`, records a sha256 manifest of every
file, builds the interface, writes the third-party notices, and runs
`tauri build --config bundle/windows.json --bundles nsis,msi` from `src-tauri/`. The installers
and a `SHA256SUMS` file are copied to `target/package/windows/`.

`scripts/package.sh --dry-run --config windows` prints the merged configuration and the steps
without building anything. It runs on any machine.

## What the installers do

- NSIS installs per user (`currentUser`), so it needs no administrator rights. The MSI is for
  managed fleets.
- WebView2 uses `embedBootstrapper`: a small bootstrapper is embedded in the installer and fetches
  the runtime only when it is missing, at install time, with the person present. When WebView2 is
  already installed, nothing is downloaded.
- The private engine is installed as the resource folder `lemonade/`, next to the app.
- The installers are unsigned in v1. Windows SmartScreen warns on first run; "More info" then
  "Run anyway" continues. Signing is deferred.

## Behaviour to know

- Notifications work only from the installed app, because Windows ties them to the app's
  registered identity. A `tauri dev` run shows none.
- The app does not listen for Windows resume events in v1. After sleep, the scheduler notices
  the wall-clock jump within one 30 second tick and carries on.
- The default quick-keep shortcut is Ctrl+Shift+Space.
