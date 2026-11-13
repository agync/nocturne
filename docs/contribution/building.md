---
type: Guide
title: Building and packaging
description: The development loop, the Linux packages, the checks that prove them, and the Windows build.
status: draft
tags: [contribution, build, packaging, windows]
---

# Building and packaging

Everything here runs on your own machine. Nothing is pushed, uploaded or published by any of it.

## The development loop

```bash
make setup          # git hooks and the interface dependencies
make dev            # the desktop app with a scripted mind and a scratch data folder
make check          # lint, drift checks and every test: the gate for every change
make help           # every target
```

`make check` does not need the embedded Lemonade and does not build a package. The package
configuration is opt-in: a plain `cargo build`, `make dev` and `make check` work on a checkout that
has never fetched `vendor/`.

To run the real engine from a checkout, fetch the pinned bundle and start the app without the
scripted mind:

```bash
make lemonade/fetch   # downloads the pinned embeddable bundle into vendor/lemonade/
cd src-tauri && NOCTURNE_DATA_DIR=<scratch> ../ui/node_modules/.bin/tauri dev
```

A debug build finds the bundle there. A release build finds it among its own resources, and
`NOCTURNE_LEMOND` (a folder, or the `lemond` file in it) overrides both. Every working folder of
the engine lives under the data folder, never beside the program, so a read-only install works.

## Linux packages

```bash
make package        # scripts/package.sh: AppImage, deb and rpm under target/package/
```

The script works in this order:

1. It fetches the pinned Lemonade archive for the platform into `target/package/download/`, checks
   its sha256 against `lemonade.lock`, unpacks it into `target/package/lemonade/<platform>/`
   (refusing an archive with more than one top-level folder or any symlink) and writes a sha256
   manifest of every file. It never trusts the stamp `scripts/fetch-lemonade.sh` leaves in
   `vendor/`.
2. It builds the interface and writes the third-party notices.
3. It runs the Tauri bundler from `src-tauri/` with `bundle/linux.json` merged over
   `tauri.conf.json`. The binary is built with the builder's home, Cargo and Rustup folders remapped
   so no host path is embedded.
4. For the AppImage it finishes the AppDir by hand: it removes modules the app does not use, copies
   the verified engine files back over whatever the AppImage tool rewrote, adds the licences of the
   bundled libraries and packs the AppImage again. It records the source package of every bundled
   library in `target/package/SOURCES.txt`, which ships beside the artefacts, not inside them.
5. It copies the artefacts to `target/package/` and writes `SHA256SUMS`.

The version has one source: the workspace `Cargo.toml`. `tauri.conf.json` carries no version, and
the script refuses to run if one ever differs.

`scripts/package.sh --dry-run` prints the merged configuration and the steps without building or
downloading. The Tauri command line tool fetches its packaging tools into its own cache the first
time; that is a developer-time download and never happens when Nocturne runs.

### What the packages carry

| Item | Where |
|---|---|
| The program | `usr/bin/nocturne` |
| The private engine | the resource folder `lemonade/`, next to the program (`/usr/lib/Nocturne/` in the deb and rpm) |
| Nocturne's licence and the third-party notices | `LICENSE` and `notices/` in the same resource folder |
| Bundled system libraries (AppImage only) | `notices/BUNDLED-LIBRARIES.txt`, with each library's licence and source package |

The deb and rpm depend on the web view, the toolkit, the tray library and the libraries the engine
links; the AppImage carries its own copies of the system libraries it needs.

## Proving a package

| Check | Command | What it proves |
|---|---|---|
| Contents | `scripts/check-package.sh <artefact>` | The engine is present, executable and byte for byte the verified bundle; licences and notices are shipped; the desktop entry and icons are right; no builder path is in any file, binaries included; the declared libraries are complete; an AppImage's bundled libraries pass the copyleft policy and appear in `SOURCES.txt` |
| Launch | `make smoke` | Each artefact starts under a virtual display with a scratch home: the engine comes up, the interface loads (not the placeholder page), a second launch with a file keeps it, SIGTERM quits cleanly in seconds, and no process of the package is left |
| Outbound traffic | `make netcheck`, `scripts/net-check.sh launch <variant>` | The app and its engine make no connection that is not on this computer, in a private network namespace under `strace` |
| Licences | `scripts/notices.py --check` | Every crate and interface dependency has an allowed licence |
| Scripts | `make scripts/test` | The unit tests for the above, part of `make check` |

The outbound check traces every network call and every local socket. It fails on any internet
address that is not loopback, any name lookup (port 53, the resolver, Avahi and nscd sockets), any
local socket that is not the scratch folder, the display or the private session bus, any bind to a
non-loopback address, and `io_uring` (which could send without a traced call). It also fails unless
the app really worked inside the namespace: the interface was shown, the engine reported ready and
a traced connection to its port exists, a second launch kept a file and the app quit cleanly.

Modes of `scripts/net-check.sh`:

- `launch`: a fresh data folder with no setup, idle for 120 seconds, then a second launch.
- `prepared`: `NET_CHECK_DIR` names a data folder where setup finished. The script works on a
  scratch copy of its `store/`, links `engine/models` in place, keeps a note and an image so the
  mind loads and reads, and idles for five minutes. Never point it at a real person's data.
- `attach`: wraps a manual session. Pass the display by socket path, because the usual abstract
  X11 socket does not cross the namespace: `NET_CHECK_WAYLAND` (an absolute path),
  `NET_CHECK_PULSE` and `NET_CHECK_DISPLAY`. Press Ctrl+C to end it and print the report.

The unit tests for the traffic judge use recorded `strace` lines, so a change to the parser is
tested without running the app.

## Windows

The Windows installers (NSIS and MSI) are built on a Windows machine only, and v1 does not publish
them. `src-tauri/bundle/windows.json` is the configuration.

Prerequisites: Windows 10 or 11 x86_64, Git for Windows (the scripts run under Git Bash), Rust with
the MSVC toolchain and the Visual Studio Build Tools "Desktop development with C++" workload,
Node.js 22 or newer, and the WebView2 runtime to run the result (it ships with current Windows 11
and Edge). The Tauri tool downloads NSIS and WiX into its own cache the first time.

From Git Bash at the repository root:

```bash
scripts/package.sh --config windows
```

This fetches the Windows Lemonade archive, verifies it, writes the manifest, builds the interface
and the notices, and runs the bundler with `bundle/windows.json`. The installers and a
`SHA256SUMS` file land in `target/package/windows/`. `scripts/package.sh --dry-run --config
windows` prints the merged configuration on any machine.

What the installers do:

- NSIS installs per user, so it needs no administrator rights. The MSI is for managed fleets.
- WebView2 uses an embedded bootstrapper: it fetches the runtime only when it is missing, at
  install time, with the person present. When WebView2 is already there, nothing is downloaded.
- The private engine is installed as the resource folder `lemonade/` next to the program.
- The installers are unsigned in v1. Windows SmartScreen warns on first run ("More info", then
  "Run anyway"). Signing is deferred.

Behaviour to know on Windows:

- Notifications work only from the installed app, because Windows ties them to its registered
  identity. A `make dev` run shows none.
- The app does not listen for Windows resume events in v1. After sleep, the scheduler notices the
  wall-clock jump within one 30 second tick and carries on.
- The default quick keep shortcut is Ctrl+Shift+Space.
