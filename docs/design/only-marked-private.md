---
type: Design
title: Only what you mark private is held back
description: Nocturne no longer decides that something is about health, money or ID. The switch, its copy, the hint to mark such things private and the questions' skips go; only what the person marks private is held back.
status: stable
tags: [approved, privacy, online-answers, journal, memories, copy]
---

# Only what you mark private is held back

The owner's rule, 2026-11-22: never judge a person's data, and apply it to what Nocturne already
does. Nocturne must not decide on the person's behalf that something is health, money, ID, work or
private. Only what the person marks private is held back.

Until now a word list and a few labels decided that a memory was "a health, money or ID detail".
Online answers kept such memories here unless the person turned a switch on, the memories list
suggested marking them private, and the morning page never asked about them or about a medicine,
allergy or health memory. Each of these was Nocturne judging what the person kept. This page
removes all of it. If something is private, the person says so by marking it.

## What stays

| Rule | Why it stays |
|---|---|
| Things marked private are never sent online and never reach a phone | The person decided it |
| Check-ins, reflections and recaps never go online ([decision 0015](../adr/0015-journal-privacy-class.md)) | A fixed product rule about kinds of writing, not a judgement about what any of it says |
| A memory being forgotten or let go is never sent | The person let it go |
| Medication, allergy and health facts are kept forever by default ([decision 0007](../adr/0007-forgetting-by-default.md)) | It keeps something, it never holds anything back, and the person can change it |

## What changes

| Where | Before | After |
|---|---|---|
| Online answers, which memories may go | Health, money and ID memories stayed here unless the person turned on a switch, off by default | Every memory that answers the question may go, unless it is marked private, being forgotten or let go |
| Online answers in Settings | A row "Health, money and ID details" with a switch | The row is gone |
| Online answers setup, step 3 | The same switch | The switch is gone; "Private things are never sent" stays |
| Online answers setup, step 5, "Your choices" | A row "Health, money and ID" with "Kept here" or "May be sent" and Change | The row is gone |
| An answer written online | Could say "Some health, money or ID details were left out" | Only says what was marked private was left out |
| What Nocturne remembers | A memory with health, money or ID words showed "This looks like health, money or ID information. You may want to mark it private." | No such line |
| The morning page's question from the journal | Never asked about a memory or sentence the check flagged, or a safety memory (one that says someone takes something, or labelled health, allergy or medication) | Asked about any memory the rule allows; private still never qualifies |
| The morning page's daily question ("Is this still true?") | Never asked about a safety memory | Asked about like any other memory. A memory kept forever is still not asked about, whatever it says; a safety memory is kept forever by default, so it is asked about once the person turns that off |
| The overnight tidy | Asked the mind, in shadow, whether each new memory was a health, money or ID detail | Not asked; the other two shadow questions stay ([decision 0016](../adr/0016-decision-models-shadow-first.md)) |

## Copy

Removed: every string that names health, money or ID details, and the hint on a memory.

| Id | Words removed |
|---|---|
| `online.row.sensitive.label`, `online.send.sensitive.label` | Health, money and ID details |
| `online.row.sensitive.off` | Off: they stay on this computer, even when they’d help an answer. |
| `online.row.sensitive.on` | On: these lines may go with a question they help answer. |
| `online.send.sensitive.hint` | Nocturne keeps a line here when it is about health or medicine, or holds what looks like a card or bank number, a long number, a passport or ID number, a password or PIN, or an amount you earn or owe. It can miss some, so mark anything else private. Turn on to let those lines go with a question they help answer. |
| `online.consent.aside.sensitive` | Health, money and ID |
| `online.consent.aside.keptHere`, `online.consent.aside.mayGo` | Kept here, May be sent |
| `ask.leftOut.sensitive` | Some health, money or ID details were left out. Ask on this computer to include them. |
| `ask.leftOut.both` | Some things you marked private, and some health, money or ID details, were left out. Ask on this computer to include them. |
| `memory.suggestPrivate` | This looks like health, money or ID information. You may want to mark it private. |

The advice to mark things private stays, because marking is now the only way to keep something
back. One sentence says it wherever the health, money and ID line was:

| Where | Before | After |
|---|---|---|
| Settings, "Always stays here", a note under the list (`online.stays.markPrivate`) | Health, money and ID details stay here unless you choose otherwise. Nocturne can miss some, so mark anything you never want sent as private. | Nocturne doesn’t guess what is private. Mark anything you never want sent as private. |
| Step 1, behind "Everything that is sent, and what stays here", Never sent (`online.explain.never.body`) | … Nocturne also keeps back lines that look like health, money or ID details unless you choose otherwise. It can miss some, so mark anything you never want sent as private. … | … Nocturne doesn’t guess what is private, so mark anything you never want sent as private. … |
| Step 5, Never sent, a note under the list (`online.consent.never.markPrivate`) | A list line: Nocturne leaves out lines that look like health, money or ID details, unless you chose to send them. It can miss some, so keep anything you never want sent marked private. | Nocturne doesn’t guess what is private. Mark anything you never want sent as private. |

The full step 1 Never sent paragraph after the change:

> Photos, recordings and documents stay here. Things you marked private are never sent. Your
> check-ins, reflections and recaps are never sent: they are written and kept on this computer.
> Nocturne doesn’t guess what is private, so mark anything you never want sent as private. Journal
> entries and good things can’t be marked private, so while this is on, a line from one can go
> with a question it helps answer.

On step 5 the note sits under the Never sent list, before the line about journal entries and good
things.

An answer from an earlier version that says something other than "private" was left out reads
"Some things were left out. Ask on this computer to include them." (`ask.leftOut.other`). Nothing
this version writes says so.

## A notice for people who already use Online answers

People who turned Online answers on before this change agreed to words that said health, money and
ID details stay here. Before their first question goes online after the change, Nocturne tells them
once what changed, and nothing is sent until they have seen it. They can see it wherever they ask,
on the computer or on a phone, and in Settings, so a person who mostly uses a phone, or who goes to
Settings first, can clear it there.

### When it shows

| When | What happens |
|---|---|
| Online answers is on, and the person agreed before this change | The next question that would go online waits: in place of the answer, Ask shows the notice below. The question is not sent |
| They press Continue | The notice is never shown again, here or on a phone, and the question that waited goes on its way as usual |
| They press "Answer on this computer" | The question that waited is answered on this computer, as when they choose this computer for a question. Nothing is sent. The notice comes back with the next question that would go online, until Continue is pressed |
| They press "Online answers settings" | Online answers opens in Settings, where the notice waits too and where they can turn Online answers off. The question that waited is not sent |
| A question chosen for this computer, or a help question | No notice: nothing goes online |
| A question from a phone, while phones may use Online answers | The phone shows the notice in place of the answer, with Continue and "Answer on your computer". The question waits on the phone |
| They press Continue on the phone | As Continue on the computer: the notice is gone everywhere, and the question that waited goes on its way, online when it can |
| They press "Answer on your computer" on the phone | The question is answered on the computer without going online, marked "Answered on your computer" as now. The notice comes back with the next question |
| A question from a phone while "Questions from your phones" is off | No notice: phone questions never go online then |
| Online answers in Settings, while the notice waits | The notice sits at the top of the page, under the opening line, with Continue. Continue clears it everywhere; turning Online answers off there also needs no notice |
| Ask opens before Online answers has been read | The question from the bar waits the moment it takes to read it, so the notice is never skipped |
| A new setup | The consent step already has the new words, so turning on there counts as having seen them: no notice. This is saved only once turning on has worked; if saving it fails, the notice comes with the first question instead |
| Turned on again later from Settings | No notice when the person has seen the new words before; otherwise the notice comes with the first question |

The phone offers no way to Online answers settings: a phone cannot change settings
([decision 0010](../adr/0010-away-from-home.md)). Continue only records that the person has seen
what changed, which is safe from their own paired phone.

### Copy

The notice, in the Ask column where the answer would be, with the Online answers lock icon:

| Part | Words |
|---|---|
| Heading (`ask.changed.title`) | Online answers has changed |
| Body (`ask.changed.body`) | Nocturne no longer keeps back lines that look like health, money or ID details. Anything you kept can now go with a question it helps answer, unless you marked it private. |
| Second line (`ask.changed.private`) | Nocturne doesn’t guess what is private. Mark anything you never want sent as private. |
| Waiting line (`ask.changed.waits`) | Your question waits until you continue. |
| Buttons | Continue (main, `ask.changed.continue`) · Answer on this computer (quiet, `ask.changed.computer`) · Online answers settings (quiet, opens Settings, Online answers, `ask.changed.settings`) |
| If saving that it was seen fails (`ask.changed.failed`) | Nocturne couldn’t save this, so your question hasn’t been sent. Try again. |
| Answer mark reason, if a question ever gets past the notice (`ask.reason.changed`) | online answers waits until you’ve seen what changed |
| Sent online reason, should such a question ever be listed (`sent.reason.changed`) | you hadn’t yet seen what changed |

On the phone, in the Ask screen where the answer would be, under "You asked" and the question. The
same heading, body, second line and waiting line, in the phone's answer type, then:

| Part | Words |
|---|---|
| Buttons | Continue (main, `ask.changed.continue`) · Answer on your computer (quiet, `phone.ask.changed.computer`) |
| If saving that it was seen fails, or the phone loses the computer while saving (`ask.changed.failed`) | Nocturne couldn’t save this, so your question hasn’t been sent. Try again. |
| While Continue saves | Continue is not pressable; the ask bar stays as it is while a question is waiting |
| A new question typed while the notice shows | Replaces the waiting question; the notice stays, for the new one |

The phone says "your computer", as its other lines do ("Answered on your computer").

In Settings, Online answers, a card under the opening line and above the switch, the same kind of
card as the page's warnings but not marked as trouble:

| Part | Words |
|---|---|
| Heading (`ask.changed.title`) | Online answers has changed |
| Body (`ask.changed.body`) | As above |
| Second line (`ask.changed.private`) | As above |
| Button | Continue (`ask.changed.continue`), the card's one button |
| If saving that it was seen fails (`online.changed.failed`) | Nocturne couldn’t save this. Try again. |

The card has no waiting line: no question waits in Settings. Once Continue works, the card goes and
the page is as it always is.

The words follow the step flows' Online answers copy: the same "keeps back" and "mark … private"
phrasing, one main button first, and a plain way out: on the computer to the place where it can be
turned off, and everywhere to an answer that sends nothing. No switch comes back; the only choices
are to carry on, to keep this question here, or to turn Online answers off.

Whether the person has seen the new words is kept with Nocturne's other settings on this
computer. It goes with Delete everything, which also turns Online answers off.

## Behaviour

- **Online answers.** A memory may go when it is not marked private in any version, not being
  forgotten and not let go. Nothing about its words, labels or subject keeps it here.
- **The page question.** The rule in [Journaling follow-ups](journaling-follow-ups.md#3-the-page-question-from-the-journal)
  loses two lines: a memory or sentence the health, money and ID check flags, and a safety memory,
  no longer fail to qualify. Private memories and memories with a private source still never
  qualify.
- **The daily question.** A memory qualifies when it is active, not kept forever, not asked about in
  the last 14 days and never asked about by the journal. What it is about plays no part.
- **Answers that give a side or a move.** A note is used for an appointment only when both are
  private or neither is. Whether either is about health, money or ID no longer matters.
- **The notice on a phone.** A phone question that would go online while the notice waits is not
  started: the computer answers that the notice waits, and the phone shows it. Continue asks the
  computer to record that the notice was seen, a call a paired phone may make, then asks the
  question again. "Answer on your computer" asks it again with this computer chosen. Should a
  question still get past the notice, the computer answers it here with the reason above.
- **Stored settings.** The saved choice for the removed switch is no longer read or written, and
  neither is the old hint flag on memories. The logged shadow answers about health, money and ID
  are dropped the next time the decision log is written.

## Mockups

The Online answers mockups in `docs/design/mockups/journaling/` (`online-settings.html`,
`online-step1.html`, `online-step5.html`) and `docs/design/mockups/step-flow-actions/`
(`online-step1.html`, `online-step5.html`) show the change. The journaling renders
`online-settings-light.png`, `online-settings-dark.png`, `online-step1-open-light.png`,
`online-step5-light.png` and `online-step5-dark.png` are made again from them. The before and after
pairs in `step-flow-actions/` record that earlier change and are left as they were.

A switch row on the Online answers screens (Settings while on and before turning on, and setup step
5) lays out as `online-settings.html` shows: the label and its hint take the room there is, the
switch keeps its own width at the right with at least 16px between them, and it never wraps under
its label.

The notice has no mockup of its own: it is drawn in Ask, on the phone's Ask and in Online answers
with the parts those screens and the Online answers steps already use, and renders of it as built,
light and dark, go with the pull request.
