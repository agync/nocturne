---
type: Log Entry
title: "Match the minimum Rust to the Flatpak SDK."
description: "Match the minimum Rust to the Flatpak SDK."
tags: [log, update]
kind: Update
status: stable
---

`rust-version` is 1.98, the Rust of the Flatpak SDK's `rust-stable` extension, so clippy flags
anything that SDK cannot build. `rust-toolchain.toml` still pins 1.100.0 for development and CI.
