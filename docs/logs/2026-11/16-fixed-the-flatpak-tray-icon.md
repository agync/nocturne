---
type: Log Entry
title: "Fixed the blank tray icon in the Flatpak."
description: "The tray icon file now lives in the app's cache folder, which the desktop's panel can read."
tags: [log, update]
kind: Update
status: stable
---

On GNOME the Flatpak showed a blank tray icon. The tray library writes the icon to a file and
gives the panel its path; the default folder is inside the sandbox's private runtime folder,
which the panel cannot open. The app now keeps that file in its own cache folder. The
[Flatpak page](/contribution/flatpak.md#the-tray-icon) explains why no extra permission is
needed.
