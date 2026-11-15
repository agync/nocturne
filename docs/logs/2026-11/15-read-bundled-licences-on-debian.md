---
type: Log Entry
title: "Read bundled library licences on Debian hosts."
description: "Read bundled library licences on Debian hosts."
tags: [log, update]
kind: Update
status: stable
---

The AppImage's bundled library licences came only from the rpm database, which is empty on the
Ubuntu release runners. With every library unowned, the step searched the whole of `/usr/lib` for
each library missing from the linker cache, following symlinks, and the release jobs timed out.
The step now reads dpkg on Debian and Ubuntu (owner, source package, machine-readable copyright
file) and rpm on Fedora, finds modules by their relative path with at most one walk that never
follows a link, prints one line per library, and fails at once when no package database knows a
library. Debian names joined the copyleft allowlist, and the TIFF pixbuf loader is no longer
bundled: on Ubuntu it pulls in jbigkit, which is GPL-2.0-or-later.
