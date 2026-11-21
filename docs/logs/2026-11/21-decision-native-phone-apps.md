---
type: Log Entry
title: "Recorded the decision for native phone apps."
description: "Decision 0017 records native phone apps on a Rust sealed channel shared with the computer, with the browser phone page kept."
tags: [log, update]
kind: Added
status: stable
---

[Decision 0017](/adr/0017-native-phone-apps.md) records the approved phone apps: Tauri 2 apps for
Android first and iPhone later, one Rust crate for the sealed channel that the computer and the app
share, the browser phone page kept beside them, Google Play and a signed file for Android, and
sending only while the app is open in the first release.
