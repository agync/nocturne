---
type: Overview
title: What Nocturne is
description: What Nocturne is, who it is for, scope and non-goals.
status: draft
tags: [overview, product]
---

# What Nocturne is

Nocturne is a local AI that remembers the day, forgets what does not matter, and prints a one-page
paper in the morning. It is a desktop app for people who are not technical about AI: one installer,
no account, no cloud, and nothing to configure before it is useful.

The bet is that memory should behave like a metabolism, not a landfill. Things stay strong when they
are used or confirmed, fade when they are not, and can always be brought back. The morning page is
the proof: it shows what changed, what needs you, what was let go, and asks one question.

## Who it is for

| Person | Day-to-day problem Nocturne solves |
|---|---|
| A parent running a household | School letters, appointments and promises scattered across photos, voice notes and chats |
| An older adult | Remembering what the doctor said, medication changes, what family members mentioned |
| A freelancer or small-business owner | Client details, decisions and follow-ups captured on the go |

## The four loops

| Loop | What happens | What the person sees |
|---|---|---|
| Capture | Text, a photo, a voice note or a page goes into one inbox | "Kept." |
| Consolidate | While the computer is idle, Nocturne reads, links, merges and lets go | Nothing, or a quiet status |
| Serve | A question gets a short answer with the originals as proof | An answer with its receipts |
| Publish | One page each morning | The morning page, on screen, aloud or printed |

Details are in [the concept](/design/concept.md).

## Scope

In scope:

- A desktop app for Linux and Windows first, macOS after.
- Capture of typed text, pasted text, images, voice recordings, audio files and documents.
- Local understanding through an embedded, private Lemonade instance.
- Plain-language questions with cited answers.
- The morning page on screen, as audio, and as print.
- A visible, reversible forgetting policy and explicit hard deletion.
- Export of everything in an open format.

Out of scope:

- Cloud processing, accounts, telemetry, or any network traffic other than consented model and app
  downloads.
- Multi-user memory and cross-device sync (a phone capture companion is a later stage).
- A general chat assistant. Questions are answered from what the person kept, and questions about
  using Nocturne from its own guide.

## Status

Pre-implementation. The first scaffold was a Python command line tool; it was retired in favour of a
desktop app ([decision 0001](/adr/0001-rewrite-as-desktop-app.md)). The product and interface design
is in progress, and implementation starts from that design.
