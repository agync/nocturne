---
type: Decision
title: "0021 A household mind for invited phones"
description: "Proposed: a computer running Nocturne can lend its mind to phones its owner invites, as a mind only: typed calls with no handle on any store, nothing kept of a borrowing phone beyond its name and key, in turn with the owner's own work, and never a way for a memory to land on that computer."
status: draft
tags: [adr, phone, household, remote, privacy, mind]
---

# 0021 A household mind for invited phones

Status: proposed. Date: 2026-11-22.

## Context

A household often has one computer that is always on, and people whose phones keep their own memory
([0018](0018-a-phone-as-its-own-home.md)). [0010](0010-away-from-home.md) lets only the owner's own
phones reach a computer. The owner decided that a household's mind serves the phones its owner
invites as a mind only, and that a memory reaches another machine only when its person pairs to make
it their home. The design is [A household computer](/design/household-server.md).

## Decision

- **By invitation only.** The owner turns on Household and makes a code for one person, choosing how
  the computer helps them. A phone cannot find a household or ask to join without that code. Pairing
  is 0010's protocol; the invitation's way travels in the sealed pairing message, shows on Allow, and
  fixes the phone's role.
- **A mind only.** A borrowing phone has its own typed set of calls, shaped like Nocturne's own mind
  (read a thing, hear a recording, read a photo, answer from at most 12 lines, write a reflection or a
  headline), dispatched with no handle on any store. A companion call from it fails to decode.
- **Nothing kept.** The computer keeps the phone's name, its public key and whether it is paused, so
  it can let it in, and deletes them when the phone leaves or is removed. What a request carries is
  held in memory until the answer is sent. Recordings and photos are never written to a file.
  Nothing is logged, cached or counted per phone, and the owner's activity list has no line about it;
  the owner sees who is invited and a live mark while a request is worked on.
- **Never a home by the back door.** No borrowing call writes anything for later. A household
  computer holds a person's memory only after a separate invitation to keep it there and a move by
  pairing, which a later record covers with the per-person store and its key.
- **Private stays home.** Things marked private never go to a borrowed mind. Check-ins may, sealed,
  so it can write reflections: the journal's rule ([0015](0015-journal-privacy-class.md)) is about
  online minds (to confirm with the owner).
- **In turn.** The owner's own asks first, then household asks, then everyone's reading in turn, one
  thing at a time per phone. Taking turns needs only what is waiting now. The owner can pause a phone.
- **Simple only, at first.** Household phones connect through Simple. Most private keeps admitting
  only the owner's own devices.
- **The desktop app hosts first;** the server image follows when [0012](0012-server-mode.md)'s
  blockers close.
- **Invariant 2** gains the household clause proposed in
  [0018](0018-a-phone-as-its-own-home.md#proposed-contract-changes).

## Consequences

- The computer reads what it works on in its own memory, so someone with full control of it could
  look. The joining screen says so, and no wording anywhere promises more.
- A test proves a borrowing phone's requests leave the host's store and data folder unchanged.
- With no counts per phone, there is no daily share per phone; pausing is the owner's lever.
- 0010's rules for the owner's own phones are unchanged.
