---
type: Log Entry
title: "Ship releases without an unpinned helper."
description: "Ship releases without an unpinned helper."
tags: [log, update]
kind: Update
status: stable
---

The release builds the Most private helper only once `helpers.lock` has no placeholder pins, and
publishes without it until then. `release-as` and `bootstrap-sha` are gone from the release-please
configuration, so the first published release is 0.1.1.
