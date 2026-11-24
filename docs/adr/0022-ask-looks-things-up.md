---
type: Decision
title: "0022 Ask looks things up"
description: "Accepted: every answer gets today's date, time and time zone; a fixed date step adds what was kept for the days a question names; and a mind may ask for read-only look-ups of the calendar, the journal, other words and health readings, up to 4 requests per question, each counted, amending 0011 and 0020."
status: stable
tags: [adr, ask, online, phone, receipts]
---

# 0022 Ask looks things up

Status: accepted by the owner on 2026-11-24, with the design
[Ask with tools](/design/ask-with-tools.md).

## Context

Ask found lines by the question's words only. "What's on tomorrow?" found nothing, since no kept
line says "tomorrow", and an online mind did not know today's date. On a phone alone the question
ended as "Nothing you've kept answers that." before anything was asked.

## Decision

- **Today in every answer.** Every answer request, local and online, ends its facts with today's
  date, time, time zone and the first day of the week, by the journal's day boundary.
- **The date step.** A fixed English reading of the question turns "tomorrow", "last Tuesday" or
  "this weekend" into days, and adds up to 12 lines kept for them (calendar, journal and check-ins,
  and health readings the question names) beside the words' lines. It needs no mind, so a phone
  with none, or a paused one, uses it too.
- **Read-only look-ups.** When a question names a time, or its words find fewer than 3 lines, the
  mind may ask for `calendar`, `journal`, `search` or `health`. Each returns only kept lines with
  their receipts; none writes, shares or reaches the network. At most 3 rounds, 4 requests, 8
  look-ups and 60 lines per question.
- **Each request counts.** Every part counts towards a monthly limit, which is looked at before
  each part. The Sent online list shows a question as one entry with its parts.
- **Never worse.** A mind that refuses look-ups, or asks for broken ones, gets the date step path,
  and that is remembered with its address and name.
- **Where it runs.** A phone alone, on its online mind, now. On the computer, Online answers waits
  for the pinned `lemond` to be confirmed to pass look-ups on, and the computer's own mind waits
  for a size to pass the look-up check; both use the date step path meanwhile.

## Consequences

- 0011 changes from "one request per question" to at most 4, each counted, and sends today's date.
- 0020's phone sends today's date and may look further in its own memory.
- Health readings and check-ins reach Ask, in the health wording and the person's own words
  ([Health from the phone](/design/phone-health.md), [Journaling](/design/journaling.md#11-ask)).
