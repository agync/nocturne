---
type: Log Entry
title: "Hardened the private engine."
description: "A separate admin key for the engine's internal endpoints, and no browser origin allowed."
tags: [log, update]
kind: Update
status: stable
---

The runtime page now says the engine's internal endpoints take their own per-launch key, which the
model client never holds, and that the engine's list of allowed browser origins is closed.
