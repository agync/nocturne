---
type: Log Entry
title: "Built the phone apps in CI and the release."
description: "ci.yml builds the iPhone app for the Simulator and runs its Swift tests; release.yml builds the Android app unsigned, signs it and uploads the iPhone app to TestFlight when the owner turns signing on."
tags: [log, update]
kind: Added
status: stable
---

The phone apps are part of the pipeline, ready for the stores once the owner adds the signing
secrets.

- `ci.yml` gains an `ios` job on macOS that builds the iPhone app for the Simulator and runs the
  plugins' Swift tests, on the same paths as `android`.
- `release.yml` builds the Android release APK and bundle for arm64, armv7 and x86_64 without a
  secret. With `ANDROID_SIGNING` on, `sign-android` signs them with the upload key and attaches
  them to the release. Without it, nothing of Android is attached.
- With `IOS_SIGNING` on, `ios-testflight` builds, signs and uploads the iPhone app to TestFlight in
  one job, because an iPhone build is signed while it is built.
- [Releasing](/contribution/release.md#the-phone-apps) lists the version codes, the new variables
  and the secrets, and what happens without them.
