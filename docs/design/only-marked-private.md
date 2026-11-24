---
type: Design
title: What private means
description: Nocturne never decides what a person's data is about, and since 2026-11-23 a private mark decides only what comes up on its own, never what an answer the person asks for may use, what a mind the person chose gets, or what their own phone shows; with the calm explanation at setup that is the safeguard and the copy that changes.
status: draft
tags: [proposal, privacy, online-answers, journal, memories, copy]
---

# What private means

## Revised 2026-11-23: private is about what comes up on its own

From the owner's decision of 2026-11-23. Built for Online answers on the computer: Ask sends a
private memory like any other, and its words, on step 1, step 5 and in Settings, follow this page.
Not built yet: a companion phone, a household's mind and an online mind on a phone alone (the last
reads private things already, [0020](/adr/0020-an-online-mind-on-a-phone.md)). It changes the
approved 2026-11-22 design below, which is built.

An online mind the person chose gets what a good answer needs. A private mark decides only what
Nocturne brings up without being asked: notifications, reminders and updates. It never keeps
something out of an answer the person asks for, or out of what goes to the provider they chose. The
safeguard is a short, calm explanation of what is sent, at the moment the person chooses the
provider, written for people who are not technical and without alarm.

| A thing marked private | Before | After |
|---|---|---|
| Notifications, reminders, reading aloud, the tray, a lock screen | Kept out | Kept out |
| The morning page's questions (asking about it on its own) | Never asked about | Never asked about |
| An answer on this computer | Used | Used |
| Online answers on the computer | Never sent | Sent when it helps answer the question |
| An online mind on a phone ([0020](/adr/0020-an-online-mind-on-a-phone.md)) | Never sent | Sent, read and used like anything else |
| A companion phone | Kept out | Shown like on the computer, still never on the lock screen or in notifications, reminders or updates |
| A household's mind, borrowed | Never sent | Gets what an answer needs, like any mind the person chose |

What still never goes: what the person let go or is forgetting. Reflections and recaps are written
on this computer and Ask does not search them, so Online answers does not send them; that is what
Ask does, not a privacy rule. Check-ins go like any other kept line, since Ask with tools reads them
(decision 0011). Online answers still sends words only, since it writes the last step of an answer.

### The explanation at setup

On Online answers step 1 (approved words in [Keeping the next step in
view](step-flow-actions.md#after-the-approved-words)) the "Never sent" line becomes "Private":

| Part | Before | After |
|---|---|---|
| Sent | Your question, with a few short lines from what you kept that help answer it, your journal entries and good things included. | Your question, with a few short lines from what you kept that help answer it, your journal and things you marked private included. |
| Never sent → Private | Your photos, recordings and documents themselves, your check-ins, reflections and recaps, and anything you marked private. | Marking something private keeps it from coming up on its own, in reminders and notifications. It still helps answer what you ask. |

Behind the expander, "Never sent" becomes "What stays here": "Your photos, recordings and documents
stay on this computer. A short line read from one can go when it helps. Things you let go never
go." The journal's added lines about check-ins and about entries that can't be marked private are
removed.

On step 5 the lead stays ("While this is on, each question and the few lines that answer it leave
this computer. Everything else you kept stays here."). The "Never sent" list becomes "What stays
here" with "Photos, recordings and documents themselves" and "Things you let go"; the private and
check-in lines and the note under the list go, and one line is added: "Marking something private
keeps it from coming up on its own. It still helps answer what you ask." "Your choices" loses
"Check-ins, reflections, recaps / Never sent", and "Journal entries and good things" becomes
"What you kept, your journal included / May be sent".

In Settings, Online answers: the lead becomes step 5's; the "Private things / Never sent" row goes;
the journal row's hint becomes "A line from an entry or a good thing may go with a question it helps
answer."; "Always stays here" becomes "Stays here" with "Photos, recordings and documents
themselves" and "Things you let go", and its notes become the one private line. Step 5's "Your
choices" loses its "Private things / Never sent" row too.

The Online answers mockups under `journaling/` and `step-flow-actions/` record earlier changes with
their markers and are left as they were. Renders of step 1, step 5 and Settings as built, light and
dark, go with the pull request, as for the 2026-11-22 notice below.

### Other copy

| Where | Before | After |
|---|---|---|
| An item's private switch, hint | (as today) | "Private things don’t come up on their own in notifications, reminders or updates. They still help answer what you ask." |
| `ask.leftOut.private` | Some things you marked private were left out. Ask on this computer to include them. | Removed: nothing is left out for being private |
| `ask.leftOut.other` | Some things were left out. Ask on this computer to include them. | Kept for one case: the provider's answer gave one side and this computer added the other from a line of an original, which never goes online |

### As built on the computer

Where the build differs from the tables above, this is what ships.

| Where | Words |
|---|---|
| Settings, the list under the lead (`online.stays.heading`) | "Stays here": photos, recordings and documents themselves; things you let go; your reflections and recaps; keeping, reading and the overnight tidy; your morning page and its voice. The note under it is the one private line |
| Settings, rows | "Private things / Never sent" is gone; the journal row's hint ends after "helps answer." |
| Step 3, the private row | Gone: "Private things are never sent" would be untrue |
| Step 5 | "What stays here" lists photos, recordings and documents themselves, things you let go, your reflections and recaps, and everything else you kept, your pages and your name. Under it: "Marking something private keeps it from coming up on its own. It still helps answer what you ask." "Your choices" has "What you kept, your journal included / May be sent", and no private row; "Reflections and recaps / Never sent" stays |
| The notice for people who agreed before (`ask.changed.body`, `ask.changed.private`) | Body: "Nocturne no longer keeps back lines that look like health, money or ID details, or things you marked private. Anything you kept can now go with a question it helps answer." Second line: the private sentence. Nobody is shown a new notice for this change |
| Privacy, "It can’t see" | Photos, recordings and documents; everything else you kept. "Anything you marked private" is gone |
| A phone that is its own home, the private switch (`phone.alone.private.hint`) | "Private things don’t come up on their own in notifications, reminders or updates. They still help answer what you ask. If your memory moves, it moves too, locked like the rest." |

### On a companion phone and a household's mind

A paired phone shows private things as the computer does: in Ask, on the page, in the journal and
when an original is opened. They still never show on its lock screen or in a notification,
reminder or update. Away from home's outbound filter for private items goes; the one for the lock
screen and notifications stays.

| Where | Before | After |
|---|---|---|
| The phone page's foot (`Phone page`) | "Things you marked private stay on your computer." | Removed |
| Opening a private original on a phone (`phone.original.private`) | "That one stays on your computer." | Removed: it opens |
| Away from home setup, side column and "Private things" row ([Keeping the next step in view](step-flow-actions.md)) | "Things you marked private stay on this computer." / "Stay on this computer and never reach your phone." | Removed |
| The private switch's hint | As above | As above: one hint everywhere |
| Joining a household ([A household computer](household-server.md)) | "Things you marked private never go to {household}." | "Marking something private keeps it from coming up on its own. It still helps answer what you ask." |

### No notice

People already using Online answers are not shown a notice about this change (owner, 2026-11-23).
The words they see at setup and in Settings change as above.

## 2026-11-22: only what you mark private is held back

The design below is approved and built. Where the revision above differs, the revision holds once
it is approved.

The owner's rule, 2026-11-22: never judge a person's data, and apply it to what Nocturne already
does. Nocturne must not decide on the person's behalf that something is health, money, ID, work or
private. Only what the person marks private is held back.

Until now a word list and a few labels decided that a memory was "a health, money or ID detail".
Online answers kept such memories here unless the person turned a switch on, the memories list
suggested marking them private, and the morning page never asked about them or about a medicine,
allergy or health memory. Each of these was Nocturne judging what the person kept. This page
removes all of it. If something is private, the person says so by marking it.

### What stays

| Rule | Why it stays |
|---|---|
| Things marked private are never sent online and never reach a phone | The person decided it |
| Check-ins, reflections and recaps never go online ([decision 0015](../adr/0015-journal-privacy-class.md)) | A fixed product rule about kinds of writing, not a judgement about what any of it says |
| A memory being forgotten or let go is never sent | The person let it go |
| Medication, allergy and health facts are kept forever by default ([decision 0007](../adr/0007-forgetting-by-default.md)) | It keeps something, it never holds anything back, and the person can change it |

### What changes

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

### Copy

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

### A notice for people who already use Online answers

People who turned Online answers on before this change agreed to words that said health, money and
ID details stay here. Before their first question goes online after the change, Nocturne tells them
once what changed, and nothing is sent until they have seen it. They can see it wherever they ask,
on the computer or on a phone, and in Settings, so a person who mostly uses a phone, or who goes to
Settings first, can clear it there.

#### When it shows

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

#### Copy

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

### Behaviour

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

### Mockups

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
