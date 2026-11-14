---
type: Guide
title: Releasing
description: How release-please cuts a release, what the release workflow builds for each platform, the secrets it needs, and the checklist a human runs before merging the release pull request.
status: draft
tags: [contribution, release, checklist, ci]
---

# Releasing

Releases are cut by release-please and built by the release workflow
(`.github/workflows/release.yml`). A human still runs the checklist below against the release pull
request and approves it; merging that pull request is the decision to publish.

## How a release is cut

1. Every push to `main` runs release-please. It reads the Conventional Commits since the last
   release and opens or updates one release pull request. That pull request moves the version in
   the workspace `Cargo.toml` and `ui/package.json`, brings `Cargo.lock` and `ui/package-lock.json`
   to it, and adds the release notes to `CHANGELOG.md`. Before 1.0 a `feat` moves the minor version
   and anything else the patch version.
2. A human runs the checklist below on the release pull request's branch, then merges it.
3. release-please tags the merge (`v<version>`) and creates the release page with the notes.
4. The same run builds the packages, checks them and attaches them to the release with one
   `SHA256SUMS`. It also calls `tailnet.yml` for the Most private helper and, for a release only,
   `flatpak.yml` for the Flatpak, which publishes to its own repository.

The configuration is `release-please-config.json` (release type `simple`, the version files it
moves and the changelog path) and `.release-please-manifest.json` (the current version, which only
release-please changes). The version keeps one source for the app: the workspace `Cargo.toml`.
`tauri.conf.json` carries none, and the packaging script refuses to run if it ever does.

The release pull request and the tags are made by a GitHub App, not by the workflow's own token,
so that its events start the other workflows. The release workflow can also be run by hand. With
`tag` set to an existing release tag (`vX.Y.Z`, with a release page) it builds that tag, signs when
signing is on and attaches the files to its release; any other value fails. With `tag` empty it
builds the chosen branch unsigned, runs no signing job and the helper workflow not at all, and
keeps the files as workflow artifacts. A file already on a release is never replaced, so a failed
upload is fixed by deleting the partial files by hand first.

The tag is resolved to its commit once, at the start of the run. Every build, the helper workflow
and the Flatpak workflow check out that commit, never the tag's name, and the upload stops if the
tag has moved since. The owner should also add a repository ruleset that protects `v*` tags and
`v*` branches: only the release App may create them, and nobody may move or delete them.

## What the workflow builds

| Platform | Runner | How | Files | Checked with |
|---|---|---|---|---|
| Linux x86_64 | Ubuntu 24.04 | `scripts/package.sh` | AppImage, deb, rpm, `SOURCES.txt` | `scripts/check-package.sh` on each |
| macOS Apple Silicon | macOS 15 | `scripts/package.sh --config macos` | `.dmg` | `scripts/check-package.sh` on the ad hoc image (engine manifest); after a Developer ID signature, with `--no-manifest` and `spctl` in a job of its own |
| Windows x86_64 | Windows Server 2022 | `scripts/package.sh --config windows` under Git Bash | NSIS `.exe`, `.msi` | the sums, and `signtool verify` when signed (`scripts/check-package.sh` does not read installers) |
| Most private helper | `tailnet.yml` | `scripts/build-tailnet.sh`, built twice | four binaries and the licence report | the two builds agree byte for byte |
| Flatpak | `flatpak.yml` | AetherPak | the Flatpak repository | AetherPak's own checks |

Each build fetches the engine pinned for its platform in `lemonade.lock` and checks its sha256, as
the local build does. There is no Intel Mac build, because `lemonade.lock` pins no Intel Mac engine.
The helper is never inside a package; it is attached beside them.

No build job sees a signing secret. The builds upload unsigned files (the Mac app signed ad hoc).
For a release tag, `sign-macos` and `sign-windows` run in the `release-signing` environment: they
download those files, check out nothing and run no build, sign them (and notarise and staple the
Mac image), and upload the signed copies, which the release then carries instead. A separate job
checks the signed Mac image again. The Windows installers are signed, the program inside them is
not, because signing it would mean building in the signing job.

Set the repository variable `NOCTURNE_RELEASE_STRICT` to `true` to run the package checks with
`NOCTURNE_RELEASE=1`, which refuses a placeholder helper pin or one older than 183 days. Leave it
unset until the Most private helper is pinned.

## Secrets

Add these in the repository settings. The signing secrets belong in the protected
`release-signing` environment, which needs a maintainer's approval for each run. Signing is turned
on by repository variables, not by the presence of a secret: `MACOS_SIGNING` and `WINDOWS_SIGNING`
set to `true`. When one is on, its signing job must succeed or nothing is attached.

| Secret | Where | Needed for | Without it |
|---|---|---|---|
| `RELEASE_APP_ID` | repository | the release bot: the GitHub App's id | release-please and the upload fail |
| `RELEASE_APP_PRIVATE_KEY` | repository | the release bot: the App's private key | release-please and the upload fail |
| `MACOS_DEVELOPER_ID_CERTIFICATE` | `release-signing` | the Developer ID Application certificate, a base64 `.p12` | with `MACOS_SIGNING` on, the signing job fails |
| `MACOS_DEVELOPER_ID_PASSWORD` | `release-signing` | the `.p12` password | as above |
| `MACOS_DEVELOPER_ID_IDENTITY` | `release-signing` | the identity name, `Developer ID Application: <name> (<team>)` | as above |
| `MACOS_NOTARY_APPLE_ID` | `release-signing` | the Apple ID that notarises | as above |
| `MACOS_NOTARY_TEAM_ID` | `release-signing` | the team id | as above |
| `MACOS_NOTARY_APP_PASSWORD` | `release-signing` | an app-specific password for that Apple ID | as above |
| `WINDOWS_CODE_SIGNING_CERTIFICATE` | `release-signing` | the code signing certificate, a base64 `.pfx` | with `WINDOWS_SIGNING` on, the signing job fails |
| `WINDOWS_CODE_SIGNING_PASSWORD` | `release-signing` | the `.pfx` password | as above |

The GitHub App needs read and write access to contents (branches, tags, releases and their files)
and to pull requests (the release pull request and its labels) on this repository. The helper
workflow uses the same signing secrets, and its macOS and Windows jobs fail without them; its
licence job also fails until `go_licenses_version` in `helpers.lock` is pinned.

## Checking a release

From a machine with the GitHub command line tool:

```bash
gh release download v<version> --dir <scratch>
cd <scratch> && sha256sum -c SHA256SUMS
scripts/check-package.sh --no-manifest <scratch>/Nocturne_<version>_amd64.AppImage
```

Do the same for the deb and the rpm. To compare the engine byte for byte as well, drop
`--no-manifest` after a `make package` of the same tag, which writes the manifest.

On a Mac, `scripts/check-package.sh --no-manifest` on the `.dmg` verifies the signature, and
`spctl --assess --type open --context context:primary-signature -v` on it shows whether Gatekeeper
accepts it (only a notarised image passes). On Windows, the file properties show the signature on
a signed installer. `make smoke` and the desktop checks below then run on the downloaded files.

## Checklist

Run it on the release pull request's branch, before merging it. Keep the real output of each step
with the release notes.

1. **Start clean.** The branch is the release pull request's latest state, the working tree is
   clean, and `make check` passes.
2. **Check the version.** The release pull request moved the workspace `Cargo.toml`, `Cargo.lock`
   and the interface's package files to the same version, and `CHANGELOG.md` names it. Never edit
   the version by hand.
3. **Check the engine pin.** `lemonade.lock` is unchanged, or the engine's runtime tests were run
   again against the new pin.
4. **Build.** `make package` produces the AppImage, the deb, the rpm, `SHA256SUMS` and
   `SOURCES.txt` under `target/package/`.
5. **Check each package.** `scripts/check-package.sh target/package/<artefact>` for the AppImage,
   the deb and the rpm. Every check passes, including the manifest comparison of the engine.
6. **Smoke test.** `make smoke` passes for the binary and all three packages.
7. **Prove it stays local.** `scripts/net-check.sh launch <variant>` for `binary`, `appimage`, `deb`
   and `rpm`, then `scripts/net-check.sh prepared appimage` with `NET_CHECK_DIR` set to a data
   folder where setup finished. Record the output in the release notes: it is the release-time proof
   behind the claim that Nocturne goes online only to download what the person agrees to.
   For Away from home also run `make netcheck-remote` (see Away from home checks below).
8. **Check the licences.** `scripts/notices.py --check` passes. The copyleft policy for the
   AppImage's bundled libraries is enforced by step 5.
9. **Check the desktop** (below) on a GNOME Wayland session and on one X11 session.
10. **Skeptical review.** Review the whole range since the last release as a maintainer who wants
    to find a reason not to ship it.
11. **Record `SHA256SUMS`** and `SOURCES.txt` next to the release notes.
12. **Approval and merge.** A human approves and merges the release pull request. The workflow
    tags it and builds and attaches the files; check them as in Checking a release above.

> [!NOTE]
> Owner decision needed: the wording of the written offer of corresponding source for the LGPL
> libraries inside the AppImage. `SOURCES.txt` names the source package of each one; the offer that
> accompanies it (where and for how long) is not written yet. TODO(owner).

## Away from home checks

Away from home adds helpers that are never part of a package. These checks use fake helpers; none
touches the real internet.

- **Net check modes.** The existing modes also fail on any listen or bind by the app, any run of a
  helper, and any file under the data folder's helper directory when no download was agreed to
  (off means off). The `remote` mode (`make netcheck-remote`) runs a build with fake helpers and a
  scripted phone with Away from home on. Non-loopback attempts and the any-interface UDP bind pass
  only for helper processes, never for the app or the engine. Calls are mapped to processes and
  processes to executables.
- **Package check.** No helper binary and no fake marker in a package; the embedded `helpers.lock`
  equals the repository file; every pinned version has help fixtures; for a release, no pin is older
  than 183 days.
- **Pins and help fixtures.** `scripts/helpers-pin.sh` is run by a maintainer, never by
  `make check`. It fetches each pinned artefact with GET only, prints its sha256 and records each
  used subcommand's help text under `crates/helpers/fixtures/help/`. Helpers use only flags found
  there. Refresh `helpers.lock` and the fixtures at least every six months and whenever a helper
  has a security fix.
- **Tailnet helper builds.** `nocturne-tailnet` (`helpers/tailnet`) is built for Linux, macOS and
  Windows in release CI (`.github/workflows/tailnet.yml`, `scripts/build-tailnet.sh`) with the
  pinned Go toolchain, no cgo and no embedded paths, and reproduced by a second independent build
  that must match byte for byte before the hash is pinned. The macOS and Windows binaries are
  signed (and notarised on macOS) and a licence report is written. Its Go tests run in `make check`
  when the pinned Go is present.
- **Reference vectors.** `scripts/remote_vectors.py` regenerates the channel's reference vectors
  from an independent implementation; both the Rust and browser tests read the committed file. A
  change to the channel changes the script first.
- **Real-network trial.** Whether helpers behave on the real internet, including tunnel stability,
  helper memory and the phone checks, is established only by trials the owner approves. Until then
  release notes do not claim it.

## Online answers checks

These use a stand-in provider on loopback and a reserved test address; none reaches a real provider.
The unit and integration tests run in `make check`. `make e2e-online` and `make e2e-models` run
the real models with `scripts/e2e-real.sh`.

- **Net check, `online` mode.** `make netcheck-online` runs `scripts/net-check.sh online`. A prepared
  data folder has Online answers on, the key in the fallback file and an address that is a
  documentation-range IP literal (no DNS). A scripted driver asks questions, idles for 10 minutes,
  turns Online answers off and asks again. A non-loopback connect passes only for the engine, only to
  that address, at most once per question plus the one model-listing request at each engine start,
  never while idle and never after the off mark. The app never connects. The existing `off` judgement
  of `scripts/netlog.py` must also be run with a key stored and Online answers off, and stay at zero.
- **Probe at each engine bump.** Before moving the pinned engine version, repeat the route B probe
  (the load, then key handover, sequence against a stand-in provider on loopback with no other
  network): a fixed model row answers, the key arrives only at run time through the engine's own
  endpoint and never in the environment, the only discovery request is the one model-listing request
  at start, clearing the key stops the next question before any request, and redirects are not
  followed. A change to `cloud_providers` or the cloud authentication endpoint in the engine blocks
  the bump until the route, the log pipe and decision 0011 are checked again.
- **Fit table.** Refresh the shipped memory figures in the fit table when the catalogue or a backend
  changes, and check that every tier preset still fits at its own minimum.
- **Package check.** `scripts/check-package.sh` fails a package whose program carries the
  `online-stand-in` marker (`STAND_IN_MARKER`), as it does for the helper stand-in marker.

## Desktop checks

Run each on the installed or extracted package, with a scratch home so nothing touches your own
files. Tick them off in the release notes.

| Check | GNOME on Wayland | X11 session |
|---|---|---|
| The window opens and the setup welcome shows | yes | yes |
| A tray icon shows (with the AppIndicator extension on GNOME) and its menu keeps, asks, opens today's page, pauses and quits | with the extension | yes |
| Without a tray, closing the window quits the app and starting at login opens the window | yes | yes |
| The quick keep shortcut opens the small box; on Wayland Settings shows the command for a desktop shortcut instead, and running that command opens the box | the command route | the shortcut itself |
| A file dropped on the window, and a file passed to a second launch, are kept once | yes | yes |
| Choosing an inbox folder keeps a file saved there, and leaves it where it was | yes | yes |
| One notification arrives when the page is ready, says nothing about what was kept, and stops when "Don't interrupt me" is chosen | yes | yes |
| "Start with this computer" adds the start-up entry, and a login starts the app in the tray | yes | yes |
| The page reads aloud (Listen) and the print view opens (Print); printing happens only on request | yes | yes |
| Quitting from the tray or with a signal leaves no `lemond` behind | yes | yes |
| Removing the package leaves the data folder in place | yes | yes |

## When a step fails

A failure is fixed in the code, or recorded as an issue. Do not tag around it.
Re-run the whole checklist after the fix, not only the failed step.
