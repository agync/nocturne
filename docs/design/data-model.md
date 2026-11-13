---
type: Design
title: Data model
description: The event tape, the fact graph, the status lifecycle and the provenance rules.
status: draft
tags: [data-model, memory, provenance, bitemporal]
---

# Data model

Two stores in one local SQLite file: an append-only tape of what arrived, and a graph of what is
believed. This model carries over from the retired scaffold and is the contract the rewrite
implements.

## Event tape

The tape is append-only. An event is what arrived, not what it means.

| Field | Type | Meaning |
|---|---|---|
| `id` | text | `evt_` plus 16 hex characters |
| `kind` | text | `text`, `image`, `audio`, `page`, or `note` |
| `source` | text | The capture surface: quick capture, drop, watched folder, MCP, or an app name |
| `content` | text | The text, or the transcription, or a description |
| `captured_at` | text | ISO 8601 UTC |
| `content_hash` | text | SHA-256 of the original bytes (the file for media, the text otherwise), unique |
| `metadata` | JSON | Stored original, MIME type, the understanding step that produced `content`, and other context |

The hash makes capture idempotent. Re-ingesting the same page does not create a second event.

## Fact graph

A fact is one durable claim. It is not a document chunk.

| Field | Type | Meaning |
|---|---|---|
| `id` | text | `fact_` plus 16 hex characters |
| `subject` | text | A person, project, place, or `user` |
| `predicate` | text | `prefers`, `works_on`, `decided`, `lives_in` |
| `obj` | text | The object of the claim |
| `valid_from` | text or null | When the claim became true |
| `valid_to` | text or null | When it stopped being true |
| `recorded_at` | text | When Nocturne learned it |
| `confidence` | real | 0.0 to 1.0 |
| `provenance` | JSON | Event ids that support the claim |
| `sensitivity` | text | `normal`, `private`, or `secret` |
| `status` | text | `active`, `fading`, `archived`, `quarantined`, or `deleted` |
| `last_seen` | text or null | Last reinforcement or use |
| `ttl_days` | integer or null | An explicit expiry, when the user sets one |

The pair of `valid_from` and `recorded_at` is the bitemporal part. It lets the system answer two
different questions:

- What was true on 1 March?
- What did we believe on 1 March?

A correction does not overwrite the old row. It closes `valid_to` on the old row and inserts a new
row. The paper can show the change.

## Status lifecycle

```text
active -> fading -> archived -> deleted
   ^         |          |
   |         +----------+
   +--------- restore or reinforce
```

- **active**: in normal use.
- **fading**: low confidence after 30 days without use.
- **archived**: out of the hot set after 90 days without use. Reversible.
- **deleted**: removed by the user. Not reversible.

Reinforcement brings an archived fact back to active. That is the difference between an archive and a
landfill.

## Search

SQLite FTS5 indexes `subject`, `predicate`, and `obj`. The query is tokenised and matched with BM25.
A vector column is reserved for semantic recall, but the first version is lexical because it is
easier to explain and to debug.

## Provenance rules

1. A fact without at least one event id is invalid.
2. An untrusted source cannot create an active fact. It creates a quarantined candidate.
3. A quarantine candidate needs a second, independent source or a user confirmation.
4. A deletion removes the fact and its index row, but keeps the source event. The tape is the record
   of what arrived; the graph is the record of what was believed.
5. The paper cites the fact id, not the model's prose.
