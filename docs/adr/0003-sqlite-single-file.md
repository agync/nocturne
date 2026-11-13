---
type: Decision
title: "0003 One SQLite file"
description: "The tape and the memory live in one local SQLite file."
status: draft
tags: [adr]
---

# 0003 One SQLite file

## Decision

The tape and the memory live in one SQLite file with FTS5 for lexical search and a vector column for
semantic recall. Originals are stored beside it by content hash.

## Why

A local app should start with one file the person can copy, inspect and delete. A database server
adds a service, a port and a failure mode before the page exists.

## Revisit when

The memory exceeds a few hundred thousand facts, or concurrent writers matter.
