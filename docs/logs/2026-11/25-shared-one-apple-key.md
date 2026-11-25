---
type: Log Entry
title: "Shared one Apple key between Mac and iPhone signing."
description: "The Mac signing job reads its identity from the certificate, and the iPhone job falls back to the Mac notary key, so one App Store Connect key serves both."
tags: [log, update]
kind: Changed
status: stable
---

# Shared one Apple key between Mac and iPhone signing

- `sign-macos` reads the Developer ID identity from the imported certificate;
  `MACOS_DEVELOPER_ID_IDENTITY` is now optional.
- The iPhone job uses `APP_STORE_CONNECT_API_*` when set, otherwise `MACOS_NOTARY_API_*`, so one key
  with the App Manager role notarises and uploads.
- The runbook allows organisation secrets shared with the repository.
- The Android upload key secrets are named `NOCTURNE_ANDROID_UPLOAD_*`: the key is Nocturne's own,
  shared as organisation secrets with this repository only, while the Apple ones serve the team.
