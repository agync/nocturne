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
| `trust` | text | `trusted` or `untrusted`, from the source (see [trust by source](#trust-by-source)) |
| `metadata` | JSON | Stored original, MIME type, the understanding step that produced `content`, and other context |

The hash makes capture idempotent. Re-ingesting the same page does not create a second event.

## Fact graph

A fact is one durable claim. It is not a document chunk.

| Field | Type | Meaning |
|---|---|---|
| `id` | text | `fact_` plus 16 hex characters |
| `subject` | text | A person, project, place, or `user` |
| `predicate` | text | One of the allowed predicates ([decision 0008](/adr/0008-small-schema.md)) |
| `obj` | text | The object of the claim |
| `valid_from` | text or null | When the claim became true |
| `valid_to` | text or null | When it stopped being true |
| `recorded_at` | text | When Nocturne learned it |
| `confidence` | real | 0.0 to 1.0 |
| `provenance` | JSON | Supporting event ids, each with an optional source span: an audio time range, a page, an image region, or a quoted passage |
| `sensitivity` | text | `normal` or `private`; private facts never reach audio, print, notifications or the tray |
| `status` | text | `active`, `fading`, `archived`, or `quarantined` |
| `last_seen` | text or null | Last reinforcement or use |
| `ttl_days` | integer or null | An explicit expiry, when the user sets one |
| `keep_forever` | boolean | Exempt from decay; on by default for health, allergy and medication facts |

The pair of `valid_from` and `recorded_at` is the bitemporal part. It lets the system answer two
different questions:

- What was true on 1 March?
- What did we believe on 1 March?

A correction does not overwrite the old row. It closes `valid_to` on the old row and inserts a new
row. The paper can show the change.

## Status lifecycle

```text
quarantined -> active <-> fading -> archived
                 ^                     |
                 +------ restore ------+
any status -> forgotten (row removed, explicit and confirmed)
```

| Status | Meaning | Interface word |
|---|---|---|
| `quarantined` | From an untrusted source; never presented as true | unconfirmed |
| `active` | In normal use | (kept) |
| `fading` | Lower confidence after 30 days without use | fading |
| `archived` | Out of the hot set after 90 days without use; reversible | let go |

"Use" means cited in an answer, cited on a page, or confirmed or corrected by the person. Facts with a
future `valid_from` or due date do not decay before that date, and `keep_forever` facts never decay.
Reinforcement brings an archived fact back to active. That is the difference between an archive and a
landfill.

Forgetting is not a status: the row is removed. See [forgetting](#forgetting).

## Search

SQLite FTS5 indexes `subject`, `predicate`, and `obj`. The query is tokenised and matched with BM25.
A vector column is reserved for semantic recall, but the first version is lexical because it is
easier to explain and to debug.

## Provenance rules

1. A fact without at least one event id is invalid.
2. An untrusted event cannot create an active fact. It creates a quarantined candidate.
3. A quarantined candidate needs a second, independent source or a confirmation from the person.
4. The page and answers cite fact ids, not the model's prose, and each citation resolves to an event.

## Trust by source

| Source | Default trust |
|---|---|
| Typed or spoken by the person | trusted |
| A photo, screenshot or recording the person kept | trusted |
| A pasted email or chat message | trusted, attributed to the sender where known |
| Pasted web content | untrusted |
| A file in the watched folder | trusted |
| Another app through MCP | untrusted |

## Forgetting

Forgetting is explicit and confirmed, and it is not undoable.

| Thing | Effect |
|---|---|
| The fact and its earlier versions | Removed |
| Derived text and indexes (FTS rows, vectors, cached audio) | Removed |
| The source event and its original file | Removed when the person chooses, the default when no other fact depends on it |
| Lines on past pages that cite the fact | Redacted to "(forgotten)" |
| Exports and printed pages already made | Out of reach; the confirmation says so |
