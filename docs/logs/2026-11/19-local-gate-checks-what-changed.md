---
type: Log Entry
title: "The local gate checks what changed."
description: "make check-changed runs the basics for the paths that changed; CI runs the full suite."
tags: [log, update]
kind: Changed
status: stable
---

Before a push, a contributor now runs `make check-changed` instead of `make check`. It reads the
paths that differ from the merge base with `main`, asks `scripts/ci-areas.sh --local` which areas
they touch, runs those areas' lint and drift checks, and runs the git hooks on the changed files.
A docs-only change no longer builds the Rust workspace. The change's own tests run through
`TESTS=`, and the full suite runs in CI, so a change is done when CI is green. `make check` is
unchanged. The [contribution guide](/contribution/guide.md) has the new Verify step.
