---
type: Log Entry
title: "Made the desktop app smaller."
description: "The Linux app program went from 45.2 MB to 27.8 MB by linking one encryption library and building for size, with no change to what a person sees."
tags: [log, update]
kind: Changed
status: stable
---

The Linux app program shrank from 45.2 MB to 27.8 MB. Nothing a person sees changed.

- The app linked two encryption libraries, ring and aws-lc-rs. Its web clients now use ring, the
  one the phone link and the sealed channel already used. That saved 2.8 MB. Clients are built
  through `nocturne_lemonade::tls`, which installs the provider once.
- The release build is size-tuned, with whole-program linking and `opt-level = "s"`. That saved
  another 14.6 MB, and a build takes about as long as before.
- Two versions of the desktop bus library stay: three Tauri plugins pin the older one and the
  notification library pins the newer one, so they cannot be merged without a fork.
- Aborting on a panic would save a further 4.7 MB but is not used: the tray catches a panic when
  its indicator library is missing, and a panicking command is reported as an error instead of
  ending the app.
