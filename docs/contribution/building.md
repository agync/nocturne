---
type: Guide
title: Building and packaging
description: The development loop, the real preview, the Linux packages, the checks that prove them, and the Windows and macOS builds.
status: draft
tags: [contribution, build, packaging, windows, macos]
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

## Real preview in a browser

For development only, and never shipped. `make preview-real` runs the real app (the private engine,
the real models, every command) behind a small HTTP server, so you can use the real product from a
browser, a phone on your own network included. The desktop shell is not involved: the server builds
the app on Tauri's mock runtime, the way the real-model end-to-end test does, and sends what the
browser asks through the same per-window allowlist as the main window.

```bash
PREVIEW_DATA_DIR=<scratch> PREVIEW_MODELS_FROM=<engine folder of a finished setup> make preview-real
```

It prints one link, `http://127.0.0.1:55066/#token=...`. The token is made at every launch and read
by the page from the fragment, so it never travels in a URL. The server binds loopback only; to
reach it from elsewhere, put a TLS proxy in front and name its address with
`PREVIEW_PUBLIC_ORIGIN`, the only other origin it accepts. Everything that carries data needs the
token as a bearer header. Two things cannot send one, so they take a short ticket from
`POST /api/ticket`: the event stream (one use, 30 seconds) and the pictures and audio of kept
originals, which are fetched by item id, never by path.

| Setting | Meaning |
|---|---|
| `PREVIEW_DATA_DIR` | Its own data folder (default `target/preview-real`). It must be empty or one the tool made. |
| `PREVIEW_PORT` | The loopback port (default 55066). |
| `PREVIEW_PUBLIC_ORIGIN` | Where other machines reach it. |
| `PREVIEW_MODELS_FROM` | An engine folder from a finished setup. Its models are hard-linked in, so nothing downloads; without it the first run offers the usual setup. |

What a browser cannot do is reported as unavailable: folder and file dialogs, opening a folder or a
file in the system's app, the logs folder, printing, sound settings and exporting to a folder.
Files are chosen in the page and travel as bytes.

The feature is `preview` on `nocturne-app` and sits in no default feature set. `make check` tests it
(`cargo test -p nocturne-app --features preview`), and the package check fails if a package holds
`nocturne-preview`, the text the server prints or the preview interface.

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

## macOS

The Mac package (an app and a disk image, Apple Silicon only) is built on a Mac and v1 does not
publish it. `src-tauri/bundle/macos.json` is the configuration. The Mac needs no Node: the app is
assembled by `scripts/macos-app.sh` with the tools macOS ships (PlistBuddy, plutil, codesign,
hdiutil), not by the Tauri command line tool.

Prerequisites: an Apple Silicon Mac on macOS 26 or newer (the pinned engine will not run on
older), the Command Line Tools, and Rust stable. `python3` comes with the Command Line Tools.

The interface and the notices are built where Node is. On any machine with Node:

```bash
scripts/package.sh --config macos --inputs-only    # ui/dist and target/package/notices
```

Copy `ui/dist` and `target/package/notices` to the same places in the Mac's checkout (`rsync`
does it), then on the Mac:

```bash
scripts/package.sh --config macos                  # target/package/Nocturne_<version>_aarch64.dmg
```

With Node on the Mac the script builds both inputs itself. Either way it fetches the pinned engine
for `macos-aarch64`, checks its sha256 against `lemonade.lock`, writes the manifest, builds the
app binary (`cargo build --release -p nocturne-app --features custom-protocol`, with the builder's
folders remapped as on Linux) and calls `scripts/macos-app.sh`, which writes `Nocturne.app` and the
disk image under `target/package/macos/`. The property list comes from `src-tauri/bundle/macos.json`
(minimum system, which follows the engine's own minimum), `tauri.conf.json` (identifier) and
`src-tauri/Info.plist` (the microphone sentence: "Nocturne listens only while you hold to talk.").
`src-tauri/icons/icon.icns` is made from `icon.png` by `scripts/macos-icon.sh`. The engine files
are copied byte for byte and signing leaves them alone. Someone with Node on a Mac can use the
Tauri tool instead (`tauri build --config bundle/macos.json`) with the same configuration.

Prove it with `scripts/check-package.sh target/package/macos/Nocturne.app` (or the `.dmg`, which it
mounts read only): the engine matches the manifest, the property list is right, no builder path
is in any file and the signature verifies. `scripts/smoke-launch-macos.sh` opens the app on the
logged-in user's screen with a scratch data folder and judges the log, a second launch, a quit
by `SIGTERM`, leftover processes and that `~/Library/LaunchAgents` was not touched. It cannot
run from a session with no one logged in at the Mac.

### Signing and notarisation

The app is signed ad hoc (`codesign --sign -`, without `--deep`, so the engine's bytes are not
touched). That runs on the Mac that built it. A copy that is downloaded or sent carries a
quarantine mark, and Gatekeeper blocks it unless it is opened with right-click then Open (or Open
Anyway in System Settings > Privacy & Security), or the mark is removed. Handing it to anyone
properly needs, as a decision for the owner:

- an Apple Developer ID Application certificate (the paid Apple Developer Program);
- signing the app and the engine's executables inside `Contents/Resources/lemonade/` with that
  identity, `--options runtime` and `--timestamp` (the microphone entitlement is in
  `src-tauri/macos/Entitlements.plist`), which `scripts/macos-app.sh --sign` does not do yet;
- notarisation with `xcrun notarytool` and stapling the ticket to the disk image.

Re-signing the engine changes its bytes, so it no longer matches the manifest. The order is
therefore fixed: `scripts/macos-app.sh --manifest` checks the copied engine against the manifest
before it signs, and the signature is verified after. For a Developer ID build, check the signed
result with `scripts/check-package.sh --no-manifest`, which still verifies the signature.

### Differences to know on a Mac

- Tests need more open files than the default of 256: run `ulimit -n 4096` first, or the parallel
  integration tests fail with "Too many open files".
- The quick keep shortcut default `CommandOrControl+Shift+Space` is Command, Shift and Space.
- Power comes from `pmset`, idle time from `ioreg` and sleep is held with a `caffeinate` child
  process. Resume has no listener; the scheduler's wall-clock check notices a wake, as on Windows.
- "Start with this computer" writes `~/Library/LaunchAgents/app.nocturne.desktop.plist` itself, with
  the program's canonical path escaped, through a temporary file set to mode 0644 and renamed
  into place. Turning it off or deleting everything removes it, even when the program's own path
  cannot be resolved. Run from the mounted disk image, or from the temporary place macOS uses for
  a downloaded app (App Translocation), turning it on is refused with "Move Nocturne to the
  Applications folder first, then turn this on.", because that path will not be there at the next
  login.
- The web view keeps its own data under `~/Library/WebKit`, `~/Library/Caches` and
  `~/Library/Saved Application State` whatever `NOCTURNE_DATA_DIR` says. "Delete everything" removes
  those too (see [Your data](../guide/your-data.md)). The smoke script sets `CFFIXED_USER_HOME`, which
  Foundation reads before the account's home, so a smoke run keeps all of it in its scratch home.
