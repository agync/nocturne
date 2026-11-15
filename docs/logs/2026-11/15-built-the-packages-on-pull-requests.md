---
type: Log Entry
title: "Build the packages on pull requests."
description: "CI builds the Linux, macOS and Windows packages when a pull request touches packaging paths."
tags: [log, update]
kind: Update
status: stable
---

A pull request that changes the packaging scripts, the bundle configuration, the lock files or the
two workflows now builds the Linux x86_64, macOS and Windows packages in CI, unsigned and uploaded
nowhere, and runs the same checks as the release. Packaging bugs used to surface only when a release
was cut.
