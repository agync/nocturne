---
type: Log Entry
title: "Gave notarisation longer to finish."
description: "The first Developer ID signing dry run signed cleanly but stopped after Apple's notary service had not answered in 60 minutes; each submission now waits up to 80 minutes and a timeout prints Apple's status."
tags: [log, update]
kind: Changed
status: stable
---

# Gave notarisation longer to finish

- The first signing dry run signed the app and the engine and verified them, then stopped after
  60 minutes with Apple's notary service still processing: a team's first submissions can sit in
  Apple's in-depth check for over an hour.
- Each submission now waits up to 80 minutes, and `sign-macos` allows 180.
- A submission that is not accepted prints `notarytool info` and `notarytool log`, so a timeout
  shows whether Apple is still working on it.
