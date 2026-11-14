---
type: Log Entry
title: "Moved Flatpak signing out of the privileged build."
description: "Moved Flatpak signing out of the privileged build."
tags: [log, update]
kind: Update
status: stable
---

Described how the Flatpak build runs privileged without secrets, pinned by digest, and how a separate unprivileged job in the release-signing environment signs and pushes the images.
