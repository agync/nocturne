---
type: Log Entry
title: "Readied macOS signing and added a signing dry run."
description: "The release workflow notarises the app and the disk image with an App Store Connect API key or an Apple ID, and a hand run with no tag can prove the signing without a release."
tags: [log, update]
kind: Changed
status: stable
---

The macOS signing jobs in the release workflow are ready for the owner's certificate.

- Signing goes inside out with the hardened runtime and a timestamp. The engine's executables get
  an entitlement file of their own; the app keeps the microphone one.
- The app is notarised and stapled, then the disk image, and a rejected submission prints Apple's
  log. `spctl` assesses both.
- Notarisation prefers an App Store Connect API key and falls back to an Apple ID and an
  app-specific password.
- A hand run with no tag and `sign` on runs the signing jobs and keeps the signed image as an
  artifact, without touching a release.

The release guide lists the new secrets and the steps to turn signing on.
