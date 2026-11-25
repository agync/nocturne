---
type: Log Entry
title: "Left the engine's command-line client out of every package."
description: "The deb, rpm, AppImage, Flatpak and Windows installers no longer carry upstream's lemonade client, about 5 MB, which the app never runs."
tags: [log, update]
kind: Changed
status: stable
---

The app only starts `lemond`. Upstream's `lemonade` command-line client, which macOS already left
out, was still shipped on Linux and Windows.

- `scripts/package.sh` removes it after the archive is verified, so the deb, rpm, AppImage and
  both Windows installers leave it out. The Flatpak build removes it too.
- The package check fails a package that holds it, and compares the rest of the engine with the
  verified manifest byte for byte.
