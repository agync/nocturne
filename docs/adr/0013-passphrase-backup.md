---
type: Decision
title: "0013 Back up and restore with a passphrase"
description: "One file holds everything needed to restore, locked with a passphrase the person sets, and a restore never deletes what is already here."
status: draft
tags: [adr, backup, privacy, data]
---

# 0013 Back up and restore with a passphrase

## Context

Export everything writes readable files and never holds a key, so it cannot be restored and it is
not safe to leave on a shared drive. A person who changes computer, or whose disk fails, loses
everything. They need one file they can put anywhere, that nobody can read without a passphrase
they chose, and that Nocturne can restore from. The screens are in
[Back up and restore](../design/backup-and-restore.md).

## Decision

- **One `.nocturne` file, locked with the age format.** The file is an
  [age](https://age-encryption.org/v1) stream with a passphrase recipient (scrypt), made by the
  `age` crate. Inside is a plain `tar` archive. age is small, standard, reviewed, authenticated and
  streaming: a wrong passphrase, a damaged byte and a cut-off file are all detected, and nothing is
  held in memory. It is also not ours: a person with the passphrase can open the file with any age
  tool and a tar tool even if Nocturne is gone. We do not invent a format. scrypt makes each guess
  cost about a second, so the passphrase is the only protection and a long one matters. The
  passphrase is normalised (NFC) before use so the same words typed on another system open the
  file. The archive holds no extra compression because originals are already compressed. The word
  in the interface is "passphrase"; "locked" is the word for the protection.
- **What goes in** (archive entries, in this order):
  - `manifest.json`: the backup format, the data schema version, the Nocturne version that made
    it, when it was made, the counts (memories, pages, originals) and whether Online answers and
    Away from home had been used. It sits inside the locked stream, so it is readable only after
    the passphrase opens the file. Before the passphrase the app checks only that the file is a
    passphrase-locked age file.
  - `store/nocturne.db`: a consistent snapshot taken with `VACUUM INTO` on a read connection while
    the app runs. It holds memories, items, entities, pages, questions and settings. Freed pages
    are not copied, so what was forgotten is not in it. Things marked private are in it, locked
    like everything else.
  - `store/originals/<sha256>`: every original, streamed from disk one at a time. The stored
    extension stays out of the archive name and comes back from the database when the file is put
    in place, so any name the person's filesystem allowed survives the round trip.
  - `engine/selection.json`: the reading setup the person chose.
- **What stays out, and why.**
  - Downloaded models and the engine's files: they can be fetched again, and they are large.
    Setup downloads them after a restore.
  - Spoken page audio and logs: made again, or hold only counts.
  - The Online answers key, in the keychain or in `online/key`: a secret. Online answers is off
    after a restore, its address and model name come back with the settings, and the person pastes
    the key again.
  - The engine's keys and the server sign-in token: made for this install.
  - Away from home: the computer's own key and the pairing secrets stay out. In the backup every
    paired phone is marked removed and its stored key is emptied, Away from home is off, and the
    person adds the phones again. The record of what each phone kept stays, so receipts still name
    it.
  - Everything the person is in the middle of forgetting: those originals are left out and the
    forget carries on after a restore, so a restore never brings back what was let go for good.
  - What an earlier restore put aside.
- **Writing.** The person sets the passphrase first (at least 8 characters, typed twice, with a
  strength hint and a warning that nobody can recover a forgotten one), then chooses the file in
  the system save dialog. The file is written as `<name>.partial` in the same folder, fsynced, then
  **read back and verified** (decrypted, every entry and every hash compared with what was
  written) before it is renamed to its real name and called saved. Cancelling or any failure
  removes the partial file. A backup that could not be read back is removed and reported as a
  failure. The scratch snapshot that exists while it runs is in the store folder, owner-only, and
  is overwritten and removed at the end or at the next start. A crash leaves a note naming the
  partial file, which the next start removes.
- **Restoring takes two passes and one swap.**
  1. The file is checked in full without keeping anything but a scratch copy of the database: the
     passphrase, the format, the data schema version, the database's integrity check, the hash of
     every original against its name. Only the entry names above are read, each as a plain file;
     any other entry, link, long-name or extended header is refused rather than followed. A file
     that asks for more passphrase work than a backup is made with, within a margin, is refused
     before the work is done. Nothing already here is touched.
  2. On the person's say, the backup is unpacked into a staging folder inside the data folder,
     with progress and the chance to stop. The file is read again in full, and one that no longer
     matches what the check showed is refused. Stopping or failing removes the staging folder.
  3. Then the swap is scheduled with a marker and the app closes and opens again. The swap runs
     at the start, before the store is opened, because the running app holds the store, the engine
     and the phone connection open. It is renames only, and it can run again after a crash at any
     step.
- **The rule for existing data.** If this install has nothing in it (no items, memories, entities,
  pages, questions or settings), the backup is restored and nothing is kept aside. If it has
  anything, the person sees both sides and must press "Replace with the backup". Before the swap
  the whole current store (database, originals and reading setup) is moved, not copied and not
  deleted, to `put-aside/<day and time>/` in the data folder, with a small note of its counts.
  Privacy lists each put-aside copy with "Bring it back" and "Delete it". Bringing one back puts
  the current data aside in turn, so nothing is ever lost by choosing either. A put-aside copy
  never expires and Nocturne never removes one by itself; only "Delete it", after it asks, and
  Delete everything do. Merging two stores is not offered: ids, forgets and the letting-go clock do
  not combine safely.
- **Newer and older.** A backup made by a Nocturne whose data schema or backup format is newer
  than this one is refused with a plain sentence that ends "Update Nocturne on this computer, then
  try again", and nothing changes. An older data schema is brought up to date in the scratch copy
  with the existing migrations before the backup is accepted, and again in the staged store.
- **Wrong passphrase.** Said plainly and nothing about the file is shown. A file that is not a
  backup, a damaged or cut-short backup, a drive that went away and a drive that is full each get
  their own sentence. Nothing is written for any of them beyond the staging folder, which is
  removed.
- **Where it works.** The desktop app. The browser and server modes do not offer it, because the
  file dialogs and the app restart are the desktop's.
- **Progress and stop.** Writing, reading back, checking and unpacking each report bytes done of
  bytes total and can be stopped, up to the final switch, which cannot.

## Why

The person owns the passphrase; there is no account or recovery and no network. A standard format
means the file outlives the app. Staging and swapping at start keeps the running store, the engine
and every other open file out of the swap. Reading the file back before calling it saved means the
person is not told "saved" about a file that cannot be restored. Keeping the old store aside rather
than deleting it follows the rule that forgetting is reversible and deletion is explicit.

## Residual risks

- A guessable passphrase can be guessed offline against a copy of the file. The length floor and
  the hint reduce this; they do not remove it.
- While a backup or a check runs, a readable copy of the database is in the data folder, which only
  the person can open, and it is overwritten when done. A crash can leave it until the next start.
- A put-aside copy is readable by anyone who can read the data folder, like the store it came from,
  until the person deletes it or deletes everything.
- Not built yet, and noted for follow-up: a check of free space before saving or restoring (a full
  drive is reported when it happens), and the tray entry and quit confirmation while a backup runs.

## Revisit when

A person needs a backup on a schedule, needs to merge two computers, or wants the backup kept
somewhere other than a file they chose.
