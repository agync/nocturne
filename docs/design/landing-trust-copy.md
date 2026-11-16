---
type: Design
title: Landing page, trust and timing copy
description: Approved words for the landing page that answer three doubts from a first reader, any sleeping hours, worry that it gets things wrong, and fear of having to keep it tidy.
status: stable
tags: [approved, copy, site, landing]
---

# Landing page, trust and timing copy

A reader new to Nocturne read the landing page and found it clear and attractive. Three doubts were
left, and each comes from what the page says, not from how Nocturne behaves:

| Doubt | What Nocturne does | Where the page misleads |
|---|---|---|
| Does it only work for people who sleep at night? | It tidies whenever the computer is idle and on power, at any hour. A laptop that sleeps catches up when it wakes. The person sets the page time at setup and in Settings. | "Overnight", "07:00" and "the overnight tidy" read as fixed. |
| Will it get things wrong or mess something up? | What you keep is never edited. Every answer shows the originals. When two things disagree it asks. A wrong answer can be corrected, and the old version is kept, not overwritten. Forgetting can be undone; deleting asks first. | The guarantees are spread over three sections, and none is next to where answers appear. |
| Will it sort things my way, or will I end up supervising it? | It files each memory under a person, place or thing by itself. The person never has to keep it tidy, and can rename, merge or move anything if they want to. | "No folders, no tags" sits mid-paragraph, and the reader still pictured a sorting assistant to supervise. |

The name, the heading "Wake to what needs you." and the overnight feeling stay. The changes say what
is true for every schedule and put the reassurance where the doubt arises.

## Proposed changes

All in `site/index.html`. Words only: no layout, image or script changes.

### 1. The morning page card

| | Words |
|---|---|
| Eyebrow, now | The morning page · 07:00 |
| Eyebrow, proposed | The morning page · at your time |
| Body, now | Overnight, while the computer is plugged in and idle, Nocturne reads what you kept and notices what changed. In the morning you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. |
| Body, proposed | Whenever the computer is plugged in and idle, by night or by day, Nocturne reads what you kept and notices what changed. At the time you choose, you get a short list to read, hear or print. When two things disagree, it asks instead of guessing. |

### 2. Keep anything, file nothing

| | Words |
|---|---|
| Body, now | Press Ctrl Shift Space, or Cmd Shift Space on a Mac, then type, talk, paste or drop a photo or a PDF. No folders, no tags. Nocturne says “Kept.” and you get back to your day. If it cannot read something, it tells you what to do. |
| Body, proposed | Press Ctrl Shift Space, or Cmd Shift Space on a Mac, then type, talk, paste or drop a photo or a PDF. Nocturne says “Kept.” and you get back to your day. No folders or tags to keep up: you ask, and it finds it. If it cannot read something, it tells you what to do. |

### 3. Answers with receipts

| | Words |
|---|---|
| Body, now | Ask in plain words. Nocturne answers only from what you kept, with the originals underneath so you can check. When it does not know, it says so. |
| Body, proposed | Ask in plain words. Nocturne answers only from what you kept, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. |

### 4. A new promise in "What it promises"

Added after "Checked, not just claimed", before "It forgets on purpose". "Never edits" rather than
"stays exactly as kept", so it does not seem to contradict the fading row below it.

| | Words |
|---|---|
| Term | It never edits what you kept |
| Detail | Your notes, photos and files are never changed. Every answer shows them underneath. If one is wrong, tell it what is right; the old version is kept, not overwritten. |

### 5. "The overnight tidy" in Requirements

Two places, so the page does not argue with change 1.

| Where | Now | Proposed |
|---|---|---|
| An AI chip | …so answers and the overnight tidy do not wait for them. | …so answers and the daily tidy do not wait for them. |
| Small | Same page. The overnight tidy takes longer. | Same page. The daily tidy takes longer. |

### 6. The lede

The worry about supervising the sorting can be met before the first scroll. Three words added to the
lede, and to the two meta descriptions that repeat it.

| | Words |
|---|---|
| Now | Save notes, photos, voice notes and files without sorting them. |
| Proposed | Save notes, photos, voice notes and files without sorting them, now or later. |

## Not changed

- The hero heading, the name, the look and the heading "Wake to what needs you."
- The engine and download sections.

## Review

A designer reviewed the first draft. They found an overclaim: the draft said there is no filing.
Nocturne does file each memory under a person, place or thing, and the person can change it.
Changes 2 and 4 were reworded, and changes 3, 5 and 6 come from that review.

## Approval

The owner approved all six changes, including the optional lede, as written on 2026-11-16, and
they are on the landing page.
