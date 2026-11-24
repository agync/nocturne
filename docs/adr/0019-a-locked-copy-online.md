---
type: Decision
title: "0019 A locked copy online"
description: "Accepted: a phone that is its own home backs up its memory each night while it charges, in locked pieces, to the person's own iCloud or Google Drive, opened with a phrase of words only they hold; opt-in, off by default, never a second home, with no server of Nocturne's."
status: stable
tags: [adr, phone, backup, privacy, online]
---

# 0019 A locked copy online

Status: accepted by the owner on 2026-11-24. Proposed 2026-11-22. Revised 2026-11-23 (owner): a
nightly backup to the person's own account, replacing both the first proposal and the same day's
file in a picked folder. Revised 2026-11-24 (owner): restoring onto a phone that holds a memory
brings the backup's things in.

## Context

A phone that is its own home ([0018](0018-a-phone-as-its-own-home.md)) loses its memory with the
phone. [0013](0013-passphrase-backup.md) seals a backup with a passphrase into a file the person
chose, and asks to be revisited when a backup is kept somewhere else. The owner asked for an opt-in
copy online, ruled out two-way sync, and chose a backup that works like a messaging app's: automatic,
to the account the phone already has, with no server of Nocturne's. A file in a folder the person
picks was considered the same day and dropped: Google Drive's app may not offer folders to other
apps, and a backup that needs the person to set up a folder is not automatic. The design is
[Backup](/design/phone-first.md#backup).

## Decision

- **Opt-in, off by default, the phone's own account.** Turned on only after its own consent steps.
  On iPhone, the app's own iCloud container in the person's iCloud. On Android, a "Nocturne backup"
  folder in the person's Google Drive, reached through the phone's Google account picker with the
  narrow scope for files the app creates (`drive.file`), so Nocturne can reach nothing else there.
- **Nightly and incremental.** Each night while the phone charges, at a time the phone's system
  chooses, Nocturne writes what changed as locked pieces, each named by a hash of its locked
  contents, then a small locked index of the night. Only pieces the account lacks are sent. "Wi-Fi
  only" is a setting, off by default. "Back up now" runs one at once.
- **Locked with a phrase only the person holds.** Pieces and indexes use 0013's sealing. One key
  locks them; it is kept locked with the person's phrase and, if they made one, with a recovery key.
  Nocturne offers a random phrase of six everyday words by default; the person may type their own
  and is warned only when it is easy to guess, with no rules about symbols, capitals or length. The
  recovery key is offered once, to print or save, and can be made later in This phone. The key itself is kept in the phone's own safe
  place, so nightly backups need no phrase. Neither the account's company nor Nocturne's makers can
  open a backup.
- **Seven nights, and forgetting reaches it.** Seven nightly indexes are kept; a piece no kept index
  lists is removed. Forgetting for good writes a new index at once and removes every older index
  that lists the thing, with its pieces, also while backup is off and the backup kept. Delete
  everything deletes the backup in the account too, or the next time the phone reaches it, and
  turns backup off. Things marked private are backed up like everything else.
- **A backup of the one home, never a home.** It is used only to restore, and nothing opens or
  edits it in place. On an empty phone it comes back as it was. On a phone that already holds a
  memory, restoring brings the backup's things in, as a computer that holds a memory brings a
  phone's in: nothing there is put aside or deleted, and a thing on both is kept once. Each index
  names, unlocked, a random number for the phone that wrote it and a count; a phone that finds a
  newer index from another phone stops, so nightly backups never overwrite or merge.
- **Export is separate.** Saving everything as one 0013 file stays its own action.
- **It is the online backup.** The phone's own device backup is out of scope, and the memory stays
  out of it.
- **When the memory moves to a computer,** backup stops, and the person chooses to delete the
  backup or keep it as it was.
- **Storage sign-ins on the computer, WebDAV or OAuth, stay deferred** to a later record.

## Invariant 2

Nocturne itself now sends the backup, so invariant 2 gains this clause, written here and in
[0018](0018-a-phone-as-its-own-home.md#proposed-contract-changes). It is in `AGENTS.md` since this
record was accepted. Since 2026-11-24 (owner) it names what may still go once backup is off:

> while a phone that is its own home has backup turned on, that phone's nightly copy of its
> memory, locked with a phrase only the person holds, to the person's own iCloud or Google Drive
> account that the phone is signed in to, plus the sign-in, listing and removal that account
> needs; after the person turns backup off or deletes everything, only the removals they asked
> for in that account (a forget for good, or deleting the backup), with the same sign-in and
> listing (0019)

## Consequences

- 0013's "revisit" is met: the sealing is unchanged; where the backup is kept, and its pieces, are
  new.
- A release needs the iCloud capability on Nocturne's Apple developer account, and an OAuth client
  for the app registered with Google Cloud, with Google's review of the `drive.file` scope if it
  asks for one.
- A lost phrase with no recovery key means a lost backup. The screens say so.
- The phone's network check gains a case: with backup on, the app reaches only the account's own
  hosts, and only while a backup runs or is restored.
