---
type: Decision
title: "0007 Forgetting by default"
description: "Facts fade at 30 days and archive at 90; deletion is explicit."
status: draft
tags: [adr]
---

# 0007 Forgetting by default

## Decision

A fact fades after 30 days without use and is archived after 90. Archiving is reversible. Deletion is
explicit, confirmed, and cascades to the fact's index entries while the tape keeps the source event
until the person deletes that too.

## Why

Memory that only grows becomes noise. Visible, reversible forgetting is the novel part, and it
handles bloat and poisoning better than a bigger index.
