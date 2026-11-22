---
type: Log Entry
title: "CI checks the engine for the phone on its own."
description: "A change to the engine no longer waits for the whole phone app build to learn that it still builds for Android."
tags: [log, update, ci, phone]
kind: Changed
status: stable
---

Decided 2026-11-22. A phone that keeps on its own carries the engine, so the engine has to build for
the phone's processor. A light job now checks that, with the Android toolchain and no app build, and
runs when the engine, its contract or the Cargo files change. The full phone app job runs on the
same paths as before. [Building](/contribution/building.md) lists both.
