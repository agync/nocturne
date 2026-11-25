---
type: Log Entry
title: "Gave the iPhone its own Apple key and moved signing secrets into environments."
description: "The Mac signing job reads its identity from the certificate, the iPhone job uses its own App Store Connect key, and the signing secrets live in the release-signing environment."
tags: [log, update]
kind: Changed
status: stable
---

# Gave the iPhone its own Apple key and moved signing secrets into environments

- `sign-macos` reads the Developer ID identity from the imported certificate;
  `MACOS_DEVELOPER_ID_IDENTITY` is now optional.
- The iPhone job reads only `APP_STORE_CONNECT_API_*`, an App Manager key of its own, separate
  from the Mac notary key.
- Signing secrets live in the `release-signing` environment, which deploys only from `main` and
  `v*` tags, not in organisation or repository secrets, which any workflow on any branch can read.
  The release bot's secrets live in `release-app`.
- The Android upload key secrets are named `NOCTURNE_ANDROID_UPLOAD_*`, to tell the key from other
  apps' keys.
