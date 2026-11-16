---
type: Guide
title: The Flatpak
description: How the Flatpak is built, what it may reach on the computer and why, and how AetherPak publishes it to flatpak.nocturne.is.
status: draft
tags: [contribution, build, packaging, flatpak]
---

# The Flatpak

The Flatpak is a fourth Linux package next to the AppImage, the deb and the rpm. Its app id is
`is.nocturne.Nocturne`, the reverse of the project's own domain. The desktop identifier
`app.nocturne.desktop` stays what it is everywhere else: it names the data folder and the
single-instance lock, and it cannot be a Flatpak id because an id may not end in `.desktop`.

## The files

| File | What it is |
|---|---|
| `flatpak/is.nocturne.Nocturne.yml` | The manifest: GNOME 51 runtime, the Rust and Node SDK extensions, three modules |
| `flatpak/is.nocturne.Nocturne.desktop` | The desktop entry |
| `flatpak/is.nocturne.Nocturne.metainfo.xml` | The AppStream description, with screenshots from nocturne.is |
| `flatpak/aetherpak.yaml` | Where AetherPak pushes the images, how it names the repository and how its page looks |
| `flatpak/site/index.html` | The repository page at `flatpak.nocturne.is` |
| `flatpak/shared-modules/` | The tray library and intltool modules, copied unchanged from `flathub/shared-modules` at commit `c91a8110` |
| `scripts/flatpak-sources.sh` | Writes the offline Cargo and npm sources into `flatpak/generated/` |
| `scripts/check_flatpak.py` | The drift check `make check` runs through its tests |
| `.github/workflows/flatpak.yml` | Builds with AetherPak and, when asked, publishes |

The manifest builds three modules:

1. `libdrm`, for `libdrm_amdgpu`, which the private engine links and the GNOME runtime does not
   carry. The version matches the runtime's own `libdrm`.
2. `libayatana-appindicator`, the tray library Tauri loads at run time.
3. `nocturne`: the interface (`npm ci --offline`, `npm run build`), the program
   (`cargo build --release --locked --offline --features custom-protocol`), the third-party
   notices, and the pinned engine. The engine archive is the one `lemonade.lock` names for the
   architecture being built (the manifest lists both, each with `only-arches`), checked by its
   sha256, and installed byte for byte into `/app/lib/Nocturne/lemonade/`: the module neither
   strips nor splits debug information. The program finds it there the same way the deb does,
   because Tauri's resource folder for `/app/bin/nocturne` is `/app/lib/Nocturne`.

Nothing is downloaded while the module builds. Cargo and npm read the sources
`scripts/flatpak-sources.sh` generates from `Cargo.lock` and `ui/package-lock.json` with the
`flatpak-builder-tools` generators at a pinned commit. Those files are generated for every build
and never committed, so they cannot fall behind the lockfiles. The script needs `uv` and the
network; it installs nothing outside uv's cache.

`scripts/check_flatpak.py` fails `make check` when the engine archives in the manifest differ
from `lemonade.lock`, when the newest release in the metainfo is not the workspace version, when
the app id differs between the four files, or when the finish-args differ from the table below.

## Build it locally

You need `flatpak`, `flatpak-builder`, `uv`, and the GNOME 51 runtime and SDK with the
`rust-stable` and `node24` extensions for the 26.08 SDK, installed for your user. GNOME 51 is the
first GNOME runtime whose C library is new enough for the pinned engine (it needs glibc 2.43;
GNOME 50 has 2.42).

```bash
flatpak install --user flathub org.gnome.Sdk//51 org.gnome.Platform//51 \
    org.freedesktop.Sdk.Extension.rust-stable//26.08 org.freedesktop.Sdk.Extension.node24//26.08
scripts/flatpak-sources.sh
cd flatpak
flatpak-builder --user --force-clean --state-dir=.flatpak-builder --repo=repo build is.nocturne.Nocturne.yml
flatpak build-bundle repo nocturne.flatpak is.nocturne.Nocturne
flatpak install --user ./nocturne.flatpak
```

Cargo uses the builder's job count (`FLATPAK_BUILDER_N_JOBS`), so on a shared machine set it
low and run the build under the usual resource cap. `appstreamcli validate --no-net` and
`desktop-file-validate` check the two metadata files.

## What the sandbox may reach

The app keeps everything inside its own folders: its data, its settings and the web view's cache
live under `~/.var/app/is.nocturne.Nocturne/`. It has no access to the home folder or any other
folder. Files arrive through the system's file chooser and drag and drop, which hand over only
what the person picked, through the documents portal. Opening links and folders goes through the
portal too.

| Permission | Why |
|---|---|
| `--share=ipc` | Shared memory for the X11 fallback; without it X11 drawing is slow or fails |
| `--socket=wayland` | Shows the window on a Wayland desktop |
| `--socket=fallback-x11` | Shows the window on an X11 desktop, and only there |
| `--device=all` | The engine's ROCm backend needs `/dev/kfd` on a discrete AMD GPU, which `--device=dri` does not expose; Vulkan needs `/dev/dri` |
| `--socket=pulseaudio` | Records voice notes and reads the page aloud |
| `--share=network` | Downloads the models the person agreed to, and carries Online answers and Away from home when they are on; the engine also listens on loopback |
| `--talk-name=org.freedesktop.Notifications` | The one note that the page is ready; the notification library talks to this service directly, not to the portal |
| `--talk-name=org.kde.StatusNotifierWatcher` | Registers the tray icon and asks whether the desktop shows one |
| `--own-name=app.nocturne.desktop.SingleInstance` | The single-instance lock: a second launch hands its file to the running window instead of starting another app |
| `--env=WEBKIT_DISABLE_DMABUF_RENDERER=1` | The web view's DMA-BUF renderer shows a blank window inside the sandbox on some graphics drivers |

### What works differently in the Flatpak

The app knows it is in a Flatpak from `/.flatpak-info`, which names its app id.

- **Start with this computer** asks the Background portal, which writes the desktop's start-up
  entry with `flatpak run`. The sandbox cannot reach the folder the desktop reads, so the app
  cannot write or check the entry itself: Settings shows the saved choice. The portal is asked
  when the switch changes, and at start until the desktop has answered, yes or no, when the
  switch was turned on by a version that wrote the entry inside the sandbox; `os-state.json`
  records that it has been told. A start that finds it was told but the switch is off asks the
  portal to remove the entry. Delete everything asks it to remove the entry only when it was
  told before, and keeps that record if the removal does not get through.
- **The quick keep command** Settings offers for a desktop shortcut is
  `flatpak run is.nocturne.Nocturne --quick-capture`, since `/app/bin/nocturne` exists only
  inside the sandbox.

### The tray icon

The tray library writes the icon to a PNG and passes its absolute path to the desktop's panel,
which runs outside the sandbox. Its default folder is under `XDG_RUNTIME_DIR`, a private overlay
inside the sandbox, so the panel found no file and drew a blank icon. The app now gives the
library a folder under its own cache folder (`tray/` in `app_cache_dir`). A Flatpak sets
`XDG_CACHE_HOME` to the real `~/.var/app/is.nocturne.Nocturne/cache`, the same path on both
sides, so no `--filesystem=xdg-run/tray-icon` permission is needed and the finish-args stay as
they are. The library reuses the folder for every later icon change, and the app clears stale
icon files from it at start. If the cache folder is unknown, cannot be created or has a relative
path or control characters in it, the library's default is used.

### What does not work in the Flatpak yet

- **Keeping the computer awake overnight, and knowing the person is away.** The sandbox has no
  access to logind, UPower or the idle monitors on the session bus. Unknown idle time already
  counts as away and mains power is read from `/sys/class/power_supply`, so the overnight tidy
  still runs, but it cannot stop the computer from sleeping during it. The Inhibit portal would
  cover sleep.
- **The keychain.** Without the Secret Service the Online answers key is kept in the owner-only
  file inside the app's data folder, which the app already falls back to.
- **Sound settings.** The button that opens the system's sound settings finds no program inside
  the sandbox and reports it as unavailable.
- **The quick keep shortcut on Wayland.** The app cannot take keys from inside the sandbox, so
  the person adds a desktop shortcut that runs the quick keep command.
- **An inbox folder** chosen through the portal is watched by the regular rescan once a minute,
  since change notifications do not cross the documents portal.

## Publishing with AetherPak

`.github/workflows/flatpak.yml` builds the Flatpak with
[AetherPak](https://github.com/aetherpak/actions) and, when asked, publishes it. AetherPak stores
the app as OCI images in GHCR (`ghcr.io/agync/nocturne-flatpak`) and writes a small static index,
a `nocturne.flatpakrepo`, a `.flatpakref` per channel and a landing page under
`https://flatpak.nocturne.is`. The remote is named `nocturne` and titled "Nocturne", so software
centres show it under its own name.

The landing page is `flatpak/site/index.html`, which `branding.index_template` in
`aetherpak.yaml` names. It is AetherPak's default template restyled as nocturne.is: AetherPak
fills in the channels, the install links, the commands and the key fingerprint when it builds the
site, so the page uses only the template fields the pinned CLI (v0.34.0) provides. The fonts and
icons load from `nocturne.is`, whose `_headers` lets this origin use the fonts. To preview it,
build the CLI at that tag into a scratch folder and run
`aetherpak preview --config flatpak/aetherpak.yaml` from the repository root. It serves the
template with made-up apps but without the logo, icon, colour and footer; for those and for
Nocturne's own channels, run `aetherpak build-site --config flatpak/aetherpak.yaml --landing-page
--allow-unsigned --no-sign` against a small records folder and serve the result.

The workflow runs by hand (`workflow_dispatch`) or from another workflow (`workflow_call`). It is
never started by a push.

| Input | Default | Meaning |
|---|---|---|
| `ref` | the triggering ref | The tag, branch or commit to build; resolved to one commit that every job builds |
| `tag` | none (called only) | The release tag `ref` came from, needed to publish to stable |
| `channel` | `stable` when called, `beta` by hand | The Flatpak branch |
| `publish` | `false` | Push the images and deploy the index; without it the build is checked and a `.flatpak` bundle is uploaded as a workflow artefact. Publishing to `stable` needs a `v*` release tag that points at the commit; any other publish goes to `beta` |

Its jobs:

1. `resolve` turns `ref` into one commit and refuses a stable publish that is not a release tag
   pointing at it. Every later job checks out that commit.
2. `sources` generates the offline sources and hands them on as an artefact.
3. `build` runs AetherPak's build (with `flatpak-builder-lint`) in the AetherPak builder
   container, which is privileged and pinned by digest, once for x86_64 on `ubuntu-24.04` and once
   for aarch64 natively on `ubuntu-24.04-arm`. The digest is a multi-architecture index, so the
   same pin runs on both. It sees no secret and cannot push. When publishing, it hands the
   unsigned repository on as an artefact and turns the build cache off.
   It still uses the AetherPak build action, whose own setup and cache actions are not pinned;
   that is acceptable only because nothing secret is reachable from this job.
4. `publish`, when publishing, runs in the `release-signing` environment without privileges. It
   fails if the key is empty, then signs the images and pushes them with the AetherPak
   `publish-oci` action, pinned to the same commit as the build action. It runs once per
   architecture, each writing its own record; both run on x86_64, because a push only copies the
   built repository into the registry.
5. `site` runs in `release-signing`. It merges the records of both architectures with the index
   live at `https://flatpak.nocturne.is`, signs it with the AetherPak `publish-site` action and
   hands the site on as an artefact.
6. `deploy` publishes that artefact to GitHub Pages.

A caller grants `contents: read`, `packages: write`, `pages: write` and `id-token: write`. The two
secrets, `FLATPAK_GPG_PRIVATE_KEY` and `FLATPAK_GPG_PASSPHRASE`, live in the `release-signing`
environment and are read only by `publish` and `site`; no caller passes them. Publishing without
a key fails, so the repository is never unsigned.

The `release-signing` environment must have required reviewers and hold the two secrets, and the
`v*` tag ruleset described in `release.md` must be in place: the workflow's tag checks assume that
only the release App can create or move a release tag.

Before the first publish, a maintainer:

- runs the workflow by hand once with channel `beta` and publish on, to prove signing, the push
  and the index against the real registry before a release depends on them;
- allows public packages for the organisation, then makes `nocturne-flatpak` public after the
  first push, so people can install without signing in to GitHub;
- sets up the domain once: in Cloudflare DNS adds a `flatpak` CNAME record to `agync.github.io`,
  set the zone's SSL/TLS mode to Full (strict) or add a configuration rule for that host, and in
  the repository's Pages settings sets the custom domain `flatpak.nocturne.is` and turns on
  Enforce HTTPS. Leave the record DNS only until GitHub has issued its certificate for the
  domain, then switch it to proxied. The `site` job writes a `CNAME` file into the site so each
  deployment keeps the domain. The landing site at `nocturne.is` stays on Cloudflare Pages;
- publishes the three screenshots the metainfo names under `https://nocturne.is/screenshots/`.

> [!NOTE]
> Owner decisions needed: the signing key and the GHCR package name.
> TODO(owner).
