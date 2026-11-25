---
type: Log Entry
title: "Moved the Mac engine's program out of Resources."
description: "The macOS app keeps lemond beside its own program in Contents/MacOS and the engine's data in Contents/Resources, so a Developer ID signature seals every piece of code."
tags: [log, update]
kind: Changed
status: stable
---

The signing dry run failed: codesign read `Contents/Resources/lemonade/` as a bundle, because it
held a program of its own name, and found the engine's licence unsigned inside it.

- `lemond` now sits in `Contents/MacOS` beside the app's program, as Apple places helper code.
- The engine's data stays in `Contents/Resources/lemonade`, and `Contents/MacOS/resources` links
  to it, since `lemond` looks for its data next to itself.
- Upstream's command-line client, which the app never runs, is left out of the macOS app.
- The app looks for `lemond` beside its own program on macOS first. Linux and Windows are
  unchanged.
- The signing job and the package check stop on any program in `Contents/Resources`.
