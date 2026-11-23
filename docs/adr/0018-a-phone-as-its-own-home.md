---
type: Decision
title: "0018 A phone as its own home"
description: "Proposed: the phone app keeps a person's memory with the same engine and store as the computer, with no mind on the phone at first, one home at a time, and a memory that moves only when its person pairs with a new home; with the proposed wording for the contract changes the phone parity records need."
status: draft
tags: [adr, phone, engine, privacy, data]
---

# 0018 A phone as its own home

Status: proposed. Date: 2026-11-22.

## Context

Today a phone is a companion: the computer holds the memory and the phone keeps, asks and reads the
page through the sealed channel. The design [Phone and computer as equal starting
points](/design/phone-parity.md) lets a person start on a phone with no computer. Its first step,
[Nocturne on a phone of its own](/design/phone-first.md), is a phone that keeps its own memory,
journal, check-ins, calendar and health, with no mind yet.

`nocturne-engine` holds the store and needs no mind: the caller reads items and hands results back
([0003](0003-sqlite-single-file.md), [0004](0004-open-engine.md)). It cross-builds for Android with
SQLite bundled. The phone app today links only the channel and the contract.

## Decision

- **The phone app links the engine.** A phone that is its own home opens the same store, schema and
  migrations as the computer, under a `store` folder of the app's data. Keeping, the journal,
  check-ins, finding by words, forgetting and fading, delete, export and backup run on the phone.
  The phone's own calendar and health readings go straight into that store through the engine's
  phone sources, as a source named for this phone.
- **No mind on the phone in the first release.** What needs reading waits, findable by its words.
  A mind comes later from an online service ([0020](0020-an-online-mind-on-a-phone.md)), a
  household ([0021](0021-a-household-mind-for-invited-phones.md)), or the phone itself.
- **One home holds the pen.** A memory lives in one place at a time. The home writes the store and
  runs its tidy; every other device reads it or keeps things waiting to go across.
- **A memory moves only by pairing.** The one way a memory comes to be held by another machine is
  its person pairing with that machine to make it their home: their own computer, or a household
  computer whose owner gave them a code to keep it there. Using a mind never moves a memory or leaves a
  copy of it on the mind's machine. A move uses the backup format of
  [0013](0013-passphrase-backup.md) over the sealed channel; the old home puts its memory aside
  once every count matches, and deletes it only when the person confirms.
- **One home is enforced at pairing.** A phone that holds a memory and pairs with a computer ends in
  a move, or in nothing changing. It never ends with two homes.
- **The device backup is out of scope.** The store stays out of iCloud Backup and Android's backup,
  as the app's files already do. The online backup is the locked copy of
  [0019](0019-a-locked-copy-online.md).

## Proposed contract changes

These change `AGENTS.md`. The owner approved them clause by clause: each clause goes into
`AGENTS.md` when the record it names is accepted, and not before. They are written here so the phone
parity records share one wording.

| Clause | In `AGENTS.md` |
|---|---|
| Invariant 2, an online mind on a phone (0020), the key route | **Yes**, since 2026-11-23, when the owner accepted 0020 |
| Invariant 2, Online answers and the online mind send what a good answer needs, private things, check-ins and originals included | **Yes**, since 2026-11-23 (owner): private decides only what comes up on its own |
| Invariant 2, Apple's servers as a phone's mind | No: later, with the Apple trial |
| Invariant 2, a locked copy online (0019) | No: 0019 is proposed |
| Invariant 2, a household's mind (0021) | No: 0021 is proposed |
| First line, invariants 4 and 5, the new invariant | No: they go in with this record |

**First line.** "Nocturne is a private app for the computer and the phone that remembers your day,
tidies its memory overnight, and hands you one page each morning. It is a Tauri 2 app (Rust core,
web-view interface); on a computer it ships and supervises a private, embedded Lemonade server."

**Invariant 2, Local only,** in full:

> **Local only.** No telemetry, no cloud calls, no network traffic other than consented model and
> app downloads; while the person has turned on Away from home, sealed traffic to their own paired
> phones through the connection they chose, plus that connection's own traffic to its company's
> servers and relays to stay connected; while the person has turned on Online answers, the embedded
> engine's requests to the one provider they chose for answers, online or on their own network (each
> question with what a good answer to it needs), plus one look at that provider's list of models
> each time the engine starts; while the person has turned on a locked copy online, copies of their
> memory locked with a key only they hold, to the one place they chose for it, plus the sign-in and
> listing that place needs (0019); while a phone that is its own home has an online mind the person
> chose, that phone's requests to the one provider they chose, online or on their own network (what
> each reading, page or answer needs to be done well), plus one look at that provider's list of
> models when it is checked (0020); and, while the person borrows a household's mind with the code its
> owner showed them, sealed traffic between their phone and that household's computer through the
> connection its owner chose, carrying only what the request in flight needs, plus, on that computer while its owner lends its mind, the same sealed traffic to
> the phones holding its current code (0021). A household computer keeps nothing about a phone that
> only borrows its mind. A memory is held by another machine only when its person pairs to make that
> machine its home. The phone app is held to the same rule. The embedded `lemond` binds
> loopback with a per-launch key.

A household computer as someone's home, entered by pairing, needs one more clause for that person's
sealed traffic; its own record adds it.

**Invariant 4, Receipts,** adds: "A thing waiting to go across has a receipt on the phone until its
home has it."

**Invariant 5, Forgetting,** adds: "A memory that moves is deleted from where it was only after it
has arrived and the person confirms. No one deletes another person's memory without that person
being told first."

**A new invariant:** "**One memory, one person, one home.** A memory belongs to one person and lives
in one place at a time. It moves only when its person pairs with a new home. Using a mind never moves
it. A copy is a locked copy of that home, never a second home."

## Consequences

- The phone app grows from a channel client to a home: it needs the engine, a local dispatcher for
  its screens, and the engine's tests on the phone targets. The iPhone build of the engine is not
  yet tried.
- The decision for native phone apps keeps its channel; a phone alone uses the channel only to
  move its memory or, later, to borrow a household's mind.
- The phone's offline queue for a companion moves from the web view into the store, so a keep that
  waits to go across is as durable as one kept at home.
- A lost phone loses a memory that has no copy. The first screens say so, and backups to a file ship
  with the first release.
