---
type: Guide
title: Building and packaging
description: The development loop, the continuous integration jobs, the real preview, the Linux packages, the checks that prove them, and the Windows and macOS builds.
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

`make check` also vets and tests the Most private helper in `helpers/tailnet` (`make tailnet/test`),
once as a plain build and once with the release build tags. It needs the Go version pinned in
`helpers/tailnet/go.mod` and never downloads another; point `GO` at that toolchain's `go` when the
one on `PATH` differs (`make check GO=<path>/go`). Without that version (no Go, or another one) the
helper's tests are skipped with a one-line warning, and the rest of the check runs. With `CI` set,
or `NOCTURNE_REQUIRE_TAILNET_TESTS=1`, a missing toolchain fails the check instead. The first run
fetches the helper's Go modules, checked against `go.sum`; the tests themselves run offline against
Tailscale's in-process control server.

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

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request and every push to `main`, on Ubuntu 24.04
(x86_64), with the tests also on Linux arm64, macOS and Windows, a read-only token and no secret.
`make check` is lint plus test, so the workflow runs its targets as parallel jobs and together
they cover all of it. On a pull request, each job runs only when a path it reads changed:

| Job | Runs | Covers | Runs when these change |
|---|---|---|---|
| `rust` | `make rust/lint rust/test contract/check`, `scripts/away-integration.sh interop` | rustfmt, clippy for both feature sets, both test runs, the release build's compile check, the bindings drift check, the Rust and TypeScript phone sides against each other | the Rust side (`crates/`, `src-tauri/`, `.cargo/`, `clippy.toml`, `Cargo.*`, both lock files), `ui/`, `scripts/bindings.sh`, `scripts/away-integration.sh` |
| `ui` | `make ui/lint ui/test`, `npm run build`, `scripts/check_ui_dist.py` | Biome, the type check, the interface tests, the interface the app ships and its check | `ui/`, `src-tauri/` |
| `packaging scripts` | `make scripts/test` | the packaging scripts' tests | `scripts/`, the Rust side, `flatpak/`, `docs/contribution/flatpak.md`, `LICENSE`, `ui/package*.json` |
| `tailnet helper` | `make tailnet/test` with `NOCTURNE_REQUIRE_TAILNET_TESTS=1` | vet and tests, plain and with the release tags, on the Go version in `helpers/tailnet/go.mod` | `helpers/`, `scripts/test-tailnet.sh`, `scripts/tailnet-tags.sh` |
| `docs and hooks` | `pre-commit run --all-files` | every hook, the OKF bundle among them | always |
| `browser tests` | `make ui/e2e` | the Playwright tests against the mock engine, in Chromium | `ui/`, `src-tauri/` |
| `site` | `make site`, then `make -C site check` | the site build and its axe and layout checks | `site/`, `docs/`, `lemonade.lock`, `ui/package*.json` |
| `rust tests` | `make rust/test` on `ubuntu-24.04-arm`; `cargo test --workspace --no-fail-fast` on `macos-15` and `windows-2022` | the Rust tests on Linux arm64, macOS arm64 and Windows x64 | the Rust side |
| `rust lint` | `cargo clippy --workspace --all-targets -- -D warnings` on `macos-15` and `windows-2022` | the code compiled only for macOS or Windows, which the Linux lint never builds | the Rust side |
| `ui tests` | `npm test` in `ui/` on `macos-15` and `windows-2022` | the interface tests on macOS and Windows | `ui/` |
| `gate` | | fails when any job above failed or was cancelled; the one job to require | always |

The `changed paths` job compares a pull request with its merge base, and its run summary says
which jobs it chose. The table of areas is `scripts/ci-areas.sh`; `scripts/test_ci_areas.py`
checks one changed path for each of its patterns, that each job's `if:` reads the flag of its
area and it needs only `changes`, that none continues on error, and that `gate` needs every job.
A change to a workflow, the `Makefile`, `rust-toolchain.toml` or that script, a push to `main`
and a hand run run every job. Without a merge queue, the run on `main` is the first test of
the merged code and the first build of the code compiled only for a release. The areas include what one side's tests read from the other: the Rust tests read files in
`ui/` and write the bindings there, and the interface's tests read files in `src-tauri/`, so on
x86_64 Linux both sides run when either changes, while the other platforms run only their own
side. A job skipped this way counts as passed in `gate`. Every run on `main` finishes, and none is
dropped while queued.

The `browser tests` and `site` jobs go beyond `make check`, and so do the platform jobs.
Rustfmt, the clippy pass with the `server` feature, the bindings drift check, the browser tests
and the Go tests run on x86_64 Linux only. A test that
needs a piece only Linux has (`/proc`, `prctl`, the PCI device folders) is compiled out or ignored
elsewhere with its reason, so the other platforms skip it rather than fail; everything the app does
on a platform is tested there. The jobs are split because one runner would run the Rust build, the
browser tests and the Go tests one after another, and the split gives each its own cache and its
own verdict. Each Rust job restores the registry and its dependencies' build output with
`Swatinem/rust-cache`, one cache per job and platform that only `main` saves. The workspace's own
crates are left out of it, which keeps every cache small enough for the repository's 10 GB.

CI does not run what needs more than a hosted runner has, and the run summary of the `gate` job
says so each time:

- `make e2e-real`, `e2e-online` and `e2e-models` download the real models, about 4.8 GB or more.
- `make smoke` and `make netcheck*` need a virtual display and a network namespace and run on a
  developer's machine.
- The Linux arm64 package is built by the release workflow, on its own runner.

The Linux x86_64, macOS and Windows packages are built by CI too, unsigned and uploaded nowhere,
so that a packaging bug fails a pull request and not a release. The `package-*` jobs run on every
push to `main`, and on a pull request that touches `scripts/`, `src-tauri/bundle/`,
`src-tauri/macos/`, `src-tauri/icons/`, a `src-tauri/tauri*.json` file, `src-tauri/Info.plist`,
`src-tauri/Cargo.toml`, `src-tauri/build.rs`, `Cargo.toml`, `Cargo.lock`, `LICENSE`,
`ui/package*.json`, `lemonade.lock`, `helpers.lock` or `release.yml` (the Linux job also runs for
`flatpak/`). A change to the Rust sources does not build a package. The release build differs from
every other build in CI: some of the app's code is compiled only for a release
(`cfg(not(debug_assertions))`), and the tests turn on the crates' stand-in features (`fake`,
`fake-server`), which a release leaves off. So the `rust` and `rust tests` jobs also run
`cargo check --release -p nocturne-app --features custom-protocol`, which compiles all of it
without generating code.
The `ui` job builds the interface the packages ship and checks it as `scripts/package.sh` does. A
release builds every package from its tag in `release.yml`, whatever CI ran.

The workflow installs `okf` at a version and checksum written in `ci.yml`, and pins every action
by commit. Dependabot keeps the actions, the Cargo crates, the interface packages and the helper's
Go modules current (`.github/dependabot.yml`); what it cannot see is in
[Pin bumps](pin-bumps.md).

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
originals, which are fetched by item id, never by path. A media ticket lasts an hour from its last
use. The data folder and the home folder never appear in what the browser reads: such paths in
answers, errors, settings and events are replaced.

Uploads other than recordings and files are limited to 1 MiB, two recordings or files are read at
once, and a recording is converted to WAV with the `ffmpeg` found on the path at start, limited to
known recording formats, local files, an hour and 512 MiB.

When a Tailscale or other proxy shares the machine, give the preview a port of its own
(`tailscale serve --https=8443 http://127.0.0.1:55066`), since an origin is shared by every
service behind the same host name and port, and set `PREVIEW_PUBLIC_ORIGIN` to exactly that.

| Setting | Meaning |
|---|---|
| `PREVIEW_DATA_DIR` | Its own data folder (default `target/preview-real`). It must be empty or one the tool made. |
| `PREVIEW_PORT` | The loopback port (default 55066). |
| `PREVIEW_PUBLIC_ORIGIN` | Where other machines reach it. |
| `PREVIEW_MODELS_FROM` | An engine folder from a finished setup. Its models are copied in (a reflink copy where the filesystem allows, a full copy elsewhere, never a hard link, so nothing the preview writes can alter the source); nothing downloads; without it the first run offers the usual setup. |

What a browser cannot do is reported as unavailable: folder and file dialogs, opening a folder or a
file in the system's app, the logs folder, printing, sound settings and exporting to a folder.
Away from home is unavailable too. Files are chosen in the page and travel as bytes. Online answers
and choosing models work as in the window: the key travels only in the request body to the preview,
is kept in the preview folder's owner-only key file and never in the system keychain, and turning
on is printed on the preview's terminal instead of asked in a window of the system's own.

The feature is `preview` on `nocturne-app` and sits in no default feature set. `make check` tests it
(`cargo test -p nocturne-app --features preview`), and the package check fails if a package holds
`nocturne-preview`, the text the server prints or the preview interface.

## The server image

The experimental server ([0012](/adr/0012-server-mode.md)) is the preview's HTTP front with a sign-in
token kept in the data folder, built `--release` and shipped only as a container image. The
`Containerfile` at the root has four stages: the interface (`vite build --mode server`, into
`ui/dist-server`), the server (`cargo build --release --features server --bin nocturne-server`),
the engine (`scripts/fetch-lemonade.sh`, checked against `lemonade.lock`), and an Ubuntu 26.04
runtime with only the libraries the server and the engine load, `ffmpeg`, a `nocturne` user (uid
10001) and the `/data` volume. The runtime is Ubuntu because the engine's Linux build needs its C
library (glibc 2.43); Debian 13 has an older one. Every base image is pinned by digest; bump the digest and its tag comment together.

```bash
make server/build                      # the server and ui/dist-server, without a container
make server/image                      # podman build, tagged localhost/nocturne:dev
make server/image IMAGE=localhost/nocturne:try
```

`make server/image` builds with `--format docker`, since the OCI format drops the `HEALTHCHECK`, and
with `--jobs 1`. Two build arguments change it: `CARGO_BUILD_JOBS` limits the Rust build on a shared
machine, and `EXTRA_PACKAGES` adds runtime packages, such as `mesa-vulkan-drivers` for the
graphics card variant the install guide describes. Only what `.containerignore` lets through reaches the build. Cargo and npm caches
are build cache mounts, so they stay out of the layers. Run it as the
[install guide](/guide/install.md#run-it-as-a-server) says.

`.github/workflows/container.yml` builds the image natively for amd64 and arm64 on every pull request
that touches what goes into it, without logging in or pushing. A release tag (`vX.Y.Z`) builds both,
pushes each as `<tag>-<arch>` to `ghcr.io/agync/nocturne` (the `IMAGE_NAME` repository variable
overrides it) with the built-in token, and pushes a manifest list as `<tag>` and `latest`. The
actions are Red Hat's buildah, podman login and push actions, pinned by commit.

## Linux packages

```bash
make package        # scripts/package.sh: AppImage, deb and rpm under target/package/
```

The packages are for the architecture of the machine that builds them: x86_64, or aarch64 (64-bit
Arm). There is no cross build. The file names say which: `_amd64` or `_aarch64` for the AppImage,
`_amd64` or `_arm64` for the deb, `.x86_64` or `.aarch64` for the rpm, and
`scripts/check-package.sh` picks the engine manifest to compare against by that name.

The script works in this order:

1. It fetches the pinned Lemonade archive for the platform into `target/package/download/`, checks
   its sha256 against `lemonade.lock`, unpacks it into `target/package/lemonade/<platform>/`
   (refusing an archive with more than one top-level folder or any symlink) and writes a sha256
   manifest of every file. It never trusts the stamp `scripts/fetch-lemonade.sh` leaves in
   `vendor/`.
2. It builds the interface and writes the third-party notices.
3. It runs the Tauri bundler from `src-tauri/` with `bundle/linux.json` merged over
   `tauri.conf.json`, and on aarch64 `bundle/linux-aarch64.json` after it, which points the
   resources at that engine. The binary is built with the builder's home, Cargo and Rustup
   folders remapped so no host path is embedded.
4. For the AppImage it finishes the AppDir by hand: it removes modules the app does not use, copies
   the verified engine files back over whatever the AppImage tool rewrote, adds the licences of the
   bundled libraries and packs the AppImage again. It records the source package of every bundled
   library in `target/package/SOURCES.txt`, which ships beside the artefacts, not inside them.
   The licences come from the build computer's package database, whichever of rpm and dpkg knows
   its C library: on Fedora each package's licence tag and licence files, on Debian and Ubuntu
   every licence the package's machine-readable copyright file names, with that file and the
   common licence texts it refers to. A package whose copyright file does not name its licences
   in a form that parses takes a reviewed licence from `DEBIAN_LICENCES` in
   `scripts/bundled_libs.py`, and `BUNDLED-LIBRARIES.txt` says so; a package in neither fails the
   step. `SOURCES.txt` names the source rpm, or the Debian source package and version
   (`glib2.0_2.80.0-6ubuntu3.10`). The step prints one line per library with the package that
   owns it. It fails at once when neither database knows the C library, and after the list,
   naming them, when no package owns some of the libraries.
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

The Flatpak is built separately, with `flatpak-builder` and AetherPak: see [The Flatpak](flatpak.md).

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

### Away from home integration runs

`scripts/away-integration.sh [interop|e2e|real|netcheck|all]` runs the Away from home checks that
need the computer's side built from Rust. They use the stand-in helpers, so nothing reaches a
network beyond loopback, no real helper starts and none touches a person's own data. Run it under
the machine's resource cap.

- `interop`: the Rust test phone and the TypeScript phone client against each other's real
  implementation (vitest).
- `e2e`: the built phone app in Chromium at 390 by 844 against the real app (Playwright), through a
  scanned and a typed code, and the queue after the computer goes away.
- `real`: Linux only. The real desktop app under a virtual display turns Away from home on, pairs,
  uploads 50 MB, turns it off and quits, then checks that nothing is left and records the memory and
  timer budgets.
- `netcheck`: `make netcheck` on the release binary, then `make netcheck-remote` on a release
  binary built with the stand-in helpers. It needs the fetched engine in `vendor/lemonade`.
- `all`, the default: `interop` and `e2e`.

None of this proves behaviour on the real internet or with the real helpers; that waits for the
owner-approved trials.

## Windows

The Windows installers (NSIS and MSI) are built on a Windows machine only: your own, or the
release workflow's (see [Releasing](release.md)). `src-tauri/bundle/windows.json` is the
configuration.

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
- A local build is unsigned, and so is a release build until the code signing secrets are set.
  Windows SmartScreen warns on first run of an unsigned one ("More info", then "Run anyway").

Behaviour to know on Windows:

- Notifications work only from the installed app, because Windows ties them to its registered
  identity. A `make dev` run shows none.
- The app does not listen for Windows resume events in v1. After sleep, the scheduler notices the
  wall-clock jump within one 30 second tick and carries on.
- The default quick keep shortcut is Ctrl+Shift+Space, or Cmd+Shift+Space on a Mac.

## macOS

The Mac package (an app and a disk image, Apple Silicon only) is built on a Mac: your own, or the
release workflow's (see [Releasing](release.md)). `src-tauri/bundle/macos.json` is the configuration. The Mac needs no Node: the app is
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
logged-in user's screen with a scratch data folder and judges the log (which includes the
line `recording: available` or `recording: unavailable (...)` the interface writes at start, so
whether the web view can record is known without a person), a second launch, a quit
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
  `src-tauri/macos/Entitlements.plist`), which `scripts/macos-app.sh --sign` does not do yet and
  the release workflow does when its signing secrets are set;
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
