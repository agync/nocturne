---
type: Guide
title: Releasing
description: The local release checklist, from a clean main to an approved annotated tag. Nothing is pushed or uploaded.
status: draft
tags: [contribution, release, checklist]
---

# Releasing

A release is built, checked and tagged on one machine. This checklist never pushes a branch, uploads
a file or creates a release page; publishing is a separate decision made by a human after it.

## Checklist

Keep the real output of each step with the release notes.

1. **Start clean.** `main` is rebased on the latest remote state, the working tree is clean, and
   `make check` passes.
2. **Bump the version** in the workspace `Cargo.toml` only, and let `Cargo.lock` follow. The
   packaging script refuses to run if `tauri.conf.json` ever carries a different version.
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
12. **Approval and tag.** A human approves. Only then, create a local annotated tag. Pushing the
    tag and publishing files are separate decisions, outside this checklist.

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
