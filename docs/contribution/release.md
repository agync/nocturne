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
