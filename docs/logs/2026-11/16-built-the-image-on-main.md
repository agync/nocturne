---
type: Log Entry
title: "Built the server image on main, not on every code change."
description: "Pull requests build the image only when its own files change; main builds it after merges."
tags: [log, update]
kind: Update
status: stable
---

[Building](/contribution/building.md#the-server-image) now says when the container workflow runs: a
pull request builds the image only when the `Containerfile`, `.containerignore`, the engine fetch
scripts or the workflow change, and every push to `main` that touches what goes into the image
builds it.
