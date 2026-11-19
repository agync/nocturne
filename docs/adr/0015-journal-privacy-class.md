---
type: Decision
title: "0015 A journal privacy class"
description: "A third sensitivity, journal, keeps journal content out of audio, notifications, the tray and a phone's lock screen while the person can still read it everywhere else."
status: stable
tags: [adr, journal, privacy, sensitivity]
---

# 0015 A journal privacy class

Status: accepted. Date: 2026-11-19.

## Context

A journal holds what people least want read out or shown on a lock screen, but the owner wants it
readable everywhere the person reads, including the phone, and usable by Ask. A per-item private
switch was tried in the design and dropped: it hid most of the journal from the phone and asked
people to make a choice on every entry.

## Decision

Add a third sensitivity, `journal`, set on journal items and on every memory read from an entry or
a good thing. It is kept out of audio, notifications, the tray and a phone's lock screen, and
allowed in Print, export and the phone. Online answers applies only to memories read from entries
and good things. A memory takes the strictest sensitivity of its sources: private, then journal,
then normal. Check-ins write no memories, and check-ins, reflections and recaps are never in Ask's
search, so they never go online.

## Consequences

- One rule, no switch. Journal items offer no private mark.
- While Online answers is on, a journal sentence can leave the computer with a question, and the
  setup and settings screens say so ([decision 0011](0011-online-answers.md)).
- A lost phone can read the journal until it is removed.
- Editions and the notifier check `journal` the way they check `private`.
