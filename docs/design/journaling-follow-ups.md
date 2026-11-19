---
type: Design
title: Journaling follow-ups
description: What the journal still needed before its next release, with screens, copy and behaviour for deleting a day, a check-in's own page, deleting a recap, the page question from the journal, recaps on every computer, and the words the build added without design.
status: stable
tags: [ux, journal, copy, forgetting]
---

# Journaling follow-ups

The [journal](journaling.md) is built. Building it turned up five things the design did not settle:
a dialog built without design, an action the design promised but never drew, a decision with no
rule behind it, a gate that does not fit what recaps are, and words that never went through
design. This page settles each, with screens, copy and behaviour, in the style of the journaling
design. Where it changes that page, the change is made there too.

Status: approved. The owner answered every open question on 2026-11-19 (see [Decisions](#decisions)).

## Mockups

In `docs/design/mockups/journaling/`, beside the journaling mockups, with the same kit and the same
rules: serve the repository root over HTTP after `make ui/install`, add `?state=` and
`&theme=light` or `&theme=dark`. Dialog renders are 1280 by 1180 so the whole list shows; the rest
are 1280 wide, and the phone is 390 by 844. The built screens they replace are in `built/`.

| Screen | File and states | Renders | Built today |
|---|---|---|---|
| Delete this day | `delete-day.html`: `pick`, `versions`, `none`, `done` | `delete-day-light.png`, `delete-day-dark.png`, `delete-day-versions-light.png`, `delete-day-none-light.png`, `delete-day-done-light.png` | `built/after-delete-day-light.png`, `built/after-delete-day-dark.png` |
| A check-in's own page | `checkin-item.html`: `item`, `delete` | `checkin-item-light.png`, `checkin-item-dark.png`, `checkin-item-delete-light.png` | `built/after-checkin-item-light.png` |
| Deleting a recap or a reflection | `delete-lookback.html`: `recap`, `deleted`, `reflection`; `recap.html` gains the action | `delete-recap-light.png`, `delete-recap-dark.png`, `delete-recap-after-light.png`, `delete-reflection-light.png`, and `recap-week-light.png`, `recap-month-dark.png`, `recap-year-light.png` rendered again | `built/recap-week-light.png` |
| The page question from the journal | `page-question.html`: `ask`, `answered`, `added` | `page-question-light.png`, `page-question-dark.png`, `page-question-answered-light.png`, `page-question-added-light.png` | Not built |
| Phone: a name question answered elsewhere | `phone-which.html`: `gone` | `phone-which-gone-light.png` | Not drawn before |

The copy changes in item 5 also touch earlier mockups, rendered again: `checkin-words-light.png`,
`checkin-words-dark.png`, `phone-checkin-light.png` and `phone-checkin-dark.png` lose the
microphone on the check-in's line, and `phone-recap-light.png`, `phone-recap-dark.png` and the three
recap renders carry the new together line.

## 1. Delete this day

`delete-day.html`. Opened from "Delete this day…" on the day's menu. The built dialog lists every
original as a flat row: "ORIGINAL" on each, titles cut mid-word, "Check-in (check-in)", earlier
versions that look like copies, and no sense of what is this day and what is not.

**Layout.** A dialog 680 pixels wide. The head and the foot stay put and the list scrolls between
them, so the title and the buttons are always in view. On a narrow window it fills the window.

- **Head.** Title "Delete {weekday} {day} {month}?". Under it: "Everything kept on this day goes
  from this computer for good, with what Nocturne learned from it. Untick anything you want to
  keep."
- **Group one: "Kept on this day".** A box for the whole group (ticked, unticked or partly ticked)
  and a live count, "{ticked} of {total}". Two quiet sub-headings follow the journal's own shape:
  - "Your entry": the entry's blocks, in the order they sit in the entry.
  - "Through the day": everything else kept that day, oldest first, as the timeline orders it.
    Check-ins and good things are here.
  A sub-heading with nothing under it is not shown.
- **Group two: "From other days".** Only when something kept on another day shares a memory with
  this day. The group box and count as above, then "These share a memory with this day. The shared
  memory goes; these stay unless you tick them." Every row here starts **unticked**.
- **Foot.** What else goes, then the buttons:
  - "And the {count} memories Nocturne read from them." with "Show them", which opens the list of
    memory sentences in place. Hidden when there are none.
  - "Your reflection on this day, and {count} lines in your recaps that cite it." when every row
    in group one is ticked (without recap lines: "Your reflection on this day."). Otherwise
    "{count} lines in your reflections and recaps cite these and will be taken out." Hidden when
    there are none.
  - "Anything you exported or printed is out of reach. Nocturne can’t delete it."
  - "Keep the day" (secondary, and where focus starts) and "Delete {count} things" (the forget
    button).

**Row anatomy.** One line per thing, about 38 pixels tall, in five columns:

| Column | What it shows |
|---|---|
| Box | Ticked or not. The whole row is the box's label, so a click anywhere on it ticks |
| Kind | A small icon: pen for a note or entry paragraph, microphone for a voice note, picture for a photo, page for a document or web page, phone for a note kept from a phone, the sun for a good thing, the part's shape in its tone for a check-in |
| Time | "7:52 am". In group two, the day instead: "Wed 18 Nov" |
| What | A readable snippet on one line, cut at a word with "…" when it does not fit. The full text is the row's accessible name and shows on hover |
| Tag | Only where the icon alone is not enough, in small mono text: "Voice note, 1:42", "Photo", "Document", "Good thing", "from {phone name}", "dropped in" |

The snippet for each kind:

| Kind | Snippet |
|---|---|
| Entry paragraph or note | Its first words |
| Voice note | Tag "Voice note, {duration}", then the first words of what was said in quotes, once read |
| Photo | Tag "Photo", then what Nocturne saw in it, once read; "Photo, not read yet" before |
| Check-in | "{part} · {words}" in medium weight, then its line in quotes. No line: just the part and words |
| Good thing | Tag "Good thing", then the line as written |
| Document or web page | Tag "Document" or "Web page", then its title |
| Answer to the page question | "{choice}" in medium weight, then the line in quotes. No line: just the choice |

There is no "ORIGINAL" column and no kind in brackets. Two rows never look the same: each has its
own time, and two things with the same words in the same group also show where they came from
(the entry's photo against "dropped in" at 3:40 pm).

**Versions collapse into one row.** An entry paragraph changed after it was read has earlier
versions on the tape. They are one row, the newest words, with a line under it: "Changed {time} ·
{count, plural, one {1 earlier version, goes with it} other {# earlier versions, go with it}}".
The disclosure shows each earlier version's time and words, read only. Ticking or unticking the row
ticks or unticks every version, because keeping an old version of a paragraph you deleted is never
what someone means. The count on the button counts rows, not versions.

**Something that cannot go.** An original that other memories, from days not listed, still use
keeps its row with no box and the line "Stays. Memories from other days still use it." in place of
a tag.

**Behaviour.**

- Everything in group one starts ticked, check-ins included. Everything in group two starts
  unticked (owner decision).
- Ticking or unticking updates the counts, the foot and the button at once.
- Nothing ticked: the button reads "Delete" and is disabled, and the foot says "Nothing is ticked,
  so nothing will be deleted."
- Esc and "Keep the day" close it with nothing changed. Once "Delete" is pressed it cannot be
  stopped, so the dialog cannot be closed until it is done, as today.
- Done: "Deleted." and "{count} things are gone from this computer, with the {memories} memories
  read from them. They won’t appear in answers, pages or anywhere else." When group two had rows
  left unticked: "{count, plural, one {1 thing from another day stays.} other {# things from other days
  stay.}}" Button "OK". The day then shows its empty state.
- The day's reflection goes when every row in group one goes. Otherwise its lines that cite what
  went are taken out, as for any deletion.

**Copy.**

| Key | Copy |
|---|---|
| Title | "Delete {weekday} {day} {month}?" |
| Lead | "Everything kept on this day goes from this computer for good, with what Nocturne learned from it. Untick anything you want to keep." |
| Group one | "Kept on this day" |
| Sub-headings | "Your entry", "Through the day" |
| Group two | "From other days" |
| Group two note | "These share a memory with this day. The shared memory goes; these stay unless you tick them." |
| Count | "{ticked} of {total}" |
| Versions | "Changed {time} · {count, plural, one {1 earlier version, goes with it} other {# earlier versions, go with it}}" |
| Cannot go | "Stays. Memories from other days still use it." |
| Memories | "And the {count, plural, one {memory} other {# memories}} Nocturne read from them." with "Show them" / "Hide them" |
| Reflection goes | "Your reflection on this day." / with recap lines: "Your reflection on this day, and {count, plural, one {1 line in your recaps that cites it.} other {# lines in your recaps that cite it.}}" |
| Lines go | "{count, plural, one {1 line in your reflections and recaps cites these and will be taken out.} other {# lines in your reflections and recaps cite these and will be taken out.}}" |
| Out of reach | "Anything you exported or printed is out of reach. Nocturne can’t delete it." (existing) |
| Nothing ticked | "Nothing is ticked, so nothing will be deleted." |
| Keep | "Keep the day" |
| Delete | "{count, plural, one {Delete 1 thing} other {Delete # things}}" / nothing ticked: "Delete" |
| Working | "Deleting…" (existing) |
| Done | "Deleted." / "{count, plural, one {1 thing is} other {# things are}} gone from this computer, with the {memories, plural, one {memory} other {# memories}} read from them. They won’t appear in answers, pages or anywhere else." / no memories: "{count, plural, one {1 thing is} other {# things are}} gone from this computer. They won’t appear in answers, pages or anywhere else." |
| Done, others stay | "{count, plural, one {1 thing from another day stays.} other {# things from other days stay.}}" |

The existing loading, refresh and error lines of the delete dialog are unchanged.

### A check-in's own page

`checkin-item.html`. Every timeline row opens its item's page. The built page treats a check-in as
any note: it offers "Mark private" (a journal item's page never does), "Keep forever" (a check-in
never fades) and "Read it again", and says "Memories from it · 0" and "Nothing has been remembered
from this yet.", which suggests something is still to come. Nothing is: a check-in yields mentions
only.

| Part | What shows |
|---|---|
| Left | The small field with the dot where the person put it, and the line they wrote, in quotes |
| Back link | "← Journal" when opened from the journal; the screen it came from otherwise |
| Eyebrow | "Check-in · {short day} · {time}" |
| Title | The part's shape, then "{part} · {words}" ("Low · tired"); no words: "{part}" |
| What you wrote | Heading "What you wrote" and the line. Hidden when there is none |
| The urge | Heading "Did you feel like doing anything?" and the answer. Hidden when there is none |
| Names | "In this check-in" and its mention chips, as on an entry. Hidden when there are none |
| Note | "Check-ins stay in your journal. Nocturne never makes memories from them, so they never fade and Ask never uses them." |
| Actions | "Change" (opens the check-in with the choice made), "Leave out of reflections" (or "Bring back into reflections" when left out), "Delete…" |
| Not shown | "Mark private", "Keep forever", "Read it again", "Understood in …", "Memories from it" |

**Deleting a check-in.** "Delete…" opens the delete dialog in its short form: title "Delete this
check-in?", then "{part} · {words} at {time}, and the line you wrote with it, go from this computer
for good." (no line: "{part} · {words} at {time} goes from this computer for good."), the
reflections line when lines cite it, the out-of-reach line, and "Keep it" and "Delete check-in".
Done: the toast "Deleted." and the journal day.

| Key | Copy |
|---|---|
| Eyebrow | "Check-in · {day} · {time}" |
| Title | "{part} · {words}" / "{part}" |
| Line | "What you wrote" |
| Urge | "Did you feel like doing anything?" (existing) |
| Names | "In this check-in" |
| Note | "Check-ins stay in your journal. Nocturne never makes memories from them, so they never fade and Ask never uses them." |
| Bring back | "Bring back into reflections" / done: "Back in your reflections." |
| Delete title | "Delete this check-in?" |
| Delete body | "{part} · {words} at {time}, and the line you wrote with it, go from this computer for good." / without a line: "{part} · {words} at {time} goes from this computer for good." |
| Delete button | "Delete check-in" |
| Keep | "Keep it" (existing) |

## 2. Deleting a recap

`delete-lookback.html`, and `recap.html` now shows the action. The design says deleting a recap
removes only the writing, but drew no action and wrote no words for it. Review item 16.

**Where it lives.** At the end of the recap, in the footer row beside "Write it again" and
"Print": "Delete this recap", a quiet button in the forget colour at the far end, exactly where a
reflection has "Delete this reflection". The phone has no delete, as for everything else.

**Confirmation.** A small dialog. Focus starts on "Keep it".

| Part | Copy |
|---|---|
| Title | "Delete this recap?" |
| Body | "Only the recap goes. Everything you kept {period} stays: entries, check-ins, good things, notes and photos." |
| {period} | Week: "from {start} to {end}" ("from 9 to 15 November"). Month: "in {month}" ("in November"). Year: "in {year}" |
| Note | "Nocturne won’t put this {week, month or year} together again unless you ask. To keep one thing out of recaps for good, choose “Leave out of reflections” on it." |
| Buttons | "Keep it" / "Delete recap" |
| Done | Toast "Deleted. Everything you kept from that {week, month or year} is still here." |

**After deleting.** The recap's page shows the period at a glance and one button, "Look back on
this week" ("this month", "this year"), which puts it together again. Without "so far": the period
is over.

**Behaviour.**

- Only the stored recap goes: its lines and their receipts. No item, memory, mention or check-in
  changes.
- A deleted recap is not put together again overnight. It comes back only when the person asks.
  This holds for a recap that is still going ("so far") too: the night after the period ends does
  not write it.
- If the morning page carries "Last week, looked back on ›" for it, the line goes from the page.
- Deleting a year recap does not delete the month recaps, and the other way round.

**A reflection gets the same confirmation.** Today "Delete this reflection" deletes at once, with a
toast. A deletion that cannot be undone is confirmed everywhere else in Nocturne, so it gets the
same small dialog:

| Part | Copy |
|---|---|
| Title | "Delete this reflection?" |
| Body | "Only the writing goes. Everything you kept on {weekday} {day} {month} stays: your entry, check-ins, good things, notes and photos." |
| Note | "Nocturne won’t write it again unless you ask." |
| Buttons | "Keep it" / "Delete reflection" |
| Done | The existing toast, "Deleted. Your entries are untouched. You can write it again." |

A deleted day's reflection is not written again overnight either, and the page's "Yesterday,
looked back on ›" goes with it.

## 3. The page question from the journal

`page-question.html`. Decision 7: at most once a week, the morning page's one question may come
from the journal, and it is never read aloud. This sets the rule that picks it and the words it
uses. The rule needs no writing by the mind: the question is a fixed template around the person's
own sentence.

### What qualifies

A memory qualifies when every line holds:

| # | Condition |
|---|---|
| 1 | Its predicate is `scheduled`: something the person said would happen at a time |
| 2 | Its date falls on the day the page looks back on (yesterday, by the [day boundary](journaling.md#what-a-day-is)). A plan with no day named, written in a block kept on that day ("Parents’ evening at six."), is dated that day |
| 3 | It was read from a block of a day's entry, text or voice, kept within the 30 days before that date |
| 4 | The sentences the memory's span touches, taken whole, are 3 to 20 words together, and are in the newest version of the block |
| 5 | Nothing kept after that block, up to the end of the event's day, mentions the memory's subject. If the person already wrote about how it went, Nocturne does not ask |
| 6 | It is active, not a safety memory, and not asked about before, on any page |

What never qualifies, whatever the rest says:

| Never | Why |
|---|---|
| Check-ins, and anything said in one | They yield no memories, and how a moment felt is not a thing to follow up |
| Good things | A good thing is not a plan |
| Notes kept from the keep bar, quick capture or a phone, documents, photos and web pages | They are not the journal. They already reach Needs you and the page |
| A photo in the entry | The words would be Nocturne's description, not the person's |
| Anything left out of reflections | The person asked for it to stay out of looking back |
| Anything private, or a memory with any private source | Private stays private |
| A memory or sentence the health, money and ID check flags | A morning page may be seen by someone else at the screen |
| A memory with a promise or due date (`promised`, `due`) | Needs you already follows those up |

### When it is asked

- **At most once in seven days.** A journal question is asked only when none of the six pages
  before had one.
- **Never on the first page.** The first page always asks about the page time.
- **It takes the slot.** On a day the rule finds a memory and the week allows it, the journal
  question is the day's one question, in place of the usual one.
- **One pick.** When more than one memory qualifies: the newest source block first, then the
  earliest time on the event's day, then the memory id.
- **Once per memory.** A memory is asked about once. Answered or not, it is never asked again.
- An unanswered journal question stays on its page, like any question. The next page asks as
  usual.

### Templates

The person's sentence is quoted whole, never cut and never reworded. Only the opening changes:

| The block was kept | Text block | Voice note |
|---|---|---|
| On the event's day | "Yesterday you wrote: “{sentence}” How did it go?" | "Yesterday you said: “{sentence}” How did it go?" |
| 1 to 6 days before it | "On {weekday} you wrote: “{sentence}” How did it go?" | "On {weekday} you said: “{sentence}” How did it go?" |
| 7 to 30 days before it | "On {day} {month} you wrote: “{sentence}” How did it go?" | "On {day} {month} you said: “{sentence}” How did it go?" |

For example: "Yesterday you wrote: “Parents’ evening at six. Not looking forward to it.” How did it
go?" The quote is two short sentences because the memory's span touches both.

### On the page

| Part | What shows |
|---|---|
| Label | "One question · from your journal" |
| Question | The template, the quote in italics |
| Receipt | A chip to the block: "Entry · {short day} · {time}", or "Voice note · {duration}" |
| Choices | "Fine", "Not great", "Not yet" |
| Answered | "{choice}. Kept on today’s journal." then "Anything to add? (optional)", a one-line field with the placeholder "A line, if you like.", and "Add" |
| Added | "{choice}. Kept on today’s journal." and "“{line}” Added." with "Open today’s journal ›" |

**What an answer does.**

- It is kept as a journal item on today, a note marked as an answer, with the choice, the line if
  any, and a link to the block it was about. On the timeline: the time, the choice in medium weight,
  the line, and under it "About: “{sentence}”", which opens the block. It is journal content: never
  read aloud, notified or in the tray.
- The line is read like an entry; the choice alone yields nothing.
- "Fine" and "Not great" count as use of the memory, as answering a question does.
- "Not yet" changes nothing about the memory. It is not asked about again.
- Pressing a choice keeps the answer at once. Adding a line is optional and can be done until the
  page is replaced.

**Read aloud and the phone.** Read aloud, the page skips the question and ends with "Today’s
question is on the page." (already designed). Printed, it prints like any question. The phone's
page does not show the day's question today, and still does not.

| Key | Copy |
|---|---|
| Label | "One question · from your journal" |
| Templates | As in [Templates](#templates) |
| Choices | "Fine", "Not great", "Not yet" |
| Answered | "{choice}. Kept on today’s journal." |
| Add label | "Anything to add? (optional)" |
| Add placeholder | "A line, if you like." |
| Add button | "Add" |
| Added | "“{line}” Added." |
| Open | "Open today’s journal ›" |
| Timeline row | "{choice}" then the line; under it "About: “{sentence}”" |
| Row menu | "Open what it was about", which opens the block, so the keyboard reaches it with one stop per row. Not shown, and no "About", once the block is deleted |

**Testing.** Each condition is a fact about stored rows (predicate, date, source item kind,
sentence length, mentions, sensitivity, earlier questions), so each has a test that flips one
condition and expects no question.

## 4. Recaps on every computer

**Approved by the owner.** This reverses decision 8 for recaps. Reflections are unchanged.

**What is built.** A recap is put together from what is already stored: check-ins with their
points and words, good things as written, names already found, highlights chosen by rule from
entries, memories and the page's own "What changed" lines. No sentence in it is written for it.
Yet it is gated like a reflection: on a computer that has not passed the gate, or before the engine
is ready, the week shows "At a glance" only.

**Recommendation: show recaps everywhere.**

| For | Against |
|---|---|
| Nothing in a recap is written for it, so there is nothing for the gate to protect: the gate checks written sentences for receipts, invented feelings and advice | On a small computer the day shows "At a glance" while the week shows a full recap, which can look uneven. The recap's own footer says it was put together from the days in the journal, so it reads as a summary, not writing |
| The memories and page lines it reuses were read on this computer already, on every size of computer | Before the engine is ready a recap has little in it. The partial line says so |
| It is the weekly payoff of keeping a journal; hiding it on smaller computers hides the part that costs least | |
| One rule is simpler to explain: Nocturne writes about a day only where it has passed the gate; it lists a week, a month or a year anywhere | |

**What changes.**

| Where | Today | With this |
|---|---|---|
| Recaps on a computer that has not passed the gate | "At a glance" only | The full recap |
| Recaps before the engine is ready | "At a glance" only | The recap from what has been read, with the partial line: "{count, plural, one {# thing from this week is} other {# things from this week are}} still being read, so {count, plural, one {it isn’t} other {they aren’t}} in this yet." and "Put it together again when {count, plural, one {it’s} other {they’re}} read" |
| Overnight | Written in the tidy after the day's reflection, when the computer can write | Put together after the day's reflection on every computer; a year takes one night |
| The gate | Covers reflections and recaps | Covers reflections only |
| Online | Never | Never. Unchanged |
| A period with only things kept from elsewhere | "At a glance" only | Unchanged |

The journaling design is updated to match.

## 5. Copy that never went through design

Each string as built, where it is, what is wrong with it, and the settled words.

| Built | Where (key) | Problem | Settled |
|---|---|---|---|
| "Nothing to look back on yet." | Choose a day, the Weeks, Months and Years tabs when empty (`journal.recap.nothing`) | Fine in tone, but the same words for three tabs, and it is also used for a link to a period that does not exist | "No weeks to look back on yet." / "No months to look back on yet." / "No years to look back on yet." A link to a period that does not exist: "Nothing to look back on here." |
| "On 3 of the 7 days you saw Leo", "went to {name}", "had {name}" | The recap's together line (`journal.lookback.did.*`, `journal.recap.together`) | It guesses. A name in an entry does not mean the person saw them ("Gran rang"), and "had the allotment" or "had the car" reads wrong. The reflection never guesses; the recap should not either | One form for people, places and things: "On {count} of the {total} days with {name} in your journal, you checked in as {what}." This replaces the approved example "On 3 of the 4 days you saw Gran" |
| "Opening the journal…" | Phone, loading a day, a reflection or recap, and a name question (`phone.journal.loading`) | "the journal" is distant beside "Opening today’s page…", and it is wrong for the name question | Day and looking back: "Opening your journal…". The name question: "Opening the question…" |
| "Nothing was kept on this day." | Phone, a past day with nothing kept (`phone.journal.empty.past`) | None. The computer shows "Nothing kept today yet…" on a past day, which is wrong there | Keep. Use it on the computer for a past day too |
| "This one has already been answered." | Phone, a name question that is no longer waiting (`phone.journal.which.gone`) | Not always true: it also shows when the question was left, or what it was about was deleted. No way back | Title "Nothing to answer here now." Body "It was answered on your computer, or what it was about is gone." Button "Back to today" (existing). See `phone-which-gone-light.png` |
| "Written on your computer at {time}, from {count} things you kept that day." | Phone, a reflection written on demand (`phone.journal.footer.onDemand`) | "that day" is odd on today's reflection so far, and the word order differs from the overnight line | On demand: "Written on your computer at {time}, from {count, plural, one {# thing} other {# things}} you kept." Overnight: "Written overnight on your computer, from {count, plural, one {# thing} other {# things}} you kept that day." The computer's on-demand footer follows: "Written on this computer at {time}, from {count, plural, one {# thing} other {# things}} you kept." |
| "{name} answered a question about a name" | Phone activity (`away.event.answeredName`) | "name" twice in one line ("Sam’s phone answered a question about a name") | "{name} answered {count, plural, one {a same-name question} other {# same-name questions}}". It matches the "Same name" badge and still says nothing about who |
| "Saved." | The check-in's done screen when no word was chosen (`journal.checkin.savedBare`) | Bare and abrupt as a title; with words the title says what was saved | "Saved. You checked in as {part}." with the part in lower case ("Saved. You checked in as low.") |
| Title "Nocturne" | The evening reminder notification (`EVENING_TITLE`) | The desktop already shows the app's name on every notification, so the title says nothing | Title "Evening reminder". Body unchanged: "Anything about today you want to keep?" The title names the setting, so the person knows where to turn it off |
| "A line, or hold Space to talk." | The check-in's line, on the computer (`journal.checkin.goingPlaceholder`) | Promises voice that the check-in does not have | "A line, if you like." The design's "a line of text or a voice note" for check-ins becomes a line of text; voice in a check-in is out of scope until designed. The phone's check-in loses the microphone the design drew |

"Saved." in Settings and in Away from home are not journal words and were designed before; they
are unchanged.

## 6. What needs you, counted once

The rail's Needs you count includes "Which {name} is this?", but the page's headline counted only
the lines on the page ("Two things need you today." beside a 3 in the rail). The two now agree:
the page's headline and its opening line count everything waiting in Needs you when the page is
made, the name question included. The name question still has no line of its own on the page
(it lives in Needs you and the journal); it is counted in the existing "{count} more this week in
Needs you" link under the page's Needs you lines.

## Changes to the journaling design

Made on [Journaling](journaling.md) in the same change, so the two pages agree:

| Section | Change |
|---|---|
| Mockups | The new mockups listed, pointing here |
| 5. The feelings check-in | Step 3 is a line of text; the placeholder is "A line, if you like." |
| 8. The day's reflection | "Delete this reflection" is confirmed |
| 9. Recaps | Shown on every computer; a year takes one night; "Delete this recap"; the together line; the picker's empty words |
| 10. The morning page | The question's example and a link to its rule |
| 12. The phone | The check-in's line has no microphone |
| Calm | The evening reminder's title |
| Privacy | Only reflections fall back to "At a glance" on a computer that cannot write them |
| Forgetting and deletion | Delete this day, a check-in's page and deleting a recap point here |
| Overnight and on demand | Recaps need no mind; the gate covers reflections |
| Decisions | Decision 7 points to its rule; decision 8 records the change |

## Decisions

The owner answered every open question on 2026-11-19.

| # | Question | Decision |
|---|---|---|
| 1 | Recaps on every computer (item 4) | A: recaps show everywhere. The gate covers reflections only. Recaps never go online |
| 2 | Confirming "Delete this reflection" | A: confirmed, with the same small dialog as deleting a recap |
| 3 | A way to stop journal questions | A: none. At most one a week, and it can be ignored |
| 4 | The together line's wording | A: "days with {name} in your journal" |
| 5 | The Today headline and the rail (item 6) | Keep: they agree |
| 6 | An undated plan written on the event's day (journal question, condition 2) | Keep: it counts as dated that day |
| 7 | "Open what it was about" in the row menu | Keep |
| 8 | The check-in page's back link | Accept: "← Journal" by default |
| 9 | "Look back again" that fails or is stopped | Accept: it still clears the decline |
| 10 | The journal question on the phone | Built: the phone's page never carries it, so its sentence never leaves the computer |

## Follow-ups

Small, agreed, and not built yet:

1. **A generic back link.** A `from` route parameter, so a check-in's page goes back to the screen
   it was opened from. Until then the back link reads "← Journal".
2. **A check-in's page in the rail.** On a check-in's page the rail highlights "What Nocturne
   remembers", not Journal, and the small field there has no part labels.
3. **The spoken page.** The mock's spoken-page `.ogg` still says "Two things need you today."
   beside the rail's three. Re-record it with the counted headline.
