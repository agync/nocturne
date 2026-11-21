---
type: Log Entry
title: "Built the iPhone app for the Simulator."
description: "The phone app now builds for the iOS Simulator, with Swift calendar and Health plugins behind the same Rust commands, the iPhone purpose strings and a committed Xcode project."
tags: [log, update, phone, ios]
kind: Added
status: stable
---

The phone app builds and runs in the iOS Simulator. The calendar plugin reads through EventKit
with full access and the Health plugin through HealthKit, asked only to read; both answer the
same commands as their Kotlin twins, so the app's sync is unchanged. The purpose strings come
from the design's copy. Signing, device builds and the share extension come later.
[Building](/contribution/building.md#the-iphone-app) has the targets and the Mac setup.
