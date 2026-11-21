---
type: Design
title: Health from the phone
description: Proposal for bringing sleep, heart rate, resting heart rate, blood pressure, steps and workouts from Apple Health or Health Connect to the computer, shown beside the journal and on the page in plain visual forms with the person's own usual range, gentle noticing with receipts, no medical claims, and the same privacy, keeping and deletion rules as everything else.
status: stable
tags: [proposal, ux, phone, health, journal, copy, privacy]
---

# Health from the phone

With the [Nocturne app on Android and iPhone](phone-apps.md), a person can choose to bring health
readings from Apple Health or Health Connect to their computer. Nocturne shows them where they
help: last night's sleep beside the morning check-in and on the page, a line of the last weeks
beside the journal, workouts on the day's timeline, and now and then a plain sentence in a recap
about what tended to go together, with its receipts.

Nocturne is not a health app. It shows the person's own numbers next to their own days, in their
own words. It never says what a number means for their health, never compares them to a norm, and
never advises.

Status: approved, with the [phone apps](phone-apps.md). The owner accepted every recommendation on
2026-11-21 ([Decisions for the owner](#decisions-for-the-owner)). Nothing here is built yet.

## Mockups

In `docs/design/mockups/phone-health/`, with the kit and rules of the
[phone app mockups](phone-apps.md#mockups). All data is fictional.

| Screen | File and states | Renders |
|---|---|---|
| The visual forms | `forms.html` | `forms-light.png`, `forms-dark.png` |
| Connecting Health on the phone | `connect.html`: `why`, `kinds`, `system`, `done`, `quiet`, `rationale`, `missing` | `connect-why-light.png`, `connect-why-dark.png`, `connect-kinds-light.png`, `connect-system-light.png`, `connect-quiet-light.png`, `connect-rationale-light.png` |
| Health in the phone's journal | `phone-journal.html`: `day`, `checkin`, `detail` | `phone-journal-day-light.png`, `phone-journal-day-dark.png`, `phone-journal-checkin-light.png`, `phone-journal-detail-light.png` |
| The journal day on the computer | `desktop-journal.html`: `day` | `desktop-journal-light.png`, `desktop-journal-dark.png` |
| One kind over time, on the computer | `desktop-detail.html`: `sleep`, `pressure`, `delete` | `desktop-detail-sleep-light.png`, `desktop-detail-sleep-dark.png`, `desktop-detail-pressure-light.png`, `desktop-detail-delete-light.png` |
| A week, with noticing | `desktop-recap.html`: `week` | `desktop-recap-light.png`, `desktop-recap-dark.png` |
| Health on the computer's settings | `desktop-sources.html` in `phone-apps/`, state `sources` | `desktop-sources-light.png` |

## What it is for, and what it is not

| For | Not for |
|---|---|
| Seeing how you slept next to how you felt | Telling you whether you sleep enough |
| Remembering the week you walked every day | Goals, streaks, rings or scores |
| Taking your own readings to an appointment, printed with your journal | Interpreting readings, flagging "high" or "low", or any diagnosis |
| Noticing, with receipts, that two things in your days went together | Saying why, or what to do about it |

## What Nocturne reads

Read only, and only the kinds the person turns on. The proposed set is small, each kind with its
reason; the rest are left out on purpose.

| Kind | What is read | Why it is in | Shown as |
|---|---|---|---|
| Sleep | Each night's asleep periods: when sleep started and ended, and time awake in between. "In bed" when the phone has no asleep periods | How a night went shapes how a day feels; it belongs beside the morning check-in | A night bar, "6 h 40 min asleep" |
| Heart rate | The readings, kept as each hour's lowest, highest and middle | A long or tense day often shows; the day's range sits beside the entry | A day range bar |
| Resting heart rate | One value a day, as Health works it out | The steadiest everyday line, with little noise | A line with your usual band |
| Blood pressure | Each reading: the top and bottom numbers and the time | People who measure at home want their readings next to what they wrote, and printed for their doctor | Paired numbers and a readings strip |
| Steps | The day's total | On almost every phone with no watch needed; it shows a busy day from a still one | Day bars |
| Workouts | Each one's kind, start and length, and distance when there is one; never the route | A walk or a swim is part of the day and belongs on its timeline | A timeline row, "Walk · 42 min" |
| Weight | Each reading | Some people track it and want it beside the journal | Off by default; a plain line when on ([decision H8](#decisions-for-the-owner)) |

**Left out, and why.**

| Kind | Why it is left out |
|---|---|
| Sleep stages (deep, light, dreaming) | Each device guesses them differently, and they read like a verdict on the night. Time asleep says enough |
| Heart rate variability | Hard to explain without jargon, very noisy, and easily read as a health score |
| Blood oxygen, ECG, irregular rhythm notices, blood glucose, temperature, breathing rate | Medical measurements whose meaning needs a clinician; showing them beside a journal invites self-diagnosis |
| Cycle tracking | Deserves its own design and its own care, not a row in a list |
| Calories and nutrition | Diet framing, and not about the day lived |
| Mindful minutes | Says little that a check-in does not |
| Medications | Tempting beside keep-forever medication memories, but taking medicine needs its own design, with reminders and safety in view |
| Workout routes and places | Location is out of scope for the phone apps, and the route adds nothing to the day |
| Distance walked, floors climbed | Steps already say it |

## How readings are kept

Readings are a new kind of thing kept, `reading`, on the tape beside notes and check-ins.

| Thing | One item per | Holds |
|---|---|---|
| A night | Night, by the [day boundary](journaling.md#what-a-day-is): a night belongs to the day it ends on | Asleep periods, total asleep, the source as Health names it ("Watch", "This phone") |
| Heart rate | Day | 24 hourly lowest, highest and middle values, and how many readings |
| Resting heart rate | Day | The value |
| Blood pressure | Reading | Top, bottom, time, source |
| Steps | Day | The total |
| A workout | Workout | Kind, start, length, distance |
| Weight | Reading | Value, unit, time |

- **Source and trust.** The source is "Health on {phone}" with the app or device Health names.
  Readings are trusted: they are the person's own.
- **No memories.** Like a check-in, a reading yields no memories. It never enters what Nocturne
  remembers, never fades, never reaches Ask, and never goes online ([decision H7](#decisions-for-the-owner)).
- **Kept until deleted.** The tape is never let go, so readings stay until the person deletes them.
  This sits with the rule that health, allergy and medication memories are kept forever by default
  ([Concept](concept.md#the-forgetting-policy)): readings are not memories, so there is nothing to
  fade, and a memory read from a journal entry about health ("Gran’s scan came back clear") is kept
  forever as before. Nothing about readings changes that rule.
- **Once, however often sent.** Each reading carries the phone's own id for it, so a reading sent
  twice is kept once, and a reading changed in Health replaces the earlier version.
- **Removed in Health.** A reading the person deletes in Health is sent as removed. The computer
  keeps it, marked "Removed in Health", leaves it out of charts, sentences and noticing, and deletes
  it only when the person chooses ([decision 10](phone-apps.md#decisions-for-the-owner)).
- **Units.** Kept as Health gives them, shown in the computer's own units (stones or kilograms, for
  example), never converted in what is kept.

## Your usual

Every chart shows the person's own usual, never a norm. "Your usual" is the middle half of their
own readings over the last 60 days: the lowest quarter and the highest quarter are left out, and
what remains is the band. It needs at least 14 days with readings; before that, charts show the
readings alone and "Your usual shows after two weeks of readings."

Comparisons say only "more than", "less than", "higher than" or "lower than" your usual, and only
when today is outside the band. Inside it, nothing is said.

## The visual forms

Drawn for a reader who has never read a chart. Every form has a sentence that says what it shows,
which is also what a screen reader hears, and a "Show as a list" that gives the numbers by day.

| Form | Used for | How it is drawn | Its sentence |
|---|---|---|---|
| Night bar | Last night, on the page, the journal and the phone | A track from 8 pm to noon, hours marked at 10 pm, midnight, 2, 4, 6, 8 and 10 am. The asleep periods in the accent colour, awake gaps as breaks. Your usual bedtime and waking time as two faint ticks | "6 h 40 min asleep, from 11:50 pm to 7:05 am, awake once." |
| Nights strip | A week or a month of nights, in recaps and on the sleep page | One night bar per row, the same track, the day at the left; a faint band where your usual night falls | "7 nights. Asleep 6 h 50 min on an average night, between 5 h 10 min and 8 h 05 min." |
| Line with your usual band | Resting heart rate, sleep length, weight over 14, 30 or 90 days | A thin line in the ink colour on your usual band in the background tint; today, or the day looked at, as a filled dot with its value; days with a journal entry marked with a small tick under the axis | "Resting heart rate, 58 today. Your usual is 55 to 61." |
| Day range bar | Heart rate for one day | A track from 40 to 180, the day's lowest to highest as a bar, the middle as a short upright mark | "Heart rate today, 52 to 148, mostly around 74." |
| Hour bars | Heart rate on its own page | 24 thin range bars, one per hour, on the same scale | "Highest between 5 and 6 pm, during a walk." when a workout explains it, otherwise only the hours |
| Readings strip | Blood pressure | Each reading an upright line from the bottom number to the top, with a dot at each end, by day; your usual band for each number in the background | "128 over 82 this morning. Your usual is 118 to 126 over 76 to 82." |
| Day bars | Steps over 7 or 14 days | One bar per day, today in the accent and the rest in a quiet grey, with the value above today | "8,412 steps today. 6,900 on an average day this fortnight." |
| Day markers | Every chart over days | A small tick under the days with a journal entry, and a ring round the day being looked at; noticing marks the days it counted | Part of the chart's own sentence |

**Rules for every form.**

- No red, amber or green, no good or bad colours, and no arrows. The accent is the only colour, so
  nothing reads as a warning.
- No target lines, goals, scores, rings or streaks.
- No reference ranges, "normal", or category bands, including for blood pressure.
- Axis labels and values on a chart in the mono face; numbers in a sentence in the sentence's own face. Units spelt out the first time: "58 beats a minute", then "58".
- Every number is a receipt: tapping or clicking it opens the readings it came from.
- Charts start at a sensible floor for each kind (40 for heart rate, 0 for steps and sleep), never
  cropped to make a change look larger.

## Wording

| Say | Never say |
|---|---|
| "6 h 40 min asleep" | "Poor sleep", "a good night", "sleep score" |
| "Less than your usual" | "Too little", "not enough", "below normal" |
| "Higher than your usual" | "High", "elevated", "at risk", "unhealthy" |
| "128 over 82" | "Stage 1", "hypertension", any category |
| "Your usual is 55 to 61" | "Healthy range", "normal range" |
| "On the 4 nights before…, you slept 50 minutes less" | "Deadlines are hurting your sleep", "You should…" |
| "Nocturne can’t tell what your numbers mean for your health. A doctor or nurse can." | Any advice |

That last sentence is fixed and appears once on each kind's own page, in the phone's Health
settings and on the why screen.

## Where it shows

### On the page

One line under the page's title, before the deck, only when sleep is brought, the night has come in,
and "Sleep last night" is on in From your phones:

| Part | What shows |
|---|---|
| Label | "Last night" |
| Line | "6 h 40 min asleep, 11:50 pm to 7:05 am" and, when outside your usual, "· less than your usual" |
| Beside it | The night bar |
| Receipt | "Sleep · Health · {phone}", opening the night's own page |
| Before it comes in | Nothing. No placeholder, no reminder |
| Read aloud | Never. When the page is read aloud the line is skipped and nothing replaces it |

The page is written overnight, often before the person wakes, so the night usually comes in after
the page is written: the line appears when the phone next sends, as an answer to the page's
question does. It is the only health line on the page ([decision H6](#decisions-for-the-owner)).

### In the journal

On the computer, the journal day gains:

- **Beside the morning check-in.** The day's first check-in row carries "after {duration} asleep" in
  the secondary ink, a receipt to the night. The check-in itself, on the phone and the computer,
  shows "Last night: {duration} asleep" above the field, as context, when the night has come in.
- **Workouts on the timeline.** "Walk · 42 min · 3.1 km" at its start time, with the workout mark.
- **The night on the timeline.** "Woke · {duration} asleep" at the waking time, first in the
  morning group.
- **A side block, "From Health".** Under "How today felt": the night bar for the night that ended
  the day; resting heart rate today on a 30-day line with your usual band; steps on 14 day bars;
  the day's heart rate range; any blood pressure readings of the day. Each has "See more ›" to its
  own page. Kinds not brought are not shown; the block is not shown when nothing is brought.
- **Day markers.** The lines mark the days with a journal entry, so a trend sits beside the days
  written about.

The phone's journal shows the same, stacked: the night under the date, workouts and the night on
the timeline, and "From Health" as a card after the timeline with the same lines.

### Reflections and recaps

- **The day's reflection** may mention the night or a workout, with its receipt: "You slept less
  than usual, then walked to the allotment." It never comments on a number beyond that.
- **Week, month and year recaps** gain a "From Health" section after "How it felt": the nights
  strip, steps as day bars, resting heart rate as a line, workouts counted ("4 walks, 1 swim")
  with each one a receipt. Blood pressure and weight are listed by day only on their own pages, not
  in recaps.

### Noticing

At most a few sentences, only in recaps, in the existing "What tended to go together", under its
existing rule: "Shown only when it held at least three times. It says what went together, never
why."

| Rule | Detail |
|---|---|
| Where | Week, month and year recaps, on the computer and the phone. Never on the morning page, never read aloud, never in a notification |
| What it may use | Sleep, steps and workouts, with things from the journal: people, places and things named in entries, check-in parts, good things ([decision H4](#decisions-for-the-owner)) |
| What it never uses | Blood pressure, heart rate, resting heart rate and weight |
| When it is shown | At least three days with the thing and three without, a difference of at least 30 minutes of sleep, 2,000 steps or one workout, and the pattern holding on at least three of the days |
| How it is said | What went together, in counts and the person's own units: "On the 4 nights before days you wrote about deadlines, you slept 50 minutes less than on other nights." Never why, never advice, never a judgement |
| Receipts | Both sides: each night or day counted is a receipt, and each journal item that named the thing. Shown as the first two chips and "+{count}" |
| Where it is made | On this computer, from what is kept, like every recap line. It never goes online |
| Turning it off | "Notice what goes together" in From your phones, under Health, on by default ([decision H3](#decisions-for-the-owner)) |

Examples, with their receipts:

| Sentence | Receipts |
|---|---|
| "On the 4 nights before days you wrote about deadlines, you slept 50 minutes less than on other nights." | 4 nights, 4 entries |
| "On 3 of the 4 days you walked for more than half an hour, you checked in as settled." | 4 workouts, 3 check-ins |
| "You took about 3,000 more steps on the 5 days with Leo’s football in your journal." | 5 days of steps, 5 items |

## Connecting Health on the phone

The same three steps as every ask in the [phone apps](phone-apps.md#4-the-asks): Nocturne's screen,
the system's own, then a screen for the answer.

**1. Why, first.**

| Part | Copy |
|---|---|
| Label | "Health" |
| Title | "Bring health readings to your computer?" |
| What is read | "Nocturne reads the kinds you choose from {Apple Health or Health Connect}: sleep, heart rate, resting heart rate, blood pressure, steps and workouts." |
| How far back | "From the last 30 days, then each new reading." |
| Where it goes | "Only to your computer, in locked messages only your computer can open. Nocturne has no account and keeps nothing anywhere else." |
| What it is for | "Your journal shows how you slept and moved beside what you wrote, and your page can show last night’s sleep." |
| Never | "Nocturne never adds or changes anything in Health. It can’t tell what your numbers mean for your health. A doctor or nurse can." |
| Next | "Next, choose which kinds. Then your phone asks you to confirm." |
| Buttons | "Continue" / "Not now" |
| Link | "Privacy policy" |

**2. Which kinds.** A Nocturne screen, so the system's screen only confirms. Each kind is a row with
a switch and its reason in one line.

| Row | Hint | Starts |
|---|---|---|
| "Sleep" | "Beside your morning check-in, and on your page." | On |
| "Heart rate" | "The day’s range, beside your journal." | On |
| "Resting heart rate" | "One number a day, as a line over the weeks." | On |
| "Blood pressure" | "Your readings by day, if you measure at home." | On |
| "Steps" | "How much you moved each day." | On |
| "Workouts" | "Walks, runs and swims on your day’s timeline." | On |
| "Weight" | "A line over the months. Never on your page." | Off |

Button: "Continue". Under it: "You can change these any time in {thisPhone}."

**3. The system's own screen.** Health Connect's permissions screen on Android, listing the same
kinds to allow; Apple Health's access sheet on iPhone, with each kind under "Allow “Nocturne” to
read". Nocturne asks only to read, never to write.

**4. The answer.**

| Case | What shows |
|---|---|
| Android, some or all allowed | "Bringing {kinds}." and "Readings show on your computer as they go across. The first 30 days can take a minute." |
| Android, none allowed | "Health stays on this phone." and "Nocturne works just the same without it. You can bring it later in {thisPhone}." |
| iPhone, any answer | "Done." and "Readings show on your computer as they go across." iPhone never tells an app which kinds it may read, so Nocturne cannot tell a refusal from no readings |
| iPhone, nothing came in after the first send | In This phone, under Health: "Nothing has come in from Health yet. If you turned kinds off, you can turn them on in the Health app: open your profile, then Apps, then Nocturne." |
| Android, Health Connect missing (Android 13 and older) | "Health Connect is needed" and "On this phone, health readings come through Health Connect, Google’s free app. Get it, then come back here." with "Get Health Connect" |

**The rationale screen (Android).** Health Connect opens it from its own "privacy policy" link
next to Nocturne. It is a page inside the app, readable without pairing.

| Part | Copy |
|---|---|
| Title | "How Nocturne uses your health readings" |
| What | "Nocturne reads only the kinds you allow: sleep, heart rate, resting heart rate, blood pressure, steps, workouts and, if you turn it on, weight. It never writes to Health Connect." |
| Why | "To show your readings beside your journal and last night’s sleep on your morning page, on your own computer." |
| Where | "Readings go only to the computer you paired, in locked messages only it can open. Nobody else receives them, including the people who make Nocturne. Nocturne has no account, no ads and no analytics." |
| Keeping | "Your computer keeps readings until you delete them there." |
| Stopping | "Turn Health off in Nocturne’s {thisPhone}, or take the permission back in Health Connect. To delete what went across, use {fromYourPhones} on your computer." |
| Link | "Read the full privacy policy" |

### On the phone

This phone's "Health" row opens:

| Part | Copy |
|---|---|
| Switch | "Bring health readings" |
| Kinds | The seven rows above, with their switches |
| From | "Readings from {date} onwards" |
| Older | "Also bring older readings" when [decision H5](#decisions-for-the-owner) allows it: "Up to a year back, where your phone has them." |
| Last sent | "Last sent {time} · {count} readings" |
| Fixed line | "Nocturne can’t tell what your numbers mean for your health. A doctor or nurse can." |
| Already sent | "What already went across stays on your computer. To delete it, use {fromYourPhones} there." |
| Taken back | "Health is off in {Health Connect or the Health app}." and "Nothing is read until you turn it back on there." with "Open Health Connect" on Android |

## On the computer

**From your phones, Health.** The row in [From your phones](phone-apps.md#from-your-phones):

| Part | What shows |
|---|---|
| Row | "Health" / "{kinds}" / "last came in {time}" |
| Actions | "See readings", "Stop taking it…", "Delete what came in…" |
| Noticing | "Notice what goes together" / "A sentence in your recaps when sleep, steps or workouts went with something in your journal, with its receipts." |
| On your page | "Sleep last night", in the page group |

**See readings** opens the sleep page, with the other kinds as tabs: Sleep, Heart rate, Resting
heart rate, Blood pressure, Steps, Workouts, Weight (when on).

**A kind's own page.**

| Part | What shows |
|---|---|
| Title | The kind, "Sleep" |
| Range | "Last 30 days", "Last 90 days", "This year" |
| Chart | The kind's form over the range, with your usual and the day markers |
| Sentence | The chart's sentence |
| Fixed line | "Nocturne can’t tell what your numbers mean for your health. A doctor or nurse can." |
| List | By day, newest first: the time, the value, the source ("Watch, through Health on My phone"), and the journal day as a link. A removed reading is in the secondary ink with "Removed in Health · not in charts" |
| Each row | "Delete…" in its menu |
| Foot | "Print these readings" and "Delete readings…" |

**Delete readings.** The existing delete dialog with a choice of kinds and "All of them" or "Before
{date}", listing:

- "{count, plural, one {# reading} other {# readings}} of {kinds} from {phone}",
- "{count, plural, one {# line in your reflections and recaps cites them and will be taken out.} other {# lines in your reflections and recaps cite them and will be taken out.}}",
- "{count, plural, one {# sentence about what went together will be worked out again.} other {# sentences about what went together will be worked out again.}}",
- "Health on your phone isn’t touched. Readings you delete here aren’t sent again.",
- and the existing line "Anything you exported or printed is out of reach. Nocturne can’t delete it."

Buttons: "Delete" in the careful style and "Keep them".

**Delete this day** lists the day's readings under "From Health", unticked.

**Stop taking it** works as for the calendar. What came in stays.

### Export

One table per kind in the export, in plain comma-separated text: day, time, value, unit, source,
and "removed in Health" where it applies.

## Privacy

Readings take the journal's rule, the closest one Nocturne already has ([decision H1](#decisions-for-the-owner)):
shown in full where the person looks, and never anywhere that speaks or shows itself.

| Place | Health readings |
|---|---|
| The journal, the page, a kind's own page, recaps | Shown |
| A paired phone | Shown, everything, as the journal |
| Print and export | Included |
| Read aloud | Never |
| Notifications, the tray, a phone's lock screen | Never |
| Online answers | Never, because Ask does not use readings. If a later design lets Ask use them, the health, money and ID check keeps them back unless the person chose otherwise ([decision 0011](/adr/0011-online-answers.md)) |
| Marked private | Not offered, as for journal items |
| Noticing | Worked out on this computer only |
| Local only | Read on the phone, sent only to the person's own computer through the sealed channel. No health company is contacted by Nocturne |
| Read only | Nocturne never writes to Health |

## Receipts

1. **Every number cites its readings.** "6 h 40 min asleep" opens the night; "8,412 steps" opens the
   day's total and its source.
2. **Every dot is a receipt.** A point on a line opens that day's readings.
3. **Sentences cite both sides.** A noticing line cites each night or day counted and each journal
   item that named the thing.
4. **Removed readings are never cited.** A sentence whose receipts fall below the rule when a
   reading is removed or deleted is worked out again or taken out.

## The stores

Check each rule again at submission; store rules change.

| Store rule | Where it is answered |
|---|---|
| Health Connect: each kind read is declared, and only those kinds are asked for | The seven kinds, weight only when on; background reading only with [decision 2](phone-apps.md#decisions-for-the-owner) B, and readings older than 30 days only with [decision H5](#decisions-for-the-owner) B, each an extra permission |
| Health Connect: a screen explaining how the app uses the readings, opened from Health Connect | The rationale screen above |
| Google Play: the health apps declaration, saying what each kind is used for | The "Why it is in" column of [What Nocturne reads](#what-nocturne-reads) |
| Apple: a Health purpose string for reading | "Nocturne reads the health kinds you choose, such as sleep and heart rate, and sends them only to your own computer to show beside your journal. It never changes anything in Health." No write purpose string, because Nocturne never writes |
| Apple: Health readings never used for advertising, never stored in iCloud, and a privacy policy | Nocturne has no advertising and no storage of its own beyond the person's computer; the privacy policy says so |

## Accessibility

- Every chart has its sentence as its accessible name and "Show as a list" as a table of the
  numbers; colour never carries the meaning, because there is only one colour.
- Charts keep a 3 to 1 contrast for marks against their background, and text keeps 4.5 to 1.
- Numbers are read as words with their units: "fifty-eight beats a minute".
- Tapping a point is never the only way to its readings; the list holds the same receipts.

## Copy

| Key | Copy |
|---|---|
| Why, title | "Bring health readings to your computer?" |
| Why, read | "Nocturne reads the kinds you choose from {source}: sleep, heart rate, resting heart rate, blood pressure, steps and workouts." |
| Why, how far | "From the last 30 days, then each new reading." |
| Why, goes | "Only to your computer, in locked messages only your computer can open. Nocturne has no account and keeps nothing anywhere else." |
| Why, for | "Your journal shows how you slept and moved beside what you wrote, and your page can show last night’s sleep." |
| Why, never | "Nocturne never adds or changes anything in Health. It can’t tell what your numbers mean for your health. A doctor or nurse can." |
| Why, next | "Next, choose which kinds. Then your phone asks you to confirm." |
| Kinds | "Sleep" / "Heart rate" / "Resting heart rate" / "Blood pressure" / "Steps" / "Workouts" / "Weight" |
| Kind hints | "Beside your morning check-in, and on your page." / "The day’s range, beside your journal." / "One number a day, as a line over the weeks." / "Your readings by day, if you measure at home." / "How much you moved each day." / "Walks, runs and swims on your day’s timeline." / "A line over the months. Never on your page." |
| Kinds, foot | "You can change these any time in {thisPhone}." |
| Bringing | "Bringing {kinds}." / "Readings show on your computer as they go across. The first 30 days can take a minute." |
| Refused | "Health stays on this phone." / "Nocturne works just the same without it. You can bring it later in {thisPhone}." |
| iPhone done | "Done." / "Readings show on your computer as they go across." |
| iPhone quiet | "Nothing has come in from Health yet. If you turned kinds off, you can turn them on in the Health app: open your profile, then Apps, then Nocturne." |
| Missing | "Health Connect is needed" / "On this phone, health readings come through Health Connect, Google’s free app. Get it, then come back here." / "Get Health Connect" |
| Rationale | As in [the rationale screen](#connecting-health-on-the-phone) |
| Phone, switch | "Bring health readings" |
| Phone, from | "Readings from {date} onwards" |
| Phone, older | "Also bring older readings" / "Up to a year back, where your phone has them." |
| Phone, last sent | "Last sent {time} · {count, plural, one {# reading} other {# readings}}" |
| Fixed line | "Nocturne can’t tell what your numbers mean for your health. A doctor or nurse can." |
| Taken back | "Health is off in {place}." / "Nothing is read until you turn it back on there." / "Open Health Connect" |
| Page line | "Last night" / "{duration} asleep, {from} to {to}" / "· less than your usual" / "· more than your usual" |
| Phone, Today | "Last night" / "{duration} asleep" / "{from} to {to}, awake {count, plural, one {once} other {# times}}." |
| Sleep receipt | "Sleep · Health · {phone}" |
| Check-in row | "after {duration} asleep" |
| Check-in context | "Last night: {duration} asleep" |
| Timeline, woke | "Woke · {duration} asleep" |
| Timeline, workout | "{kind} · {length}" / "{kind} · {length} · {distance}" |
| Side block | "From Health" / "See more ›" |
| Usual | "Your usual is {low} to {high}" / "Your usual shows after two weeks of readings." |
| Comparisons | "more than your usual" / "less than your usual" / "higher than your usual" / "lower than your usual" |
| Blood pressure | "{top} over {bottom}" |
| Units, first time | "{count} beats a minute" / "{count} steps" / "{hours} h {minutes} min asleep" |
| List view | "Show as a list" / "Show as a chart" |
| Removed | "Removed in Health · not in charts" |
| Recap section | "From Health" / "{count, plural, one {# walk} other {# walks}}" (and each workout kind) |
| Noticing switch | "Notice what goes together" / "A sentence in your recaps when sleep, steps or workouts went with something in your journal, with its receipts." |
| Kind page ranges | "Last 30 days" / "Last 90 days" / "This year" |
| Kind page foot | "Print these readings" / "Delete readings…" |
| Delete, rows | "{count, plural, one {# reading} other {# readings}} of {kinds} from {phone}" / "{count, plural, one {# sentence about what went together will be worked out again.} other {# sentences about what went together will be worked out again.}}" / "Health on your phone isn’t touched. Readings you delete here aren’t sent again." |
| Delete this day, group | "From Health" |
| Source | "{device}, through Health on {phone}" |
| iPhone purpose, Health | "Nocturne reads the health kinds you choose, such as sleep and heart rate, and sends them only to your own computer to show beside your journal. It never changes anything in Health." |

## Out of scope

- Ask answering from readings ([decision H7](#decisions-for-the-owner)).
- The kinds left out above, including cycle tracking and medications, each of which needs its own
  design.
- Writing to Health, goals, reminders and any notification.
- Sharing readings with anyone, including a doctor, beyond Print and export.
- Readings from a watch or scale that does not go through Apple Health or Health Connect.

## Decisions for the owner

The owner accepted every recommendation on 2026-11-21: the Recommendation column is the decision.

| # | Question | Options | Trade-offs | Recommendation |
|---|---|---|---|---|
| H1 | Where health readings may appear | A: the journal's rule: shown in the app, on the phone, in Print and export; never read aloud, notified, in the tray or on a lock screen. B: like an ordinary thing kept: read aloud with the page | A reuses a rule Nocturne already has and keeps numbers about the body out of a room that may not be private when the page is read aloud. B lets the sleep line be heard, at the cost of saying health numbers aloud | **A** |
| H2 | Which kinds | A: sleep, heart rate, resting heart rate, blood pressure, steps and workouts, with weight offered and off. B: only sleep, heart rate, resting heart rate and blood pressure. C: A plus heart rate variability, blood oxygen and cycle tracking | A covers what most phones have and what helps a journal, with a reason for each. B leaves out steps and workouts, the two most phones have without a watch. C adds kinds that are noisy, medical or need their own design | **A** |
| H3 | Noticing | A: in recaps only, at least three times, both sides cited, on by default with a switch. B: also one line on the morning page. C: none | A gives the "you slept less on days you wrote about deadlines" moment where looking back already happens, calmly. B puts a body sentence in front of every morning, which can feel like being watched. C is safest but loses the one thing a journal and health together can show | **A** |
| H4 | What noticing may use | A: sleep, steps and workouts. B: also heart rate, resting heart rate and blood pressure | A stays with everyday things anyone can read. B can find more, but a sentence about blood pressure next to a journal topic reads like a medical finding | **A** |
| H5 | How far back on first connecting | A: the last 30 days. B: A, plus "Also bring older readings", up to a year where the phone allows. C: from today | A is enough to show your usual within two weeks and is what Health Connect gives without an extra permission. B lets someone see a longer line, with one more permission and a larger first send. C starts empty | **B**, with 30 days as the default |
| H6 | Health on the morning page | A: last night's sleep only. B: sleep and resting heart rate. C: nothing; health only in the journal | A gives the one number that matters to a morning, without turning the page into a dashboard. B adds a number most people cannot read. C keeps the page as it is but loses sleep beside the day ahead | **A** |
| H7 | Ask and readings | A: Ask does not use readings in the first release. B: Ask answers from readings on this computer, kept back from online answers by the health check | A keeps answers about the body out until their wording is designed; the charts already answer "how did I sleep last week?". B is more useful but needs its own design for numbers in answers | **A** |
| H8 | Weight | A: offered, off by default, never on the page or in noticing. B: left out. C: on with the rest | A serves people who track it without putting it in front of anyone who did not ask. B is simplest. C shows a number many people find hard to see every day | **A** |
