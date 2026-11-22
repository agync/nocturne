---
type: Decision
title: "0019 A locked copy online"
description: "Proposed: a phone that is its own home can keep numbered copies of its memory, locked with a key only the person holds, in the one place they chose, as its online backup; opt-in, off by default, never a second home."
status: draft
tags: [adr, phone, backup, privacy, online]
---

# 0019 A locked copy online

Status: proposed. Date: 2026-11-22.

## Context

A phone that is its own home ([0018](0018-a-phone-as-its-own-home.md)) loses its memory with the
phone. [0013](0013-passphrase-backup.md) seals a backup with a passphrase into a file the person
chose, and asks to be revisited when a backup is kept somewhere else. The owner asked for an opt-in
copy online and ruled out two-way sync. The design is [A locked copy
online](/design/phone-first.md#a-locked-copy-online).

## Decision

- **Opt-in, off by default, one place.** Turned on only after its own consent steps. The copy goes to
  iCloud Drive on iPhone, the app's hidden folder in Google Drive on Android (asking Google for that
  folder only), or a storage address the person runs, as an advanced choice.
- **Locked with a key only the person holds.** Copies use 0013's format. The key that seals them is
  itself sealed with a passphrase the person chooses once; the key is kept in the phone's own safe
  place so later copies need no passphrase. Neither the storage company nor Nocturne's makers can
  open a copy.
- **Daily by default.** A copy is made every day; the person can choose every week or every month.
- **Numbered and never overwritten.** Each copy is new; photos and recordings go once each, locked.
  The last seven copies are kept. Forgetting for good prunes older copies at once, so forgetting
  reaches the copy.
- **A copy of the one home, never a home.** It is used to restore on a new phone and nothing else.
  Only the phone in use writes copies; a newer copy from another phone stops this one, so nothing is
  overwritten or merged.
- **It is the online backup.** The phone's own device backup is out of scope, and the memory stays
  out of it.
- **When the memory moves to a computer,** the copy stops, and the person chooses to delete it or
  keep it as it was.
- **Invariant 2** gains the locked copy clause proposed in
  [0018](0018-a-phone-as-its-own-home.md#proposed-contract-changes).

## Consequences

- 0013's "revisit" is met: the format is unchanged, only where it is kept.
- An Apple developer account with iCloud, and a Google sign-in limited to the app folder, become
  part of a release.
- A lost passphrase means a lost copy. The screens say so in 0013's words.
