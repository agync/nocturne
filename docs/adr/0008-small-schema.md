---
type: Decision
title: "0008 A small schema"
description: "A short list of predicates; new ones are quarantined until confirmed."
status: draft
tags: [adr]
---

# 0008 A small schema

## Decision

The allowed predicates start as `prefers`, `works_on`, `decided`, `lives_in`, `knows`, `happened`,
`due`, `promised`, `scheduled` and `takes`. The extractor may propose a new predicate, but it is
quarantined until the person confirms it.

## Why

Local extraction is measurably weaker on relations than cloud models. A small schema keeps the memory
legible and the page honest. `due`, `promised` and `scheduled` drive the page's needs-you section;
`takes` covers medication.
