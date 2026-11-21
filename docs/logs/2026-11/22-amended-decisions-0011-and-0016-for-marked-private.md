---
type: Log Entry
title: "Amended decisions 0011 and 0016: only marked private is held back."
description: "Online answers no longer keeps back health, money or ID memories, and the overnight shadow pass no longer asks whether a memory is one."
tags: [log, decision]
kind: Update
status: stable
---

[Decision 0011](/adr/0011-online-answers.md) now sends any memory that answers a question unless it
is marked private, being forgotten or let go; the health, money and ID check and its switch are
gone, because Nocturne never judges a person's data. [Decision 0016](/adr/0016-decision-models-shadow-first.md)
asks two kinds in shadow instead of three. The [Online answers architecture](/architecture/online.md)
lists the filters that remain.
