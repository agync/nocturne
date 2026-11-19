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
| `kind` | text | `text`, `image`, `audio`, `page`, `note`, or `checkin` |
| `source` | text | The capture surface: quick capture, drop, watched folder, MCP, or an app name |
| `content` | text | The text, or the transcription, or a description |
| `captured_at` | text | ISO 8601 UTC |
| `content_hash` | text | SHA-256 of the original bytes (the file for media, the text otherwise), unique |
| `trust` | text | `trusted` or `untrusted`, from the source (see [trust by source](#trust-by-source)) |
| `metadata` | JSON | Stored original, MIME type, the understanding step that produced `content`, and other context |

The hash makes capture idempotent. Re-ingesting the same page does not create a second event. A
journal item hashes its text with its own id, so two entries with the same words are both kept.

A `checkin` is a feelings check-in from the journal. It holds a part, a point (two numbers from
-1 to 1), up to three words, and optionally a line and an urge, in `metadata`. A check-in yields
mentions only, never facts: reading its lines, the urge included, finds names and writes
[mentions](#mentions), so a check-in never enters the fact graph, never fades, and never reaches
Ask or goes online.

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
| `sensitivity` | text | `normal`, `private` or `journal`; private facts never reach audio, print, notifications or the tray. A `journal` fact is read from a journal entry or a good thing: it never reaches audio, notifications, the tray or a phone's lock screen, and is allowed in Print, export, the phone and Online answers ([decision 0015](/adr/0015-journal-privacy-class.md)). A fact takes the strictest of its sources: private, then journal, then normal. The journal mark is never cleared when a journal source is later deleted or refiled; it stays the stricter mark |
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

## Filing: names, merges and moves

A memory is filed under one person, place or thing (an entity). Its `subject` and `subject_key` are
that entity's name and key, kept on the row so search and matching need no join. The person can
change how things are filed. Each change is explicit, logged and reversible, and none of them
deletes anything.

| Change | Rule |
|---|---|
| Rename | The entity takes the new name. Every memory filed under it, including earlier versions, takes it as `subject`. The old name is kept as a former name. Current memories whose sentence says the old name, as a whole word and in the same case, say the new one. Saved pages keep what they said that day. |
| Merge | Offered when a new name is one another entity already has. Everything filed under the first moves to the second, which keeps its name; the first stays hidden, so the merge can be taken back. A claim both already held the same way is kept once, with both receipts. |
| Re-file | One memory, with its earlier versions, moves to another entity or to a new one. Its words are not touched. It remembers what it was first filed under (`moved_from`). |
| Undo | Each change returns an id. Taking it back restores names, filing and wording exactly, as long as nothing since has made that impossible. Changes undo newest first: a rename waits for any later rename or merge of that entity, and a merge for any later change that touches either entity. An undo that cannot finish whole fails and changes nothing. |

A claim read again must not undo these choices. The overnight tidy and any other re-read run the
same path as the first read, so it asks, before matching, what the person has decided:

1. A subject that is a former name of an entity, and that no live entity carries, is read as that
   entity's current name, and its sentence follows. The same holds for the names of an entity that
   was merged in. What the read writes is noted against the change, so taking the change back
   takes that back too.
2. A memory the person moved is found again by the name it was first filed under, but only by
   the item that first said it. Any other item that says something about that name is about
   that name, so it neither reinforces, closes nor conflicts with the moved memory.
3. A claim forgotten under any of these names stays forgotten under all of them.

A name is matched by its key whatever the kind of thing, so two things never share one. Where
older data has two, a subject files under the one of the kind asked for, then a person, a place,
a thing, then by id.

Forgetting removes an entity once nothing is filed under it. A thing merged into another stays,
hidden, for as long as the one it was merged into does, so a forget that has nothing to do with
a merge does not take its undo away. A move whose source has been forgotten cannot be taken back.
An export lists former names and where a moved memory was first filed, and leaves out things
merged into another, which the one that remains is also known by. Delete everything removes every
one of these rows.

## Mentions

A mention links a span of an item's text to an entity, so the journal can show who and where an
entry is about. Reading writes them, for entries, good things and the lines of check-ins.

| Field | Meaning |
|---|---|
| item | The item the words are in |
| entity | The person or place the words are taken to mean |
| span | Where the words start and end in the item's text, and the words themselves |
| state | `found` by reading, `confirmed` by the person, or `removed` |

"Not about {name}" sets `removed` and lets go the memories from that item filed under the entity;
Undo restores both. Two people who share a first name are never guessed between: reading keeps
no memory for that name and asks "Which {name} is this?" in Needs you.

## Journal grouping and versions

A good thing is a `text` item with `metadata.journal` set to `good`; an entry block is one with
`entry`, and a check-in one with `checkin`. Journal items carry a few more keys in `metadata`:

| Key | Meaning |
|---|---|
| `journal_day` | The day the item belongs to, by the day boundary, not the clock date |
| `block` | The block's order in the day's entry |
| `replaces` | The item an edit replaces |
| `about` | The note a feeling tapped in quick capture belongs to |

A block saved in today's entry is held for a short while before reading, so a half-written
sentence is not read; Read now, or the overnight tidy, ends the hold. Editing a block that was
never read changes it in place. Editing one that was read keeps the earlier version until it is
deleted, and reading treats the new version as a re-read through the same path: memories only the
earlier version supported are let go. An item can be left out of reflections with a flag on the
item; looking back never updates `last_seen`.

## Reflections and recaps

A reflection is the writing about one day, and a recap is the writing about a week, a month or a
year. Both are derived, kept per period, and never read as facts.

| Field | Meaning |
|---|---|
| period and kind | The day, week, month or year, and which it is |
| lines | The text of each line, in order |
| cited items | The ids of the items each line cites, so every line has a receipt |
| written | When, and whether on demand or overnight |

Only the day's reflection is written by the mind; counts, quotes, check-ins and mentions are
composed from what was kept. A line citing an item that is left out is hidden, and forgetting an
item removes the lines that cite it. Delete everything removes them, and export includes them as
text. Their lines carry `journal` sensitivity, they never go online, and citing them is not use.

## Search

SQLite FTS5 indexes each memory's sentence, subject and object, and each item's title, text and
summary twice: as typed, for search as the person types, and with the Porter stemmer, for
questions, so "plumbers" finds "plumber". Forgetting and Delete everything compact every one of
these indexes. A question is matched by its
words (BM25) and by its meaning (the recall model's vectors), and the two rankings are fused by
reciprocal rank. The recall model reads a question with the search instruction it was trained
with (Qwen3-Embedding) or as it is (any other), and the memories always as they are. On Standard and Large, a reranking model, when it is on disk, orders the first
twelve by how well each answers the question and keeps eight.

When reading missed a line of an item, that line can still answer: a line of a matching item that
no memory says is offered as a fact with the item as its receipt, in places the memories left
free. It is never offered from an item
that is private, untrusted or had something forgotten, or whose memories were let go, corrected or
replaced by a later change, and it never leaves the computer or reaches a phone.

A dated appointment that a note kept clearly later says has moved, when reading kept the move
but not the new day, reaches the answer as a move: in its place, the line of that note that says
where it went, with the old value as what it was and the note as its receipt. The line is the
move sentence or the one right after it and names the appointment or says "it". The note must be
about this appointment: filed under it or naming it, naming no other owner ("Mum's", "her",
another person's name, also as its first word), and saying nothing else ("Sam moved to Leeds" is
no move), and it is kept with the same care as the appointment (private, or a health, money or
ID detail). When more than one dated appointment is filed under the same name, found by the
search or not, the note could be about any of them and is no move. A phone gets the memory that
says it moved instead of the line, with the old value only when the old memory may go to a
phone. The old value never goes online: an online question gets the memory as it is kept, so the
Sent log shows exactly what left.

An answer never gives one side of an open which-is-right question as settled, or a move without
where it went. When it cites one side without the other's day or date and time, said in one
sentence about it (in any usual form), the other side is added after its own words and cited:
for an open question marked as the earlier or the later note, followed by "It is not settled
which is right" (also when the answer names both but picks one); for a move as the later note,
said to have since moved when the answer gave the old day. A side is added only with its
receipt. This runs on this computer for an online answer too: a side kept back from the provider
is added here and never sent or logged, and the answer is marked as having kept something here.

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

Forgetting is explicit and confirmed, and it is not undoable. The interface calls this Delete; letting go is the reversible path.

| Thing | Effect |
|---|---|
| The fact and its earlier versions | Removed |
| Derived text and indexes (FTS rows, vectors, cached audio) | Removed |
| The source event and its original file | Removed when the person chooses, the default when no other fact depends on it |
| Lines on past pages that cite the fact | Redacted to "(forgotten)" |
| Exports and printed pages already made | Out of reach; the confirmation says so |
