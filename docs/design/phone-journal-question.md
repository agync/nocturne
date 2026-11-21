---
type: Design
title: The journal question on the phone
description: The morning page's question from the journal on a paired phone, with where it sits, the three answers and the optional line, the receipt, answers from either device, waiting on the phone, and errors.
status: draft
tags: [proposal, ux, journal, phone, away-from-home, copy]
---

# The journal question on the phone

At most once a week the morning page asks one question from the journal: "Yesterday you wrote:
“Parents’ evening at six.” How did it go?" with Fine, Not great and Not yet. The rule, the
templates and the computer's screen are in [Journaling follow-ups](journaling-follow-ups.md#3-the-page-question-from-the-journal).
A paired phone reads the whole journal, and its page data already carries the question
([decision 10](journaling-follow-ups.md#decisions)), but the phone's page did not show it. This
page puts it there, answering into the same place on today's journal as the computer.

Status: proposed, with its build in the same change ([#113](https://github.com/agync/nocturne/issues/113)).

## Mockups

In `docs/design/mockups/journaling/`, with the same kit and rules as the
[journaling mockups](journaling-follow-ups.md#mockups): the phone is 390 by 844, `?state=` picks the
state and `&theme=light` or `&theme=dark` the theme.

| Screen | File and states | Renders |
|---|---|---|
| The question on the phone's page | `phone-page-question.html`: `ask`, `answered`, `added`, `elsewhere`, `waiting`, `failed` | `phone-page-question-light.png`, `phone-page-question-dark.png`, `phone-page-question-answered-light.png`, `phone-page-question-added-light.png`, `phone-page-question-added-dark.png`, `phone-page-question-elsewhere-light.png`, `phone-page-question-waiting-light.png`, `phone-page-question-failed-light.png` |
| The answer on the phone's journal | `phone-day.html`: `answer`, `answerWaiting` | `phone-day-answer-light.png`, `phone-day-answer-dark.png`, `phone-day-answer-waiting-light.png` |

The built screens are in `built/`: `followups-before-phone-page-*.png` is the phone's page before,
`followups-after-phone-*.png` after.

## Where it sits

On "Today’s page", after the page's lines and before the foot, as on the computer, where it
follows the lines. It is a box in the background colour on the paper, so it reads as one thing to
answer, not as another line. The page scrolls to it; nothing jumps to it.

Only a question from the journal shows. The page-time question and the yes or no question stay off
the phone, as before.

## What shows

| Part | What shows |
|---|---|
| Label | "One question · from your journal" |
| Question | The template, the quote in italics, as on the computer. Once answered it is set smaller in the secondary ink |
| Receipt | "Entry · {short day} · {time}", or "Voice note · {duration}", in the page's receipt style. It opens the journal day the block was kept on |
| Choices | "Fine", "Not great", "Not yet": three buttons in one row, 48 points tall |
| Answered | "{choice}. Kept on today’s journal." then "Anything to add? (optional)", a field with the placeholder "A line, if you like.", and "Add" beside it, off until something is typed |
| Added | "{choice}. Kept on today’s journal." and "“{line}” Added." with "Open today’s journal ›", which opens the phone's journal on today |

## Answering

- **One press keeps it.** Pressing a choice keeps the answer at once, as on the computer, with the
  same storage: a journal item on today, marked as an answer, with the choice and a link to the
  block. Fine and Not great count as use of the memory; Not yet changes nothing.
- **The line.** Optional, once, and only while the page is the latest, as on the computer. It is
  read like an entry. On an earlier page the choices still work and the field is not shown.
- **Once, however often it is sent.** Each answer carries a fresh id from the phone, as a keep
  does. A retry, or an answer that waited and is sent again, finds the first one and keeps nothing
  more. A line goes on once, so a retried line finds itself there.
- **From the phone.** On the computer's timeline the answer carries "from {phone name}", as
  anything kept from a phone does.

## On either device

The phone reads the page each time it is opened, so an answer given on the computer shows there as
the answered or added state, with the computer's choice and line.

If the page was open on both and the computer answered first, the phone's press keeps nothing and
shows what the computer kept. When the computer's choice differs from the one pressed, "Already
answered, on your computer or another phone." goes under it; the same choice needs no note.
A line works the same way: one line, whichever device adds it first. An answer from the phone
reaches the computer's page and journal at once, and Away from home's activity list says "{name}
answered a question on the page", never what the answer was.

## Waiting on the phone

When the computer cannot be reached, a pressed choice waits on the phone like a keep:
"{choice}. Waiting on this phone." and "It’s on this phone and goes across when your computer is
back. You can leave this page." The field stays; a line added now waits with the answer, as
"“{line}” Waiting on this phone.", and goes across with it. When the computer is back the answer goes across first thing, like anything else
waiting, and the page shows the answered state.

It counts in "{count} waiting" on Home and is listed under "Waiting on this phone" on the phone's
journal as "{choice}" in medium weight, the line, and "waiting on this phone".

If the computer answered in the meantime, the waiting answer keeps nothing, and a line written for
it is not added to the computer's answer when the choices differ. The page shows the computer's
answer with "Already answered, on your computer or another phone." A waiting answer whose question has
gone, because its memory or block was deleted, is let go and never holds up what waits behind it.

## When it goes wrong

| What happened | What shows |
|---|---|
| The computer refused the answer | "That didn’t go across." and "Nothing was kept. Try again in a moment." under the choices, which stay |
| The line did not go across | The same two lines under the field, which keeps what was typed |
| The page was replaced before the line went across | The field goes, and "A new page has arrived, so this answer stays as it is." |
| The question is gone (its memory or block was deleted) | The question is not on the page; nothing else changes |

## The answer on the phone's journal

The answer is a row of today's timeline on the phone, as on the computer: the time, the choice in
medium weight, the line, and under it "About: “{sentence}”" in the secondary ink. It carries "from
{phone name}" when it was answered on a phone. Without "About" once the block is deleted.

## Never aloud, never in a notification

The phone shows journal content in full, and never reads it aloud, notifies it or puts it on the
lock screen. The phone app has no notifications and no reading aloud, and the question adds none:
the answer, the line and the question's words appear only on the page and in the journal.

## Copy

Every string the computer already has is reused as it is.

| Key | Copy | Source |
|---|---|---|
| Label | "One question · from your journal" | Computer |
| Templates | As in [Templates](journaling-follow-ups.md#templates) | Computer |
| Receipt | "Entry · {day} · {time}" / "Voice note · {duration}" | Computer |
| Choices | "Fine", "Not great", "Not yet" | Computer |
| Answered | "{choice}. Kept on today’s journal." | Computer |
| Add label | "Anything to add? (optional)" | Computer |
| Add placeholder | "A line, if you like." | Computer |
| Add button | "Add" | Computer |
| Added | "“{line}” Added." | Computer |
| Open | "Open today’s journal ›" | Computer |
| About | "About: “{sentence}”" | Computer |
| Waiting | "{choice}. Waiting on this phone." | New |
| Waiting body | "It’s on this phone and goes across when your computer is back. You can leave this page." | Phone keep |
| Line waiting | "“{line}” Waiting on this phone." | New |
| Elsewhere | "Already answered, on your computer or another phone." | New |
| Failed | "That didn’t go across." / "Nothing was kept. Try again in a moment." | Phone keep |
| Replaced | "A new page has arrived, so this answer stays as it is." | New |
| Waiting tag | "waiting on this phone" | Phone journal |
| From a phone | "from {phone}" | Phone journal |
| Activity, on the computer | "{name} answered {count, plural, one {a question on the page} other {# questions on the page}}" | New |

## Testing

- The engine: an answer from a phone keeps one item however often it is sent; an answer already
  given elsewhere keeps nothing; a line goes on once and only with the choice it was written for.
- The remote call: the phone's answer reaches today's journal through the same path, a retry keeps
  one answer, and only a question from the journal can be answered.
- The phone: each state above, waiting and sending again, and the receipt opening the journal day.

## Changes to other pages

| Page | Change |
|---|---|
| [Journaling follow-ups](journaling-follow-ups.md) | Item 3's "Read aloud and the phone" and follow-up 4 point here |
| [Journaling](journaling.md) | Item 12, the phone: the table gains the page's question, and the name question is no longer the one change beyond keeping |
| [Remote access](../architecture/remote.md) | The phone's calls include answering the page's question from the journal |
