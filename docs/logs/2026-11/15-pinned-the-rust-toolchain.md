---
type: Log Entry
title: "Pin the Rust toolchain."
description: "Pin the Rust toolchain."
tags: [log, update]
kind: Update
status: stable
---

`rust-toolchain.toml` pins Rust 1.100.0 for local builds and every CI job, and `rust-version` moves to
1.100, so new compiler lints arrive only when the pin is bumped. The pin bumps page names both files.
