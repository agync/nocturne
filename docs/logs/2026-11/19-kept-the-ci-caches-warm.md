---
type: Log Entry
title: "Kept the CI caches warm."
description: "Each Rust job keys its cache on the pinned toolchain only, the platform test jobs stop building the workspace twice, and a new workflow prunes old cache generations."
tags: [log, update]
kind: Changed
status: stable
---

The Rust jobs were restoring their dependencies on a warm run (no registry crate compiled) and
rebuilding all 444 to 454 of them when a job found no cache, which added up to about nine minutes
on macOS and three on Windows. Two things made a job find no cache, and a third made the warm runs
slower than they needed to be:

- `Swatinem/rust-cache` hashes every installed Rust toolchain into its key, and the hosted runner
  image ships an older preinstalled one next to the pinned `1.100.0`. The extra toolchain comes and
  goes between runner instances, so the same job was saved under two keys. A new composite action,
  `.github/actions/rust-toolchain`, installs the pinned toolchain and removes the others before the
  cache step.
- Every `Cargo.lock` change starts a new generation of 7 to 9 caches, and the repository held
  12.15 GB against GitHub's 10 GB limit, so caches were evicted least recently used first. A new
  `cache-prune.yml` keeps the newest cache for each job and platform and deletes the rest.
- The platform test jobs built `cargo build --workspace --tests` in the `dev` profile and then
  `cargo test` built the same crates in the `test` profile, about four minutes on Windows. They now
  build the `test` profile once.

The Windows lint job also sets `core.autocrlf false` like every other Windows job, so the files its
cache key hashes match the rest of CI. Its key was stable, just different, so no run was lost to it.

The first push after this lands is cold on every Rust job: the toolchain set is part of the cache
key, so every key changes once. A later prune clears the generation it replaces, once that has gone
an hour unused.

[Continuous integration](/contribution/building.md) says how the caches are kept.
