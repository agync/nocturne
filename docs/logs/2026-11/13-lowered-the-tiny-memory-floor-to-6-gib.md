---
type: Log Entry
title: "Lowered the Tiny memory floor to 6 GiB."
description: "Lowered the Tiny memory floor to 6 GiB."
tags: [log, update]
kind: Update
status: stable
---

Tiny now needs 6 GiB of reported memory instead of 7. Exactly 6 GiB gets Tiny and anything under
it is unsupported. The runtime page's tier table and its Running on 8 GB section, and the install
guide's setup table, say 6.
