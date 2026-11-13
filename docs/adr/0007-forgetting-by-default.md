---
type: Decision
title: "0007 Forgetting by default"
description: "Facts fade at 30 days and archive at 90; deletion is explicit."
status: draft
tags: [adr]
---

# 0007 Forgetting by default

## Decision

A fact fades after 30 days without use and is archived after 90. Archiving is reversible. Use means
cited in an answer, cited on a page, or confirmed or corrected by the person.

Exceptions: facts with a future date (appointments, deadlines, promises) do not decay before the
date, and health, allergy and medication facts are kept forever by default.

Forgetting is explicit, confirmed and not undoable. It removes the fact, its history and everything
derived from it, offers to remove the original, and redacts the fact's lines on past pages. The full
table is in the [data model](/design/data-model.md#forgetting).

## Why

Memory that only grows becomes noise. Visible, reversible forgetting is the novel part, and it
handles bloat and poisoning better than a bigger index. The exceptions stop the policy from
archiving the rarely used facts that matter most.
