---
type: Design
title: Phone and computer as equal starting points
description: Roadmap proposal for a Nocturne that can start on a phone as well as a computer, with a ladder from a phone alone, to a locked copy online and an online mind, to a household computer that lends its mind to any phone holding the code its owner shows, to a computer of one's own with companion phones, plus personas, journeys, what each mode shows, who sees what, consent, invariant and decision changes, phasing, each roadmap item with its decisions and open questions, risks and the owner's decisions.
status: draft
tags: [proposal, roadmap, ux, phone, household, online-answers, privacy, copy]
---

# Phone and computer as equal starting points

Today Nocturne starts on a computer. A phone is a companion: it keeps, asks and reads the page
while the computer holds the memory and does the thinking ([Nocturne on Android and
iPhone](phone-apps.md)). That leaves out people who have no computer, or none that is on at night,
and it makes a household with one computer choose whose memory it keeps.

This page proposes the way there: a person can start on either, and grow from there. It is a
roadmap, not one build. Each step is a design of its own, on the pages below, and each needs the
owner's decisions before it is built.

Status: proposed, with the owner's decisions folded in ([Decisions](#decisions)). Nothing here is
built. The design for today's companion apps is unchanged and ships first.

| Page | What it covers |
|---|---|
| This page | The ladder, personas, journeys, what each mode shows, who sees what, rule changes, phasing, risks, decisions |
| [Nocturne on a phone of its own](phone-first.md) | Step 1 and step 2: first run without a computer, Today with no mind, a locked copy online, choosing an online mind, their consent screens and copy |
| [A household computer](household-server.md) | Step 3 and hosting in step 4: borrowing a household's mind with its code, what the person who runs it can and cannot see, making a household computer your home by pairing, the mind being away, leaving, removing, the household screen on a computer, copy |
| [Moving your memory](moving-memory.md) | One home at a time: moving a memory from a phone to a computer or a household computer, and back, always by pairing, with what happens to copies and the phone afterwards |

## Mockups

In `docs/design/mockups/phone-parity/`, with the kit and rules of the [phone app
mockups](phone-apps.md#mockups): serve the repository root over HTTP after `make ui/install`, add
`?state=` and `&theme=light` or `&theme=dark`. `index.html` links every view. Phone screens show
Android (412 by 892) and iPhone (393 by 852) side by side, rendered 920 by 1000. Computer screens
are 1280 wide, full length. A dashed outline marks what is new on an existing screen; it is not part
of the design. All names and data are fictional.

| Screen | File and states | Page |
|---|---|---|
| First run on a phone, and Today with no mind | `first-run.html`: `welcome`, `start`, `today`, `ask` | [Phone first](phone-first.md#screens) |
| The first release of a phone alone | `first-release.html`: `welcome`, `start`, `today`, `reminder`, `full`, `ask`, `mind`, `backup`, `saved`, `restore`, `item`, `this-phone`, `calendar`, `health`, `delete`, `damaged` | [Phone first](phone-first.md#mockups-for-the-first-release) |
| A locked copy online | `copy-online.html`: `why`, `passphrase`, `on` | [Phone first](phone-first.md#a-locked-copy-online) |
| Choosing a mind | `mind.html`: `choose`, `service`, `consent`, `apple`, `first-page` | [Phone first](phone-first.md#choosing-a-mind) |
| Joining a household, and a passphrase for making it your home | `household.html`: `found`, `see`, `passphrase`, `words`, `joined` | [Household](household-server.md#joining) |
| The mind is away | `away.html`: `today`, `journal`, `back`, `removed` | [Household](household-server.md#states) |
| This phone in each mode | `this-phone.html`: `alone`, `guest`, `member` | [Phone first](phone-first.md#8-this-phone), [Household](household-server.md#this-phone) |
| The household on a computer | `desktop-household.html`: `household`, `add`, `allow`, `remove`, `off` | [Household](household-server.md#on-the-computer-that-runs-it) |
| Moving your memory | `move.html`: `found`, `size`, `moving`, `moved`; `desktop-move.html`: `receive`, `done`, `choose`, `merged` | [Moving](moving-memory.md#screens) |

## Goals

1. **Start anywhere.** A person can install Nocturne on a phone with no computer and use it from the
   first minute: keep, write the journal, check in, and see their calendar and health beside their
   days.
2. **Grow without starting over.** Adding a mind, a household computer or a computer of one's own
   keeps everything kept so far, with its receipts.
3. **One memory, one person, one home.** A memory belongs to one person and lives in one place at a
   time. A copy is a locked copy of that home, never a second home that must be merged.
4. **Exact about who sees what.** Every step that lets something leave the phone says what goes,
   to whom, what they can see, and how to stop, before anything is turned on.
5. **The same Nocturne in every mode.** Today, Journal and Ask keep their place and words; a mode
   changes what fills them, not where they are.

## Not goals

- Two-way sync, or the same memory edited in two places.
- A Nocturne account, a hosted service, or anything run by the project that holds people's data.
- Using a person's Claude, Gemini or ChatGPT subscription as Nocturne's mind (see
  [Choosing a mind](phone-first.md#choosing-a-mind) for why).
- Nocturne as a tool inside another assistant. Parked for a separate design.
- A shared household memory. A household shares a computer's mind, never a memory.
- Anything that copies a memory to the computer whose mind it uses. Using a mind never puts the
  memory there; only pairing to make that computer its home does.
- The phone's own mind, beyond a placeholder: it is the moonshot below.

## The ladder

```mermaid
flowchart LR
    S1["1 · A phone alone<br/>memory on the phone<br/>no mind yet"]
    S2["2 · Online<br/>a locked copy online<br/>and an online mind<br/>complete as it is"]
    S3["3 · A household computer<br/>borrow its mind with its code<br/>or pair to make it your home"]
    S4["4 · A computer of your own<br/>your phones are companions"]
    M["Later · the phone's own mind"]
    S1 --> S2
    S2 --> S3
    S1 -->|pair, move your memory| S4
    S3 -->|pair, move your memory| S4
    S4 -. lends its mind .-> G["Household phones with its code<br/>borrow its mind"]
    M -.-> S1
```

| Step | Where the memory lives | Where the mind is | What leaves the phone | Today |
|---|---|---|---|---|
| 1 · A phone alone | On the phone | None yet: nothing is read, no page | Nothing | New |
| 2 · Online | On the phone, with a locked copy online if chosen | An online service the person chose, or Apple Intelligence on iPhone | The locked copy, to the person's own online storage. Words to be read and questions, to the mind they chose | New |
| 3 · Borrow a household's mind | On the phone | A household computer whose owner showed this phone its code | What each request in flight needs (a thing to read, a question with its few lines), sealed, to that computer, which keeps nothing of it | New |
| 3 · A household computer as your home | On the household computer, locked apart | That computer | Everything kept, sealed, to that computer, only because the person paired and moved their memory there | New |
| 4 · A computer of your own | On the computer | The computer | What phones keep and bring, sealed, to the person's own computer | Today's build |
| Later · the phone's own mind | On the phone | On the phone | Nothing | Placeholder |

The steps are built in this order: a phone alone, then online (the locked copy and an online mind),
then the household. That is the order they are built, not a path a person is led along. **A phone
alone with an online mind is a complete Nocturne**: a person can use it for good and never connect a
computer or a household, and no screen, reminder or line of copy nudges them towards one. The other
choices are listed where a mind is chosen and nowhere else. Once the steps exist, a person may still
move between them in any order: they can go from 1
straight to 4, borrow a household's mind from 2, or come back down from 4 to 1 by [moving their
memory](moving-memory.md). The interface never numbers them. It shows two plain facts in This
phone: where your memory is, and where its mind is.

Two rules hold on every step:

- **Using a mind never moves a memory.** An online mind, Apple's servers and a household's mind each
  read what a request needs and keep nothing of it. A household computer that lends its mind keeps no
  memory, file, copy or history of a phone that only borrows it.
- **A memory moves only by pairing.** The one way a memory comes to be held by another machine is its
  person pairing with that machine to make it their home: their own computer, or a household computer
  they choose. The locked copy online is a locked copy of that one home, never a second home.

### The phone's own mind, later

A placeholder only, so nothing here blocks it. When phones can read and tidy on their own at a
quality and battery cost the owner accepts, a phone alone gains a mind that never leaves it. The
owner has chosen the shape for when that comes: the same engine as the computer's smallest mind,
inside the app, with the phone's built-in minds as a quicker way to answer later; phones with 8 GB
of memory or more, Android 12 or later, iOS 17 or later. It would appear as one more choice in
[Choosing a mind](phone-first.md#choosing-a-mind), "On this phone", and changes nothing else on
these pages.

## Personas

Fictional people, each standing for a group the ladder serves.

| Persona | Who | Devices | Where they start | Where they go |
|---|---|---|---|---|
| **Maya** | Nurse on shifts, keeps a journal, no computer at home | An Android phone | Step 1, a phone alone | A locked copy online and an online mind, and stays there: no computer, no household |
| **Tom and Ines** | A couple. Tom set up a small computer in the hall that is always on; Ines is careful about who holds her things | Tom: iPhone. Ines: Android. The hall computer, called Hallway | Hallway is Tom's own Nocturne; his phone is its companion. Ines keeps her memory on her phone, and Tom shows her Hallway's code so she can borrow its mind | Ines later pairs to make Hallway her home; one day she moves out and takes her memory with her |
| **Sam** | Parent with a desktop, uses today's build. Hosts the household | A desktop called Home computer, two companion phones | Step 4 | Invites Leo (their son) to borrow Home computer's mind. Invites Gran, who has no computer, to make it her home. Leo later buys a laptop and moves his memory there |

## Journeys

Each journey is the path through the screens on the sub-pages. Words in quotes are interface copy.

### Maya: a phone alone, then online, for good

```mermaid
flowchart TD
    I[Install Nocturne] --> W[Welcome]
    W -->|Start on this phone| ST[Your memory lives on this phone: what works now, what waits for a mind]
    ST --> OF[Bring more from this phone? calendar, health]
    OF --> T1[Today: calendar, last night, check-in, kept today. No page yet]
    T1 -->|a week later, This phone| CP[A locked copy online: why, then a passphrase]
    CP --> T1
    T1 -->|Choose a mind| CM{Where should its mind be?}
    CM -->|An online service you choose| SV[What goes, then its details, a limit, then agree]
    CM -->|iPhone: Apple Intelligence| AP[What goes, then agree]
    CM -->|A computer at home| HH[Borrow a household's mind with its code]
    SV --> FP[Reading what you kept, then the first page next morning]
    AP --> FP
    FP --> EV[Every morning: her page from what she kept, answers in Ask, a copy made each day]
```

1. **Welcome.** "Start on this phone", "Connect to my computer", and, once households ship, "Join a
   household".
   Maya has no computer, so she starts on the phone.
2. **What works now.** One screen says her memory lives on this phone, what works from the first
   minute (keep, the journal, check-ins, calendar and health), and what waits for a mind (reading
   photos and voice notes, answers, the morning page). It says plainly that if the phone is lost,
   so is the memory, unless she keeps a locked copy online.
3. **Today with no mind.** The date, her calendar today, last night's sleep, today's check-in and
   what she kept, with one quiet card: "Your page needs a mind to write it."
4. **A locked copy online.** From that card or This phone: what goes (one locked file), where (her
   own Google Drive), who can open it (only someone with her passphrase), then the passphrase.
5. **Choosing a mind.** She chooses an online service with her own key. The steps follow Online
   answers: what goes, then its details, a monthly limit, then agreeing. Nocturne reads what she kept, in turn, and
   her first page comes the next morning.
6. **For good.** That is where Maya stays. Her page comes every morning, Ask answers with receipts,
   and her locked copy is made every day. Nothing suggests a computer or a household; if she ever
   buys a laptop, "Move my memory…" in This phone is there, as Leo's journey shows.

### Tom and Ines: one household computer, one household code

```mermaid
flowchart TD
    H[Tom: Hallway runs Nocturne, his phone is its companion] --> HO[Tom turns on Household]
    HO --> AT[Tom: Show the code]
    AT --> IJ[Ines's phone, already a phone alone: Join a household, scan]
    IJ --> IS[What Tom can see: Hallway keeps nothing of yours, not even that your phone exists]
    IS --> IT[Ines: Today, her page written by Hallway's mind]
    IT --> AW[Power cut: Hallway is away]
    AW --> AI[Ines: keeping and journal work, reading waits]
    AI --> BK[Hallway back: reading resumes]
    IT -->|later| IH[Tom: Keep someone's memory here, a code for Ines alone]
    IH --> IM[Ines pairs, moves her memory to Hallway: her phone is a companion of her memory there]
    IM -->|one day| IL[Ines moves out: bring my memory to this phone]
```

1. **Tom sets up Hallway.** A computer that is always on, running Nocturne as his own home, with his
   phone as its companion, as today. He turns on Household
   ([Household](household-server.md#on-the-computer-that-runs-it)).
2. **Tom shows the code.** Hallway has one household code. Tom shows it, on Hallway or on his own
   phone; Ines scans it and is done. Hallway keeps no list of phones. If Tom ever wants a phone to
   stop, he makes a new code, which stops every phone using the old one, and shows it again to the
   people who should keep borrowing.
3. **What the person who runs it can see.** Before joining, one screen says exactly what Tom can and
   cannot see: Hallway keeps nothing of Ines's and nothing about her phone, not even a record of when
   she used it, and the limit of what Nocturne can promise on a computer someone else controls.
4. **The mind is away.** During a power cut Ines's phone keeps working as a phone alone, and reading
   waits. When Hallway is back, reading carries on by itself.
5. **Making Hallway her home.** Ines later wants Hallway to keep her memory. That is not a setting of
   borrowing: Tom makes a code for her alone to keep her memory there, she pairs with three words and
   Tom's Allow, and her memory moves, following
   [Moving your memory](moving-memory.md#to-a-household-computer). Her phone then works as a companion
   of her memory on Hallway, locked apart from Tom's.
6. **Leaving.** When Ines moves out, "Leave this household" offers to bring her memory to her phone,
   then deletes it from Hallway only when she confirms.

### Sam: a computer of their own that hosts the household

```mermaid
flowchart TD
    S[Sam: Home computer with two companion phones] --> HO[Settings, Household: lend this computer's mind]
    HO --> SC[Show the household code]
    SC --> L[Leo's phone scans it: borrows the mind]
    HO --> GK[Keep someone's memory here: a code for Gran alone, three words, Allow]
    GK --> G[Gran's phone pairs with a passphrase]
    L --> HV[Household screen: the code, in use now, Gran's memory kept here]
    G --> HV
    HV -->|Gran's phone lost| RM[Remove Gran's phone: it stops at once]
    RM --> GR[Gran's memory waits, locked. Her new phone pairs; she types her passphrase]
    HV -->|Leo buys a laptop| LM[Leo pairs his laptop and moves his memory from his phone]
    LM --> LL[Leo leaves Sam's household]
```

1. **Lending the mind.** Sam turns on Household in Settings. Home computer keeps Sam's memory and
   companion phones as today, and lends its mind to any phone holding its household code.
2. **Leo borrows the mind.** Sam shows the code from a companion phone at dinner; Leo scans it. His
   memory stays on his phone. Home computer keeps nothing of his and nothing about his phone: no
   files, no copy, no name, no record of what or when.
3. **Gran's home.** Gran has no computer, so Sam makes a code for her alone to keep her memory on
   Home computer. She pairs with three words and Sam's Allow, chooses a passphrase, and her memory
   lives there, locked apart from Sam's.
4. **The household screen.** The household code, "In use now" while any phone is being answered, and
   Gran's memory with how much room it takes and when her phone last connected. No list of borrowing
   phones exists to show. Never what was read or asked.
5. **Removing.** Gran's phone is lost; Sam removes it and it stops at once. Gran's memory stays on
   Home computer, locked with her key. Her new phone pairs, she types the passphrase she chose, and
   her page is back. Had Leo's phone been lost, Sam would make a new household code instead.
6. **Moving on.** Leo buys a laptop, pairs it, moves his memory there from his phone, and leaves
   Sam's household.

### Moving up and back down

| From | To | What the person does | What moves | Page |
|---|---|---|---|---|
| A phone alone | A computer of their own | Installs Nocturne on the computer, pairs the phone, agrees to move | The phone's memory, to the computer. The phone becomes a companion | [Moving](moving-memory.md#from-a-phone-to-a-computer) |
| A phone alone | A household, borrowing its mind | Scans the household code | Nothing moves, and nothing is kept there | [Household](household-server.md#joining) |
| A phone alone, or borrowing | A household computer as home | Given a code for them alone, pairs, agrees to move | The phone's memory, to the household computer | [Moving](moving-memory.md#to-a-household-computer) |
| A household computer as home | A computer of their own | "Move my memory" on the phone, pairs the new computer | From the household computer to their own, through the phone | [Moving](moving-memory.md#from-a-household-computer-to-a-computer) |
| A household computer as home | A phone alone | "Leave this household", "Bring my memory to this phone" | From the household computer to the phone | [Moving](moving-memory.md#back-to-the-phone) |
| A computer of their own | A phone alone | On the computer, "Move my memory to a phone" | From the computer to one paired phone, after a size check | [Moving](moving-memory.md#back-to-the-phone) |
| Any step | Online | Turns on a locked copy online, or chooses an online mind | The locked copy, or the words to read and the questions | [Phone first](phone-first.md) |

## What each mode shows

The tabs, their order and their names never change. What fills them does.

| Place | Companion (today) | A phone alone, no mind | A phone alone, online mind | Borrowing a household's mind | A household computer as home |
|---|---|---|---|---|---|
| **Today** | The computer's page, phone edition; calendar and sleep as designed | The date, "In your calendar today", "Last night", today's check-in, "Kept today", and the card "Your page needs a mind to write it." | The page, written by the online mind overnight from what it read; each line cites the original | The page, written by the household's mind when the phone could reach it, from this phone's memory | The page from the household computer, as a companion's page |
| **Journal** | The computer's journal | Today's timeline from the phone's own memory: entries, keeps, check-ins, calendar rows, health. Photos and voice notes show as kept, "waiting to be read". No reflection | As the computer's journal, with no reflections or recaps: check-ins stay on this phone (see [decision 10](#decisions)) | As the computer's journal, reflections written by the household's mind | As the companion's journal, from the household computer |
| **Ask** | Asks the computer | Finds by words in what was kept, with receipts: "Found by their words. Answers need a mind." | Answers from the online mind, citing this phone's memory; every answer says where it was written | Answers from the household's mind, citing this phone's memory | Answers from the household computer |
| **Keep** | Waits on the phone when the computer is away | Kept on the phone at once | Kept on the phone at once, read when the mind next runs | Kept on the phone at once, read when the household is reachable | Waits on the phone until the household is reachable: "waiting to go across" |
| **This phone** | Connected to; what this phone brings; when it sends; this phone; help | Where your memory is: "On this phone". Its mind: "None yet". A locked copy online. What this phone brings | Adds its mind: "{service}, online", what goes, the Sent online list | Its mind: "Hallway, at home". Household; what Tom can see; leave | Where your memory is: "On Hallway". What Hallway's owner can see; move my memory; leave |

Rules that hold in every mode:

- Receipts: every line, answer and journal row cites an original in the person's own memory.
  A thing still waiting to go across has a receipt on the phone that becomes the home's receipt
  when it arrives.
- A private mark decides only what comes up on its own: notifications, reminders, updates and a
  lock screen. Private things reach a companion phone, an online mind and a borrowed household mind
  like everything else ([decision 37](#decisions)). Where a household computer is the person's
  home, it is their own Nocturne and reads everything.
- Nothing is held back by what it seems to be about ([decision 20](#decisions)). A mind the person
  chose gets what a good answer needs, check-ins included ([decision 37](#decisions)); a borrowed
  household mind gets them sealed and keeps nothing ([decision 23](#decisions)).
- Notifications, the lock screen, reading aloud and the tray follow today's rules in every mode.

## Who sees what

Stated plainly, for each person and company that can be involved.

| Who | A phone alone | Online: a locked copy | Online: a mind | Borrowing a household's mind | A household computer as home |
|---|---|---|---|---|---|
| **The person** | Everything | Everything | Everything, and a list of what was sent | Everything | Everything |
| **Google or Apple, storing the copy** | Nothing | One locked file, its size and when it changes. Without the passphrase it can't be opened | Nothing | Nothing | Nothing |
| **The online service, or Apple** | Nothing | Nothing | What each reading, page and answer needs: each thing kept with its photo or recording, the journal and check-ins, each question with the lines that answer it, things marked private included (decision 37). Never what the person let go | Nothing | Nothing |
| **The person who runs the household computer** | Nothing | Nothing | Nothing | In Nocturne: whether the mind is in use right now, not by which phone. No list of phones, no record of when or how much, and never what was read or asked | In Nocturne: the phone's name, that the memory is kept there, how much room it takes and when the phone last connected. Never its contents. Someone with full control of that computer could still get at it while it is being read or tidied |
| **Others in the household** | Nothing | Nothing | Nothing | Nothing: no one in a household sees another's phone, memory or use | Nothing |
| **The connection's company (Cloudflare for household phones, Cloudflare or Tailscale otherwise)** | Nothing | Nothing | Nothing | As Away from home: when the phone connects, from where, sizes and timing. Never content | As Away from home |
| **The Nocturne project** | Nothing | Nothing | Nothing | Nothing | Nothing |

The household line is the one most easily over-promised, so the joining screens use exactly these
words ([What the person who runs it can see](household-server.md#what-the-person-who-runs-it-can-see)).
Nocturne on the household computer works on a borrowed thing only in its memory while it reads or
answers, and keeps nothing: no memory, no file, no copy, and nothing about the phone at all. It
keeps one household code, which any phone holding it may use. A memory whose person made that computer their home is locked with a key their phone holds, so it can't
be browsed, copied or backed up readable. While it is read or tidied it is open in that computer's
memory, and someone with full control of that computer could change Nocturne or read that memory.
Nocturne can't prevent that, and says so.

## Consent and disclosure

Each new way out follows the Online answers pattern ([Keeping the next step in
view](step-flow-actions.md)): a light first step with three short true lines and one expander, the
main button kept in a bar with "Nothing is turned on until the last step.", and a last step that
shows every fact in full before the person agrees.

| What turns on | First step's three lines | Last step | Page |
|---|---|---|---|
| A locked copy online | What goes · Who can open it · You need | The passphrase, with what is lost without it | [Phone first](phone-first.md#a-locked-copy-online) |
| An online mind | Sent · Private · You need | Everything sent, an example, who sees it, what changes, changing your mind | [Phone first](phone-first.md#choosing-a-mind) |
| Joining a household with its code | What goes · What it keeps · Who runs it | What the person who runs it can and cannot see, then Join | [Household](household-server.md#joining) |
| Lending a computer's mind | What they get · What you see · What it costs this computer | The household code, which anyone shown it may use until a new code replaces it | [Household](household-server.md#on-the-computer-that-runs-it) |
| Moving a memory, to any home | Where it goes · How big it is · What happens here after | Confirm after it has arrived, before anything is deleted | [Moving](moving-memory.md) |

## Rule changes this needs

Proposed wording. The owner approved it clause by clause: each clause goes into `AGENTS.md` when its
record (0019, 0020, 0021) is accepted. None is made by this page. The online mind clause (0020, the
key route) is in `AGENTS.md` since 2026-11-23. The exact wording is in
[0018 A phone as its own home](/adr/0018-a-phone-as-its-own-home.md#proposed-contract-changes), and
each clause takes effect only with the record it names.

### The contract

| Where | Today | Proposed |
|---|---|---|
| `AGENTS.md`, first line | "Nocturne is a private desktop app that remembers your day…" | "Nocturne is a private app for the computer and the phone that remembers your day…", and "It is a Tauri 2 app (Rust core, web-view interface); on a computer it ships and supervises a private, embedded Lemonade server." |
| Invariant 2, Local only | Allows consented downloads, Away from home traffic and Online answers | Adds a locked copy online, an online mind on a phone that is its own home, and a household's mind for phones holding its code, used for the request in flight and keeping nothing. Exact wording in [0018](/adr/0018-a-phone-as-its-own-home.md#proposed-contract-changes). Also: "The phone app is held to the same rule." |
| Invariant 4, Receipts | Every answer and page line cites the original | Adds: "A thing waiting to go across has a receipt on the phone until its home has it." |
| Invariant 5, Forgetting | Never silently delete | Adds: "A memory that moves is deleted from where it was only after it has arrived and the person confirms. No one deletes another person's memory without that person being told first." |
| New invariant | | "**One memory, one person, one home.** A memory belongs to one person and lives in one place at a time. It moves only when its person pairs with a new home. Using a mind never moves it. A copy is a locked copy of that home, never a second home." |
| Experience principles, no jargon | Lists for Away from home and Online answers | Adds for these screens: cloud, sync, server (except "Apple’s servers"), host, client, guest, tenant, admin, role, upload, encrypt. "Online", "locked", "household", "copy", "Google Drive" and "iCloud Drive" are allowed. "Borrow its mind" is the interface for a guest |
| New experience principle | | From the owner's direction: "**Never judge a person's data.** Offer every source the device has, and let the person choose. No labels like work or private on their behalf, and no filtering before they choose." |

### Decision records

| Record | Change |
|---|---|
| [0018 A phone as its own home](/adr/0018-a-phone-as-its-own-home.md), proposed | The phone app keeps its own memory with the engine's store, no mind on the phone in the first release, moving between homes by the backup format over the sealed channel, only by pairing, one home at a time enforced at pairing. Reverses "a phone that works without a computer" in [Nocturne on Android and iPhone](phone-apps.md#goals) |
| [0019 A locked copy online](/adr/0019-a-locked-copy-online.md), proposed | The phone writes numbered copies, never overwritten, sealed with a key that is itself locked with the person's passphrase (the [0013](/adr/0013-passphrase-backup.md) format), to iCloud Drive on iPhone or the app's hidden Google Drive folder on Android, or a storage address the person runs. Originals go once each. The last seven copies are kept, and forgetting for good prunes older ones at once, so forgetting reaches the copy. This is the online backup; the phone's own device backup is out of scope, and the memory stays out of it. Triggers 0013's "revisit when kept somewhere other than a file they chose" |
| [0020 An online mind on a phone](/adr/0020-an-online-mind-on-a-phone.md), accepted 2026-11-23, amending [0011 Online answers](/adr/0011-online-answers.md) | On a phone alone the phone's own core calls the service, not `lemond`. The service also reads the words of what is kept and writes the page. A named second route on iPhone, Apple Intelligence on Apple's servers, with no key, against "one generic form" and "no provider-specific code". The health, money and ID check stops holding things back, on the computer too: only things marked private are held back ([decision 20](#decisions)) |
| [0021 A household mind for invited phones](/adr/0021-a-household-mind-for-invited-phones.md), proposed | One household code admits any phone holding it to borrow the mind; nothing is kept about any borrowing phone; a new code cuts off every phone holding the old one. A separate typed set of calls for borrowing with no handle on any store, a scheduler that takes turns without counting per phone |
| Later: a household computer as someone's home | The third role, keeps its memory here: one store and key per person, entered only by a code for that person and their own pairing, with its own Local only clause |
| [Nocturne on Android and iPhone](phone-apps.md#decisions-for-the-owner), decision 6 | Every calendar starts on for companions too ([decision 20](#decisions)) |
| [0010 Away from home](/adr/0010-away-from-home.md), amended | Household phones connect through Simple. Most private keeps admitting only the owner's own devices; named other people later, as an advanced choice |
| [0012 Run as a server](/adr/0012-server-mode.md), amended | "One server is one person" becomes one owner and the people who keep their memory there; Away from home is made to run without a window so the server can host phones |
| 0017 Native phone apps, on the phone app branches today | The app is one app, `is.nocturne.phone`, published by Nocturne, for every mode; the store listing loses "needs a computer" |

## Phasing

The owner's order: a phone alone, then online (the online mind first, then the locked copy), then the household.

```mermaid
flowchart LR
    P0["Now<br/>companion apps<br/>Android, then iPhone"] --> P1["1 · A phone alone<br/>own memory, calendar, health,<br/>find by words, move to a computer"]
    P1 --> P2["2 · Online<br/>an online mind,<br/>then a locked copy online"]
    P2 --> P3["3 · Household: borrow its mind<br/>one household code, the desktop app hosts"]
    P3 --> P4["4 · Household: make it your home<br/>by pairing"]
    P4 --> P5["5 · A household computer<br/>without a window"]
    P2 -.-> M["Later · the phone's own mind"]
```

| Phase | Ships | Depends on | Why this order |
|---|---|---|---|
| Now | Today's companion apps, as designed | Nothing new | Approved and being built |
| 1 | A phone alone: its own memory, keep, journal, check-ins, calendar and health on the phone, find by words, backups to a file, and moving that memory to a computer | The native app; the engine's store built for the phone; the backup format | The owner's first step: a satellite with its own data and connections. Moving to a computer ships with it, so no one is stuck |
| 2a | An online mind for a phone alone: the key route, Android first, then iPhone; Apple Intelligence later, after a trial | Phase 1; 0011 amended; 0020 and its invariant 2 clause, both in since 2026-11-23 | The first way a phone alone gets a page and answers. The owner put it first (decision 35) |
| 2b | A locked copy online, and restoring it on a new phone | Phase 1; 0019 | A lost phone must not lose a memory once people rely on it |
| 3 | Borrowing a household's mind with its code, hosted by the desktop app | Phase 1 (the phone keeps its own memory and drives its own reading); the guest calls and turn-taking on the computer | Needs nothing kept on the host, so it is the smaller household promise |
| 4 | A household computer as someone's home, by pairing | Phase 3 (the Household page and its calls); one store and key per person; moving a memory | The larger promise: another person's memory on a computer someone else runs |
| 5 | A household computer with no window (the server image) | Phase 4; 0012's blockers closed; Away from home without a window | Most households have no spare computer with a screen on all night |
| Later | The phone's own mind | Quality and battery the owner accepts | The moonshot |
| Alongside | Decision models that label what is kept, holding nothing back | [0016](/adr/0016-decision-models-shadow-first.md)'s shadow results; a mind | Labels help finding; they never decide what goes where |

## Roadmap items

Each item with its final decisions, what is still open (options and a recommendation), and what it
depends on. Every person-facing part works from the phone unless a reason is written here.

### A phone alone

- **Decided.** The phone keeps its own memory with the computer's engine, no mind yet, one home at a
  time, moved only by pairing ([0018](/adr/0018-a-phone-as-its-own-home.md)); voice notes in words
  only by the phone's own offline recogniser (decision 9). Specified to build in [The first
  release](phone-first.md#the-first-release-a-phone-alone).
- **Depends on.** The native app; the engine built for the phone; the backup format.

Approved by the owner, with voice notes in words where a platform can't do it offline from a
recording: that platform ships with voice notes waiting (A). A phone alone, and a phone alone with an
online mind, are complete end states, never steps towards a computer or a household.

### A locked copy online

- **Decided.** Decisions 1 to 5 and 19: a locked copy of the one home, in iCloud Drive or Google
  Drive's hidden app folder, or storage the person runs; photos and recordings once each; a
  passphrase once; the online backup, with the device backup out of scope; no sync of any kind
  ([0019](/adr/0019-a-locked-copy-online.md)).
- **Depends on.** A phone alone; 0019 and the invariant 2 change; an Apple developer account with
  iCloud; Google's sign-in limited to the app folder.

- **How often (owner).** Daily by default; the person can choose daily, weekly or monthly.

| Open | Options | Recommendation |
|---|---|---|
| Storage the person runs | A: one kind of address (WebDAV) first. B: WebDAV and S3 | **A**; S3 when someone asks |

### An online mind, and the phone's assistant

- **Decided.** The phone's Claude, Gemini or ChatGPT app is not a route, and Nocturne inside an
  assistant is parked (decision 21). A service with the person's own key on both phones, Android's
  default moving to the phone's own mind once it ships (decision 6). Apple Intelligence after a trial
  on iOS 27 (decision 7). It reads what is kept, photos and recordings included, writes the page,
  reflections and recaps, and answers; it gets what a good answer needs, check-ins and things marked
  private included, and private decides only what comes up on its own (decision 37, replacing 8 and
  10) ([0020](/adr/0020-an-online-mind-on-a-phone.md)).
- **Accepted.** 0020 and its invariant 2 clause, 2026-11-23 (decision 34). The first release is
  Android with the key route: Online answers' one form, the limit and the Sent online list on the
  phone ([The first release of an online mind](phone-first.md#the-first-release-of-an-online-mind)).
  Apple Intelligence is later, with its trial and its own clause.
- **Depends on.** A phone alone; for Apple, the trial.

- **Apple trial (owner, A).** It passes when a typical day's reading (about 50 things) and its page
  fit inside one day's limit, at a quality the owner signs off on a fixed set.

| Open | Options | Recommendation |
|---|---|---|
| Reading on mobile data | A: allowed. B: only on Wi-Fi. C: the person chooses | **C**, allowed by default: photos and recordings now go to be read (decision 37), so a person on a small data plan may want Wi-Fi only |

### Household: borrowing its mind

- **Decided.** One household code that any phone holding it may use, a mind only, nothing kept about
  any borrowing phone, a new code cutting off every phone holding the old one, what each request
  needs sent, private things included (decision 37), check-ins sent sealed so it writes reflections, in turn with no daily share per phone,
  through Simple (decisions 11, 14, 15, 16, 22, 23 and correction 1;
  [0021](/adr/0021-a-household-mind-for-invited-phones.md)). Specified in [A household
  computer](household-server.md).
- **Depends on.** A phone alone (the phone drives its own reading); online first, by the owner's
  order; the guest calls and turn-taking on the computer.

Nothing open.

### Household: hosting on a computer of one's own

- **Decided.** The desktop app hosts first; the server image follows 0012's blockers (decision 12).
  Any computer may host, with no check of how strong it is (owner, A). A Household page in Settings:
  the lending switch, Show the code, New code. From the owner's companion phone (owner, B): the
  switch, Show the code and New code, so the code can be shown where people are.
- **Depends on.** Borrowing its mind; Away from home in Simple.

Nothing open.

### Household: a household computer as someone's home

- **Decided.** Only by a code for that person alone and their own pairing, with three words and
  Allow, then a move (correction 2). A desktop with its own owner can be someone's home, as can a
  computer kept for the purpose (owner, A). One store per person, locked with a key their phone holds and their passphrase; the key left
  for one night so the tidy runs (decision 13); private things read like everything else, since it
  is their home (decision 15); the phone keeps its recent pages to read while away (decision 17);
  removal keeps the memory locked, deleted only after 30 days' notice (decision 18). Needs its own
  decision record.
- **Depends on.** Borrowing its mind (the Household page); moving a memory; a per-person store.

| Open | Options | Recommendation |
|---|---|---|
| How the per-person store is locked at rest | A: the store file itself encrypted with the phone's key. B: a sealed file opened into memory only while read or tidied | **A**, settled in its decision record after a measured trial; B reads a whole store into memory each night |

### Moving a memory between homes

- **Decided.** One home holds the pen; a memory moves only by pairing; the backup format over the
  sealed channel; nothing deleted until it has arrived and the person confirms; the locked copy stops
  and the person chooses to delete or keep it (decision 19). Specified in [Moving your
  memory](moving-memory.md). Nocturne never restarts for a move: a memory moving into an empty
  computer has its things brought in, as into a computer that already holds one, without the
  question (decision 31, the owner's decision of 2026-11-23).
- **Depends on.** A phone alone for the first move (phone to computer); the household home for the
  rest.

| Open | Options | Recommendation |
|---|---|---|
| A household home to a computer of one's own | A: through the phone, as designed. B: the two computers pair directly | **A**: no new pairing between machines; revisit if large memories make it slow |

### The phone's own mind

- **Decided.** The same engine as the computer's smallest mind, inside the app, with the phone's
  built-in minds as a quicker way later; phones with 8 GB, Android 12, iOS 17; one more choice, "On
  this phone", in Choosing a mind; nothing leaves the phone.
- **Depends on.** An online mind (it fixes the phone's typed mind calls); quality and battery the
  owner accepts.

| Open | Options | Recommendation |
|---|---|---|
| When to try it | A: a trial now. B: after the online mind ships | **B**: the same calls, so the trial measures only the mind |
| When it reads | A: only while charging. B: any time | **A**: battery is the risk people notice first |

### Labels from decision models

Decision models answer typed questions about a kept thing ([0016](/adr/0016-decision-models-shadow-first.md)).
Here they label what a thing is, to help finding and grouping.

- **Decided.** Labelling never holds anything back and never changes where anything goes: only what
  the person marked private is held back. Labels say what a thing is (an appointment, a purchase, a
  trip, a recipe, a person or place it names), never how sensitive it is or which part of life it
  belongs to: no work, private, health or sensitive labels. 0016's question about health, money or ID
  details may never add a mark that holds anything back. Shadow first, as 0016; anything a person sees
  comes through design first. On a phone it runs wherever its mind runs.
- **Depends on.** 0016's shadow results beating the rules; a mind (a phone alone has no labels).

- **What labels are for (owner).** Finding and grouping first (an Ask filter, "show only" in the
  journal), then shown on each item with "Remove this label".

| Open | Options | Recommendation |
|---|---|---|
| Can the person change labels | A: remove only. B: remove and add their own | **A** first; B is a different feature (the person's own tags) |

## Risks

| Risk | Likelihood | Impact | What this design does |
|---|---|---|---|
| A lost phone loses a memory | Medium | High | Says so on the first screen of a phone alone; offers a locked copy online from day one; reminds once after 30 days of keeping with no copy |
| A lost phone takes the key to a memory kept in a household | Medium | High | The key is also locked with a passphrase chosen when joining; a new phone joins and types it |
| "The person who runs it can't see your memory" is heard as a promise it isn't | Medium | High | The exact words on the joining screen include what Nocturne can't prevent; no softer wording anywhere |
| An online mind for a phone alone sends far more than Online answers does | High | High | Its own consent with every fact and a calm, plain line on what private does and doesn't do; the limit; the Sent online list on the phone |
| A household phone's reading slows the owner's own answers | High without a scheduler | High | The owner's asks first, then household asks, then everyone's reading in turn, one thing at a time per connection; the owner can turn lending off or make a new code |
| A household code passed on, or a lost phone, keeps borrowing | Medium | Low | Anyone with the code can borrow until a new code; the code never opens anyone's memory; New code is one tap on the computer or the owner's phone, and the trade-off is stated in decision 22 |
| Something of a borrowing phone lingers on the household computer | Medium | High | Recordings and photos are read from memory, never written to disk; no logs, caches or counts per phone; a test proves the host's store and data folder are unchanged after a borrowing phone's requests |
| Two homes by accident (a phone alone pairs with a computer and both keep going) | Medium | High | One home enforced at pairing: pairing a computer always ends in moving the memory, or in the phone borrowing its mind with the household code, never both homes |
| Moving a large memory to a phone that lacks space | Medium | Medium | Size check before anything moves; nothing deleted until it arrives |
| Store review of a phone that works alone and talks to online services | Low | Medium | Listing and privacy policy rewritten for each mode; every online step behind its own consent |
| Platform background limits make overnight reading on a phone unreliable | High | Medium | Reading happens when the phone allows; the page is written from what finished and says so |
| Policy drift at assistant companies | High | Low | Nothing is built on a person's assistant subscription |
| Scope creep into sync | High | High | Not a goal; the locked copy is a copy of one home, restored, never edited |

## Research that informs this page

| Question | Conclusion used here | Confidence |
|---|---|---|
| Can the phone's Claude or Gemini app be Nocturne's mind? | No. One company forbids other apps from using a person's subscription, another offers no way for an app to get an answer back, and Android's app-to-assistant calls run the other way. On iPhone, Apple Intelligence on Apple's servers can answer for an app with no key or account (iOS 27), counted on the person's iCloud. Everywhere, a service with the person's own key works | Settled for the design; Apple's route needs a trial |
| Can a household computer lend its mind without seeing what it reads? | No. It sees what it reads in its memory while it works. It can keep nothing, and its owner's view need show nothing about the phones | Settled |
| Can the owner be kept out of a memory whose person made their computer its home? | Against a curious owner, yes: a store locked with a key the phone holds can't be browsed or copied readable. Against an owner with full control of the computer, no, while it is read or tidied | Settled; the wording is in [Household](household-server.md#what-the-person-who-runs-it-can-see) |
| How do household phones reach the computer? | Simple works as it is. Most private admits only the owner's own devices. A home network alone does not work for the phone page and needs a listener the rules forbid | Settled |
| Where does a locked copy online go? | iCloud Drive on iPhone needs no sign-in; on Android the app's hidden folder in Google Drive needs one account screen and only the narrowest access. Copies are numbered and never overwritten, so a second writer is caught, not merged. The phone's own device backup should leave the memory out | Settled; iPhone storage not yet built or tried |
| Does the computer's store run on a phone? | It builds for Android unchanged; iPhone not yet built | Android settled; iPhone to confirm |

## Decisions

The owner accepted every recommendation, with five corrections that override the earlier text
where they differ:

1. **A household's mind is only a mind for the phones its owner lets in.** A phone that only borrows
   it is never stored there: no memory, no files, no copy, and no history of it beyond what answering
   the request in flight needs. A phone joins only with the code the household's owner shows, as a QR
   code or typed. The owner then chose (decision 22) that the household keeps no record of any phone
   at all: one current code admits any phone holding it.
2. **A memory moves only by pairing.** Using a mind never puts a memory on the mind's machine. The
   only time another machine holds a memory is when its person pairs with it to make it their home:
   their own computer, or a household computer they choose.
3. **The ladder's order.** A phone alone first, then the locked copy online and an online mind, then
   the household.
4. **The online backup is the locked copy** of the one home. The phone's own device backup is out of
   scope.
5. **Decision 20 is A.** Every calendar starts on, and nothing is held back unless the person marked
   it private.

Answered earlier, and used as given: full parity is the ideal and a long way off, so step 1 is a
phone satellite with its own data and connections; the phone's own mind later uses the computer's
engine in the app with the phone's built-in minds as a quicker path; phones with 8 GB, Android 12,
iOS 17; one home at a time, pairing a computer moves the memory and the phone becomes a companion
again, with no two-way sync; the app is `is.nocturne.phone`, published by Nocturne; never judge a
person's data.

"Changed" marks a row where a correction moved the decision away from the earlier recommendation.

| # | Question | Options | Decided |
|---|---|---|---|
| 1 | What a copy online is, given one home at a time | A: a locked copy of the one home, for restoring on a new phone and nothing else. B: a copy another device can open and edit | **A** |
| 2 | Where the locked copy goes first | A: each phone's own place, with no new account: iCloud Drive on iPhone, the app's hidden folder in Google Drive on Android; a storage address the person runs, as an advanced choice. B: Google Drive on both. C: only storage the person runs | **A**, with the advanced choice |
| 3 | Photos and recordings in the copy | A: all of them, each sent once, locked. B: words only. C: the person chooses | **A**, with a plain size line |
| 4 | The copy's passphrase | A: asked once when the copy is turned on, its key then kept in the phone's own safe place. B: asked for every copy. C: no passphrase; the storage company's account holds the key | **A** |
| 5 | The phone's own device backup (iCloud Backup, Android's backup) | A: leave Nocturne's memory out of it; the locked copy is the one copy. B: let it in | **A**. The locked copy is the online backup; the device backup is out of scope and the memory stays out of it |
| 6 | The online mind on Android, where no key-free route exists | A: an online service with the person's own key only. B: wait for the phone's own mind. C: A now, the phone's own mind as the default once it ships | **C** |
| 7 | Apple Intelligence on Apple's servers as a named choice on iPhone | A: add it, amending 0011. B: key route only. C: a trial on iOS 27 first, then decide | **C**, then **A** |
| 8 | What an online mind does for a phone alone | A: answers only. B: reads the words of what is kept, writes the page and answers; photos and recordings themselves never go. C: B plus photos | **B**; changed to **C** by decision 37 |
| 9 | Voice notes on a phone alone | A: kept as recordings, waiting to be read. B: the phone's own speech recogniser turns them into words on the phone, only where it works without a network | **B**, on-phone only |
| 10 | Check-ins, reflections and recaps with an online mind | A: keep the rule: check-ins never go online, so reflections and recaps wait for a home mind. B: let them go | **A**, the journal's agreed rule ([0015](/adr/0015-journal-privacy-class.md)); changed to **B** by decision 37 |
| 11 | Which household way ships first | A: a memory kept on someone's computer. B: borrowing its mind. C: both together | **B**, after a phone alone and online. Changed by corrections 2 and 3: a memory reaches a household computer only by pairing, which follows |
| 12 | What runs a household | A: the desktop app with Household turned on. B: the server image. C: A first, B when 0012's blockers close | **C** |
| 13 | What "the person who runs it can't see it" means for a memory kept there | A: separate files, not locked, and say the owner could read them. B: locked with a key the phone holds; read and tidied only while the phone is reachable. C: B, and the phone leaves the key in that computer's memory for one night so the tidy runs overnight | **C**, for a memory whose person paired to make that computer their home. A phone that only borrows the mind has no memory there to lock |
| 14 | How household phones reach the computer | A: through Simple, as today. B: also named other people on Most private. C: the home network with a local certificate | **A**, B later as an advanced choice |
| 15 | Do things marked private reach a household mind? | A: never. B: yes. C: B for a memory whose home is there, A for a borrowed mind | **C**; changed to **B** by decision 37 |
| 16 | Sharing the mind fairly | A: the owner always first. B: equal shares. C: the owner's asks, then household asks, then everyone's reading in turn, with a daily limit per phone | **C, in turn, with no daily limit per borrowing phone.** Changed by correction 1: a daily limit needs a count kept per phone. The owner can turn lending off or make a new code instead |
| 17 | A phone whose memory is kept elsewhere, while that computer is away | A: keeps no copy, as companions today. B: keeps the last page, the last week of pages it opened and its own entries, readable, marked with when | **B** for a memory whose home is a household computer; companions stay **A** |
| 18 | When the person who runs a household removes someone whose memory is kept there | A: the memory stays, locked, until that person brings it home or deletes it; the owner can delete it only after 30 days' notice shown on that person's phone. B: the owner chooses keep or delete at once | **A** |
| 19 | The locked copy online after the memory moves to a computer | A: turned off, with a choice to delete it or keep it as it was. B: carries on from the computer | **A** |
| 20 | The rule against judging data, against two earlier defaults | Holidays and birthday calendars start off. Lines that look like health, money or ID are kept back from online answers ([0011](/adr/0011-online-answers.md)). A: change both, so every calendar starts on and nothing is kept back unless marked private. B: change the calendars, keep the 0011 check as a default the person can turn off. C: keep both | **A**. Changed by correction 5. Work-profile calendars are offered like any other |
| 21 | The phone's assistant, later | A: park "Nocturne inside Gemini, Siri or Claude" for a separate design. B: design it now. C: rule it out | **A** |

The owner then answered the questions the first pass left open:

| # | Question | Options | Decided |
|---|---|---|---|
| 22 | What a household computer keeps to let a borrowing phone in | A: each phone's name and key until it leaves or is removed, with pause and remove per phone. B: nothing per phone: one household code, shown as a QR code, that any phone holding it may use; "New code" replaces it | **B**. The trade-off, plainly: there is no removing or pausing one phone. To stop a phone, lost or passed on, the owner makes a new code, which stops every phone holding the old one, and shows the new one to the people who should keep borrowing. The owner's view has no list of phones, only "In use now" |
| 23 | Check-ins and reflections with a borrowed household mind | A: check-ins go to it, sealed, and it keeps nothing, so it writes reflections. B: they stay on the phone | **A**. The journal's rule keeps check-ins from online minds; a household mind is not online |
| 24 | The proposed `AGENTS.md` wording | Approve it as written, edit it, or approve it clause by clause | **Clause by clause**, as 0019, 0020 and 0021 are accepted |
| 25 | How often the locked copy is made | A: after each keep. B: at most hourly, and daily. C: daily | **Daily by default**, and the person can choose daily, weekly or monthly |
| 26 | What makes the Apple Intelligence trial pass | A: a typical day's reading and its page fit one day's limit, at a quality the owner signs off. B: answers only | **A** |
| 27 | Which computers may host a household | A: any. B: only where the mind fits with room to spare | **A**: any computer, no check of how strong it is |
| 28 | Managing the household from the owner's phone | A: the computer only. B: the owner's companion phone too | **B**: the lending switch, Show the code and New code (decision 22 leaves nothing per phone to manage) |
| 29 | Whose computer can be someone's home | A: a desktop with its own owner, and a computer kept for the purpose. B: only one kept for the purpose | **A** |
| 30 | What labels from decision models are for | A: finding and grouping only. B: also shown on items. C: a hidden ranking signal | **A, then B** with "Remove this label" |
| 31 | Finishing a move on the computer (question 9) | A: reopen the memory in place. B: restart Nocturne, saying so before and after. C: no restart: an empty computer brings the phone's things in | **C, no restart: an empty computer brings the phone's things in** (owner, 2026-11-23). It runs as bringing things in does (decision 32), without the question, since an empty computer has nothing to choose about: kept things with their originals, how and where they were kept and their private marks, journal entries and check-ins travel; calendar and health are sent again from the phone; the phone becomes a companion and puts its memory aside (removed now or kept 30 days). Pages and reflections stay in what is put aside on the phone. Why: in the end-to-end test the restart (B, chosen earlier by build cost) gave a Simple connection a new address, and the phone, still waiting on "Finishing on {computer}…", could never reach the computer again. Restoring a backup file still restarts (0013) |
| 32 | A memory moving to a computer that already holds one | A: put the computer's memory aside and take the phone's (0013's replace). B: bring the phone's things into the computer's memory. C: use the computer's memory and bring nothing | **B or C, the person chooses on the computer** (owner). Replace leaves the move; it stays only for restoring a backup file. Bringing things in is the one merge in Nocturne, and still one home afterwards |
| 33 | How the choice in decision 32 works | Default selection; a way back after "Use this computer’s memory"; which moves it covers | **"Bring the phone’s things in" is selected when the dialog opens. After "Use this computer’s memory", what was put aside on the phone offers "Bring its things to {computer}…" later. The two choices apply to any move into a computer that already holds a memory**, not only from a phone alone (owner) |
| 34 | ADR 0020, an online mind on a phone | Accept now, or with the rest of step 2 | **Accepted now** (owner, 2026-11-23). Its clause for the key route is in invariant 2; Apple's servers join with their own clause after the trial. Real-provider runs happen only in development once this design merges; tests use a stand-in provider |
| 35 | Which of step 2 ships first | A: the locked copy. B: the online mind | **B, the online mind, then the copy** (owner, 2026-11-23) |
| 37 | What an online mind the person chose may get, and what private means | A: as decided (no originals or check-ins online; private never sent). B: what a good answer needs, check-ins, originals and private things included; private decides only what comes up on its own (notifications, reminders, updates) | **B** (owner, 2026-11-23), for the phone's online mind and the computer's Online answers. Replaces decisions 8 (now C), 10 (now B) and 15 (now B). The safeguard is a short, calm explanation of what is sent when the person chooses the provider. It covers a companion phone, which shows private things as the computer does (never on its lock screen or in notifications, reminders or updates), and a borrowed household mind, which gets what an answer needs (owner). No notice for people already using Online answers (owner) |
| | A phone alone, and a phone alone with an online mind | | **Approved.** Both are complete end states, never stepping stones; no copy nudges towards a computer or a household |
