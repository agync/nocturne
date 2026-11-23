---
type: Log Entry
title: "Tested only the crates a change reaches."
description: "On a pull request the Rust jobs test the packages a change reaches, under cargo-nextest with flaky tests reported, and the other platforms run only when what builds differently there changed."
tags: [log, update]
kind: Update
status: stable
---

On a pull request the Rust jobs now test only the workspace packages a change reaches: the ones
its files are in, the ones that depend on them, and the ones whose tests read them
(`scripts/affected_crates.py`). The tests run under cargo-nextest, which retries a failed test
once and lists one that passed on the retry as flaky in the job summary. The Rust jobs on Windows,
macOS and Linux on Arm run on a pull request only when what builds differently there changed. A
push to `main` and a new nightly run still test the whole workspace on every platform.
[Building and packaging](/contribution/building.md#continuous-integration) has the details.
