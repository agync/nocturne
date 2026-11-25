---
type: Log Entry
title: "Moved the light release jobs to the small runner."
description: "Planning, publishing, finishing, the Flatpak resolve and deploy steps, the helper's checksum steps and the cache prune run on GitHub's ubuntu-slim runner."
tags: [log, update]
kind: Changed
status: stable
---

The jobs that only call the GitHub API, move artifacts or check sums now run on `ubuntu-slim`, a
1 vCPU container that starts faster than a full virtual machine.

- Moved: release `plan`, `publish` and `finish`; Flatpak `resolve` and `deploy`; the helper's
  `reproduce` and `publish`; the cache prune. Each sets `timeout-minutes: 15`, the runner's limit.
  Their recent runs took 6 to 25 seconds.
- Stayed: `sign-android` (needs the Android SDK build tools and Java), the Flatpak `publish` and
  `site` (OSTree and registry tools, up to 28 minutes), `release-please` (runs `cargo`), and the
  helper's `licenses` (builds a Go tool; no run to time).
