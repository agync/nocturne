# Decisions

## ADR-001: SQLite, not a server

The first version stores the tape and the graph in one SQLite file. It uses FTS5 for search and a
reserved BLOB column for vectors.

Why: a local app should start with one file the user can copy, inspect, and delete. A Postgres or
graph server adds a service, a port, and a failure mode before the paper exists.

When to revisit: when the graph exceeds a few hundred thousand facts, or when concurrent writers
matter. Then consider HawDB or Postgres.

## ADR-002: an open engine, not a Nowledge Mem plugin

The memory engine is Apache-2.0 and self-contained. It can import from Nowledge Mem, but it does not
depend on the closed backend.

Why: the challenge requires open access, and the user should be able to run the paper without a
vendor account. Borrow the graph and working-memory ideas, not the binary.

## ADR-003: the paper first

The first milestone is a text-only paper. The senses, the audio, and the printer come after.

Why: the paper is the demo and the proof. A perfect graph with no artifact is not a submission.
Scope creep is the largest risk in a 14-day build.

## ADR-004: the NPU is optional and idle-only

The NPU classifies salience when the GPU is idle. The default path is sequential. The concurrent
NPU plus GPU path is an experiment.

Why: the one no-contention measurement is not reconciled with a later segfault report. A challenge
entry should not depend on an unstable path. The NPU is a bonus, not the architecture.

## ADR-005: forgetting by default

A fact fades after 30 days without use and archives after 90. Archiving is reversible. Deletion is
explicit.

Why: memory that only grows becomes noise. The forgetting policy is the novel part, and it solves
bloat and poisoning better than a bigger vector index.

## ADR-006: a small schema

The allowed predicates are `prefers`, `works_on`, `decided`, `lives_in`, `knows`, and `happened`.
The extractor may propose a new predicate, but it goes to quarantine until the user confirms it.

Why: the local extraction evaluation shows weaker relation quality than a cloud model. A small
schema keeps the graph legible and the paper honest.
