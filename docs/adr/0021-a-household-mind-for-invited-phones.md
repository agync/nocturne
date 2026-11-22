---
type: Decision
title: "0021 A household mind for invited phones"
description: "Proposed: a computer running Nocturne can lend its mind to any phone holding the one household code its owner shows, as a mind only: typed calls with no handle on any store, nothing kept about any borrowing phone, a new code cutting off every phone holding the old one, in turn with the owner's own work, and never a way for a memory to land on that computer."
status: draft
tags: [adr, phone, household, remote, privacy, mind]
---

# 0021 A household mind for invited phones

Status: proposed. Date: 2026-11-22.

## Context

A household often has one computer that is always on, and people whose phones keep their own memory
([0018](0018-a-phone-as-its-own-home.md)). [0010](0010-away-from-home.md) lets only the owner's own
phones reach a computer. The owner decided that a household's mind serves the phones its owner lets
in as a mind only, that it keeps no record of them at all, and that a memory reaches another machine only when its person pairs to make
it their home. The design is [A household computer](/design/household-server.md).

## Decision

- **One household code.** The owner turns on Household and shows its code, as a QR code or typed.
  It carries the computer's address, its public key (so the phone knows it reaches that computer)
  and a secret. Any phone holding the current code may borrow the mind; each request proves it holds
  the code inside a sealed session. There is no per-phone pairing, Allow or three words for
  borrowing. "New code" replaces the code and cuts off every phone holding the old one. A phone
  cannot find a household or borrow without the code.
- **A mind only.** A borrowing phone has its own typed set of calls, shaped like Nocturne's own mind
  (read a thing, hear a recording, read a photo, answer from at most 12 lines, write a reflection or a
  headline), dispatched with no handle on any store. A companion call from it fails to decode.
- **Nothing kept.** The computer keeps its one current code and nothing about any borrowing phone:
  no name, key, list, log, cache or count. What a request carries is held in memory until the answer
  is sent. Recordings and photos are never written to a file. The owner's activity list has no line
  about a borrowing phone; the owner sees only "In use now" while a request is worked on.
- **Never a home by the back door.** No borrowing call writes anything for later. A household
  computer holds a person's memory only after a code for that person alone, their own pairing with
  three words and Allow, and a move, which a later record covers with the per-person store and its key.
- **Private stays home.** Things marked private never go to a borrowed mind. Check-ins do, sealed,
  so it can write reflections: the journal's rule ([0015](0015-journal-privacy-class.md)) is about
  online minds, and the owner confirmed this.
- **In turn.** The owner's own asks first, then household asks, then everyone's reading in turn, one
  thing at a time per connection. Taking turns needs only what is waiting now. The owner can turn
  lending off or make a new code.
- **Simple only, at first.** Household phones connect through Simple. Most private keeps admitting
  only the owner's own devices.
- **Any computer may host,** with no check of how strong it is. The desktop app hosts first; the
  server image follows when [0012](0012-server-mode.md)'s blockers close. The owner's companion phone
  can turn lending on or off, show the code and make a new one.
- **Invariant 2** gains the household clause proposed in
  [0018](0018-a-phone-as-its-own-home.md#proposed-contract-changes).

## Consequences

- The computer reads what it works on in its own memory, so someone with full control of it could
  look. The joining screen says so, and no wording anywhere promises more.
- A test proves a borrowing phone's requests leave the host's store and data folder unchanged.
- With nothing kept per phone, there is no daily share, pause or removal per phone. The trade-off is
  deliberate: to stop one phone, lost or passed on, the owner makes a new code and everyone else
  scans again.
- Anyone who gets a photo of the code can borrow the mind until a new code; the code never opens
  anyone's memory.
- 0010's rules for the owner's own phones are unchanged.
