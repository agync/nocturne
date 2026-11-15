---
type: Log Entry
title: "Checked the release build for Rust changes."
description: "The Rust jobs now compile the app as a release does, without building a package."
tags: [log, update]
kind: Update
status: stable
---

A release build differs from every other build in CI: some of the app's code is compiled only for
a release, and the tests turn on the crates' stand-in features. The Rust jobs now check the app's
release build on every platform they cover, which compiles all of it without generating code, so
a pull request that changes the Rust sources does not build the packages. The `ui` job also builds
the interface the app ships and checks it as the package script does, which the browser tests'
mock build did not cover.
