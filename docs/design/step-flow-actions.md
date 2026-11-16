---
type: Design
title: Keeping the next step in view
description: The approved bar that keeps a step flow's main button in view, and lighter first steps for Online answers and Away from home that keep every fact a person needs before they agree.
status: stable
tags: [approved, layout, flows, online-answers, away-from-home, setup, copy, accessibility]
---

# Keeping the next step in view

A tester going through Online answers setup found step 1 too much to take in at once. At first they did not see
that there was a button to go on, and asked for the next step's button to be shown at all times, in a
bar at the bottom, kept apart from the words. They also said the page gave their eyes nowhere to
settle, and that the column on the right repeated the rows beside it.

The owner's position: trust is the problem, so detail cannot simply be dropped. It has to be balanced
against focus.

This page is the agreed design for two things: where a step's buttons live in every step flow, and
lighter first steps for Online answers and Away from home that still show every fact a person needs
before anything is turned on. Nothing on it is built yet.

## Owner decisions

| Question | Decision |
|---|---|
| Where a step's buttons live | One bar pinned to the bottom of the window on every step of every step flow (option A). Two others were considered and dropped: buttons at the top of the side column (it adds something top right for the eye to chase, and needs the bottom bar anyway below 1024 pixels), and a bar that also names each step (five new names, and Away from home changes its number of steps in front of the person) |
| Button order | The main button goes first, on the left, everywhere. First-run setup changes to match |
| The line in the bar | Kept: "Nothing is turned on until the last step." on Online answers steps 1 to 4 |
| Narrow windows | Below 720 pixels wide, the keep or ask bar is hidden while a step flow is open |
| Online answers step 1 | Lightened: three short true lines, one expander holding the full detail, the side column folded into it |
| Online answers step 5 | Gains a "What changes" section and a corrected lead, so everything step 1 folds away is shown in full before the person agrees |
| Away from home step 1 | Lightened the same way, now, with the full detail repeated before anything is downloaded or turned on |

## Before and after

Every "before" is captured from the real app in mock mode (`npm run dev` in `ui/`). Every "after" is
rendered from the mockups in `docs/design/mockups/step-flow-actions/`. All are in the dark theme. The
mockups use the app's own tokens and fonts, so serve the repository root over HTTP after
`make ui/install` and open them there; add `?theme=light`, `?open=1` to open an expander, or `?end=1`
to scroll to the end.

| View | Size | Before | After | Mockup |
|---|---|---|---|---|
| Online answers step 1 | 1280 by 800 | `online-step1-before.png` | `online-step1-after.png` | `online-step1.html` |
| Online answers step 1, medium window | 900 by 600 | `online-step1-narrow-before.png` | `online-step1-narrow-after.png` | `online-step1.html` |
| Online answers step 1, narrow window | 480 by 720 | `online-step1-small-before.png` | `online-step1-small-after.png` | `online-step1.html` |
| Online answers step 5, top | 1280 by 800 | `online-step5-before.png` | `online-step5-after.png` | `online-step5.html` |
| Online answers step 5, scrolled to the end | 1280 by 800 | `online-step5-end-before.png` | `online-step5-end-after.png` | `online-step5.html?end=1` |
| Away from home step 1 | 1280 by 800 | `away-step1-before.png` | `away-step1-after.png` | `away-step1.html` |
| First-run setup step 1, its foot | 1280 by 800 | `setup-foot-before.png` | `setup-foot-after.png` | `setup-step1.html` |

At 900 by 600 the app is at its medium size, where the rail and the keep or ask bar at the top stay.
The narrow size starts below 720 pixels; the 480 by 720 pair shows the keep or ask bar hidden there.

## The problem

Online answers step 1 put 256 words before its buttons: the step label, the title, a lead, six
labelled rows (What is sent, with a "What exactly is sent?" disclosure, What changes, Never sent, Who
sees it, What it costs, What you need), a closing line, and a side column with a heading, four list
lines and two notes. At 1280 by 800 Continue started 956 pixels down, below the window, with nothing
to say there was more. At 900 by 600 it started 2,508 pixels down: at that width the side column
stays beside the words and squeezes them to about 240 pixels.

### Every step flow today

Positions are the top edge of the main button, measured in mock mode. An 800 pixel window shows up
to about 760; a 600 pixel window up to about 560.

| Flow | Steps | Where the buttons sit today | Main button at 1280 by 800 | At 900 by 600 |
|---|---|---|---|---|
| First-run setup, `ui/src/screens/setup/` | Welcome, then Step 1 of 3 This computer, Step 2 Getting ready, Step 3 Ready | A centred card with no rail. The foot sits at the end of the card: the quiet button on the left, the main one on the right. Pinned only below 640 pixels wide | Step 1: 628, in view | 576: only its top edge shows |
| Online answers, `ui/src/screens/online/` | 1 Explainer, 2 Choose the service, Advanced details (Connect, not numbered), 3 Which questions, 4 Monthly limit, 5 Consent | After the words, in the left column. Main button first, then Not now or Back as a quiet link | Step 1: 956, hidden. Step 2: 394. Connect: 653, disabled, with "Continue works once the check passes." Step 3: 617. Step 4: 414. Step 5: 1,425, hidden | Step 1: 2,508. Connect: 768 |
| Away from home, `ui/src/screens/away/SetupFlow.tsx`, `AddPhone.tsx` | Choose the connection, then the download, sign in, allow links, the phone app and Add a phone. 3 steps for Simple, 6 for Most private | After the words. Main button first, Not now as a secondary button, and a muted line inside the same row | Step 1: 931, hidden | Step 1: 1,930 |
| Make a backup, `ui/src/screens/backup/MakeBackup.tsx` | The form, saving, saved. Not numbered | After the form. Save is disabled until ready, with "Choose a passphrase, type it again and tick the box to go on." | 779: the button's lower half is cut off | 1,030 |
| Restore, `ui/src/screens/restore/Restore.tsx`, also inside setup | Choose, passphrase, checking, ready, confirm, restoring, restored | After the words | Choose: 315 | 402 |

## The bar

Every step of every flow above puts its buttons in one bar, pinned to the bottom of the window.

| Rule | Detail |
|---|---|
| Place | Across the bottom of the sheet (or of setup's card), on every step, however long or short the words. A rule separates it from the words above |
| Order | The main button first, on the left, lined up with the words. Then Not now, Back or Cancel as a quiet link. Away from home's Not now becomes a quiet link to match |
| The line | After the buttons, one short line: the reason when the main button is disabled, or the step's line below. Never both |
| More to read | While there is more above the bar, the last lines fade into it. At the end of the words the fade goes |
| Reading order | The bar comes after the words in the page, so a keyboard or screen reader person meets the words first, as today. Each step's heading still takes focus on arrival |
| Focus | The scrolling area keeps room at its bottom the height of the bar (`scroll-padding-bottom`), so a focused field never hides behind it (WCAG 2.2, 2.4.11 Focus Not Obscured) |
| Large text, short windows | The bar wraps onto two lines when it must. When the window is shorter than about 28em, it stops pinning and sits after the words |
| Medium windows | Below 1024 pixels the side column drops under the words in Online answers and Away from home, as the kept-today column already does |
| Narrow windows | Below 720 pixels the keep or ask bar is hidden while a step flow is open, so the action bar is the only bar at the bottom. It comes back when the flow ends or the person leaves it |
| Reduced motion | Nothing slides in, and the fade shows without a transition |

### The line in the bar

| Step | Line |
|---|---|
| Online answers steps 1 to 4 | Nothing is turned on until the last step. |
| Online answers step 5 | None |
| Away from home step 1 | Nothing is downloaded or turned on until the next step. |
| Every other step | None, unless the main button is disabled |

### When the main button is disabled

The reason is plain text beside the button, joined to it with `aria-describedby`, so it stays in view
with the button even when the field it is about has scrolled away.

| Where | Reason |
|---|---|
| Connect (exists) | Continue works once the check passes. |
| Monthly limit, a number of questions | Continue works once the limit is a whole number, like 200. |
| Monthly limit, an amount | Continue works once the amount is above zero, like 5.00. |
| Make a backup (exists) | Choose a passphrase, type it again and tick the box to go on. |
| Online answers step 5 while turning on | None: the button reads "Turning on…" |

## Online answers step 1

### Before: today's words

| Part | Words |
|---|---|
| Step label | Online answers · step 1 of 5 |
| Title | Have your answers written by an online service you choose. |
| Lead | Nocturne still finds what you kept on this computer, and only sends what each answer needs. |
| What is sent | Your question, and the few things you kept that answer it, as short sentences. |
| Disclosure | What exactly is sent? (opens the eye test example) |
| What changes | Your questions are answered online by the service you set up. Answers may take a few seconds longer. |
| Never sent | Photos, recordings and documents stay here. Things you marked private are never sent. Nocturne also keeps back lines that look like health, money or ID details unless you choose otherwise. It can miss some, so mark anything you never want sent as private. |
| Who sees it | The service you choose can see each question and what goes with it. Every answer says it was written there. |
| What it costs | Most services charge a little for each question, on your account with them. You choose how many questions a month may go online. |
| What you need | An account with an online service. Its details are added under Advanced details, where Nocturne checks that they work. |
| Closing | While this is on, your questions leave this computer. Everything else stays here. |
| Side column heading | Stays on this computer |
| Side column list | Finding what answers your question · Keeping, reading and listening · The overnight tidy and your page · Checking where each answer came from |
| Side column notes | If the service can’t be reached, this computer answers, and says so. · Turn it off at any time, in Settings or on the Privacy page. |
| Buttons | Continue · Not now |

### A shorter version that was not used

A tester suggested replacing the step with a few friendly sentences. Its claims, and why each could
not be used:

| Its claim | Why not |
|---|---|
| Answers come from searching the web | Untrue. Nothing searches the web. A service the person chooses writes the answer from the lines Nocturne sends. |
| Only the question is sent | Untrue. The few lines from what they kept that answer it go too, with Nocturne's standard instructions. |
| Private information is never sent | Untrue. The lines that go are personal by nature. Only things marked private are never sent, and the health, money and ID check can miss some. |
| The service is a third party of your choice | True, but "third party" is not a word people use. Nocturne says "the service you choose". |
| Setting up takes some effort, because you need an account with the service | True, and the most useful part: it warns about the effort. Kept, made concrete, and moved to the top. |

The suggestion to shrink the side column to a note saying the app keeps things safe and respects
privacy was not used either: it is a promise without a fact behind it, which the experience
principles rule out ("private by proof, not by promise").

### After: the approved words

On screen, 88 words including the buttons and the bar line:

| Part | Words |
|---|---|
| Step label | Online answers · step 1 of 5 |
| Title | Have your answers written by an online service you choose. |
| Sent | Your question, with a few short lines from what you kept that help answer it. |
| Never sent | Your photos, recordings and documents themselves, and anything you marked private. |
| You need | An account with that service. Most charge a little for each question. Setting up means copying three details from its website. |
| Expander, closed | Everything that is sent, and what stays here |
| Bar | Continue · Not now · Nothing is turned on until the last step. |

Behind the expander, in this order, with today's words unchanged:

1. What is sent, followed straight away by the eye test example in full (no second disclosure inside
   the first).
2. Never sent, with the health, money and ID check and its warning that it can miss some.
3. Who sees it.
4. What changes.
5. What it costs.
6. What you need.
7. Stays on this computer: the four list lines, then "If the service can’t be reached, this
   computer answers, and says so." and "Turn it off at any time, in Settings or on the Privacy
   page."

Why each short line is true:

- **Sent.** The question goes, and so do "a few short lines from what you kept". It does not say
  "only". The standard instructions also go; they are not the person's, and the expander says so.
- **Never sent.** "Themselves" matters: the photo, recording or document never leaves, though a short
  line read from it can, which the Sent line already says. Things marked private never go, whatever
  the settings. The health, money and ID check is not claimed here, because it can miss some; it is
  explained behind the expander and at step 5.
- **You need.** Connect always asks for three details (address, key, and which one to use). "Most
  charge" stays true for a service on the person's own network that charges nothing.

The lead goes because it repeated the Sent line. The closing line goes because "Everything else stays
here" is not quite true: the lines that answer the question leave too. The side column folds into the
expander, where it sits with the facts it belongs to, and step 1 becomes one column with nothing in
the top right to pull the eye away from the three lines and the bar. Steps 2 to 4 keep their side
columns, which hold facts not repeated beside them.

## Online answers step 5

Step 5 (`Consent.tsx`) is where the person agrees, so everything step 1 folds away is shown there in
full, on screen, not behind an expander.

| Fact from step 1 | On step 5 today | After |
|---|---|---|
| What is sent, with an example | Yes | Unchanged |
| Never sent, with the health, money and ID warning | Yes | Unchanged |
| Who sees it | Yes, more fully, naming the service | Unchanged |
| Finding and checking stay here | Yes, in "Where it goes" | Unchanged |
| Keeping, reading, the tidy and your page stay here | Yes, in "Never sent": "Everything else you kept, your pages and your name" | Unchanged |
| Turn it off at any time | Yes, in "Changing your mind" | Unchanged |
| Answers may take a few seconds longer | No | Added in "What changes" |
| What it costs, on your account | No, only the limit | Added in "What changes" |
| If the service can't be reached, this computer answers | No | Added in "What changes" |

New section, between "Never sent" and "Changing your mind":

| Part | Words |
|---|---|
| Heading | What changes |
| Body | Your questions are answered by {host}, on your account with them, so it may charge for each one. Answers may take a few seconds longer. If it can’t be reached, this computer answers, and says so. |

Corrected lead:

| | Words |
|---|---|
| Before | While this is on, your questions leave this computer. Everything else stays here. |
| After | While this is on, each question and the few lines that answer it leave this computer. Everything else you kept stays here. |

The bar holds "Turn on online answers" and "Not now", with no line.

## Away from home step 1

Today, pressing Continue on step 1 either opens the download question (step 2) or, when the
connection part is already on the computer, turns Away from home on straight away. So step 1 is
sometimes the last screen before anything is turned on. After this change, step 2 always comes
first and carries the full detail; step 1 gets short lines and one expander.

### Before: today's words

The Simple card shows its note about the free service only when the app marks Simple as a trial
(`view.trial`); the Most private card shows only where Most private is available.

| Part | Words |
|---|---|
| Back link | Settings |
| Step label | Away from home · step 1 |
| Title | Use Nocturne from your phone when you’re out: ask, keep things and read today’s page. |
| Question | How should your phone reach this computer? |
| Simple, Needs | Nothing to sign up for. Nothing to install on your phone. |
| Simple, Who sees what | Your phone reaches this computer through Cloudflare, a large internet company. Nocturne locks what you ask and keep, so Cloudflare can’t read it. Cloudflare can see when your phone connects, from where, how much, and the size and timing of each message. It also serves the page your phone opens, and a change it made to that page could stay on your phone until you add the phone again. |
| Simple, note | Cloudflare offers this for free without promises, so it may sometimes stop working. |
| Most private, Needs | A Tailscale account and its app on your phone. |
| Most private, Who sees what | Usually your phone and computer talk to each other directly. If not, Tailscale passes along locked messages it can’t read. Tailscale, a network company, helps them find each other. It can see that they’re connected, and when, but not what you keep or ask. |
| After the choices | While this is on, your phone can reach this computer over the internet. Nothing goes anywhere else. |
| Buttons | Continue · Not now (a secondary button) |
| Beside the buttons | Switching between Simple and Most private means pairing your phones again. |
| Side column | Your phone can: Ask, with where each answer came from · Keep a note, a photo or a voice note · Read today’s page and the last week’s |
| Side column | Your phone can’t: Your phone can’t delete, export or change settings. Do that here. |
| Side column | Things you marked private stay on this computer. |

### After: the approved words

| Part | Words |
|---|---|
| Back link | Settings |
| Step label | Away from home · step 1 |
| Title | Use Nocturne from your phone when you’re out: ask, keep things and read today’s page. |
| Question | How should your phone reach this computer? |
| Simple, Needs | Nothing to sign up for. Nothing to install on your phone. |
| Simple, Who sees what | Cloudflare carries locked messages between your phone and this computer. It can see when your phone connects and from where, and it supplies the page your phone opens. |
| Simple, note | Cloudflare offers this for free without promises, so it may sometimes stop working. (unchanged, same condition) |
| Most private, Needs | A Tailscale account and its app on your phone. |
| Most private, Who sees what | Usually your phone and computer talk to each other directly. Tailscale can see your account, the devices on it and when they connect, but not what you keep or ask. |
| Your phone | Can’t delete, export or change settings. Do those here. |
| Private things | Stay on this computer and never reach your phone. |
| Expander, closed | Who sees what, and what your phone can do |
| Bar | Continue · Not now · Nothing is downloaded or turned on until the next step. |

Behind the expander, in this order:

| Part | Words |
|---|---|
| Simple | Your phone reaches this computer through Cloudflare, a large internet company. Nocturne locks what you ask and keep, so Cloudflare can’t read it on the way. Cloudflare can see when your phone connects, from where, how much, the size and timing of each message, and a number that tells your phones apart. It also serves the page your phone opens. A change it made to that page, or one made by someone who broke into it, could stay on your phone until you add the phone again, and could act as your phone while the page is open. After this computer restarts, your phone looks for it at its old address, which Cloudflare, or whoever gets that address later, may answer. |
| Most private | Usually your phone and computer talk to each other directly. If not, Tailscale passes along locked messages it can’t read. Tailscale, a network company, helps them find each other. It can see your account, the devices on it and when they connect, but not what you keep or ask. Only your own devices on your account can reach this computer, and each still needs your Allow to pair. |
| Either way | While it’s on, the connection part also talks to its company’s servers to stay connected. |
| Your phone can | Ask, with where each answer came from · Keep a note, a photo or a voice note · Read today’s page and the last week’s |
| Your phone can’t | Delete anything · Export what you’ve kept · Change settings or add phones |
| Notes | Do those here, on this computer. · Anyone holding your unlocked phone can do what it can. If a phone is lost, remove it here and it stops working at once. · Switching between Simple and Most private means pairing your phones again. · Leaving at any step turns Away from home back off. |

The Simple and Most private texts behind the expander are today's texts, corrected to match the guide
(`docs/guide/away-from-home.md`, "What the companies can see"). Today's Simple text says Cloudflare
"can’t read it" without saying a changed page could act as the phone, and leaves out the number that
tells phones apart and the old address after a restart. Today's Most private text leaves out that
Tailscale sees the account and the devices on it.

Why each short line is true, checked against the guide and `ui/src/copy/en-GB/away.json`:

- **Simple, Who sees what.** The guide: content is sealed; Cloudflare sees the phone's address and
  rough place and when it connects; it serves the page the phone opens. The line lists some of what
  Cloudflare sees and does not say "only", and it does not say Cloudflare cannot read anything, because
  a changed page could. The full list is behind the expander and on step 2.
- **Most private, Who sees what.** The guide: Tailscale sees "your account, the computers on it and
  when they connect", and carries sealed traffic when a direct connection fails, which "Usually"
  leaves room for.
- **Your phone.** The guide: the phone cannot forget, delete, export, change settings or add other
  phones. The line names three of those and does not claim the list is complete.
- **Private things.** The guide: the phone "never sees anything you marked private".
- **The bar line.** True once step 2 always comes before anything is downloaded or started (below).

What moves where:

| Before | After |
|---|---|
| Each connection's Who sees what | One short line on the card; the full, corrected text behind the expander and on step 2 |
| "While this is on, your phone can reach this computer over the internet. Nothing goes anywhere else." | Removed. "Nothing goes anywhere else" overclaims: the connection part talks to its company's servers, and the company sees when and from where. A corrected lead opens step 2 |
| "Switching between Simple and Most private means pairing your phones again." | Behind the expander. The switch dialog already says it at the moment it matters |
| Side column | Short lines on screen for what the phone can't do and private things; the full lists behind the expander |

### Away from home step 2: before anything is turned on

Step 2 shows for every connection, whether or not the connection part needs downloading. It carries
the full detail for the chosen connection, on screen, not behind an expander. The step number does
not change: the download question is already step 2.

| Part | Words |
|---|---|
| Step label | Away from home · step 2 of {total} · {connection} (unchanged) |
| Title, part needs downloading | Nocturne needs a small extra part from {company} (about {size}) to connect your phone. Download it now? (unchanged) |
| Title, part already here (new) | Turn on Away from home through {company}? |
| Lead (new) | While this is on, your phone can reach this computer over the internet, through {company}. |
| Body, part needs downloading | It’s the piece that lets your phone reach this computer through {company}. Nocturne checks it before it’s used, and the download shows on the Privacy screen like every other. (unchanged) |
| Facts, part needs downloading | From · Size · Runs: Only while Away from home is on (unchanged) |
| Section heading (new) | Who sees what |
| Section body (new) | The chosen connection's full text from the expander above, then "While it’s on, the connection part also talks to its company’s servers to stay connected." |
| Section heading (new) | Your phone |
| Section body (new) | Your phone can ask, keep a note, a photo or a voice note, and read today’s page and the last week’s. It can’t delete anything, export what you’ve kept, change settings or add phones. Things you marked private never reach it. Anyone holding your unlocked phone can do what it can. If a phone is lost, remove it here and it stops working at once. |
| Bar, part needs downloading | Download and turn on · Not now (was "Download") |
| Bar, part already here | Turn on · Not now |

The firewall note for Most private stays where it is today. There is no mockup of step 2; its layout
is today's download question with two sections added before the bar.

The Away from home page uses the same "Nothing goes anywhere else." lead once it is on
(`AwayPage.tsx`). It changes to the step 2 lead in the same piece of work.

## First-run setup step 1

Setup's step 1 (Step 1 of 3, This computer) gets the same bar inside its card: pinned to the bottom of
the window when the card is taller than the window, at every size, not only below 640 pixels.

| | Before | After |
|---|---|---|
| Foot | "Why this one?" on the left, "Set up · 8.4 GB" on the right | "Set up · 8.4 GB" on the left, alone in the bar |
| "Why this one?" | A quiet button in the foot; its answer opens below the foot | A disclosure in the words, after the space line; its answer opens there |
| Pinned | Only below 640 pixels wide | At every size, when the card is taller than the window |

"Why this one?" moves because its answer would otherwise open below a pinned bar. The words do not
change. The Welcome screen before step 1 puts "Get started" before "Restore from a backup" for the
same order.

## Where this meets control chips

Another design adds inline icons for named controls such as Settings. This page does not design
those. They meet here:

- Behind Online answers' expander: "Turn it off at any time, in Settings or on the Privacy page." and
  "Its details are added under Advanced details". Step 2 of Online answers: "Settings, then Advanced
  details, then Online answers." These name controls and would carry chips.
- The bar's disabled reasons name a button: "Continue works once the check passes." The bar line must
  leave room for a chip and still fit, or wrap cleanly.
- Away from home's expander and step 2 say "remove it here" and "your Allow"; if chips mark named
  buttons, Allow would carry one.
- The short lines on both first steps name no control on purpose.
- The buttons in the bar are the controls themselves, so they carry no chips.

## Not decided here

- Whether an expander stays open when the person comes back to a step they opened it on.
- Where the bar sits on very short steps: the mockups keep it at the bottom of the window for one
  fixed place, which leaves a gap below short words.
