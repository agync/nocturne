---
type: Design
title: Setting up Nocturne's mind online
description: Proposal for three fixes to the phone's online mind, with Online answers on the computer for parity, the step's buttons pinned in view on every phone step flow, the model name chosen from the service's own list with typing kept, and honest reading on battery with Read now, every state, copy and accessibility.
status: draft
tags: [proposal, ux, phone, online-answers, flows, layout, battery, copy, accessibility]
---

# Setting up Nocturne's mind online

Part of [Nocturne on a phone of its own](phone-first.md#an-online-service-you-choose). Three problems
with "Nocturne's mind online" on the phone, the second shared with Online answers on the computer:

1. **The buttons are out of reach.** Step 4 of 4 is long, and its "Turn on" and "Not now" sit at
   the end of the scroll. On a real phone the person sees the words and the back arrow, and goes
   back.
2. **The model name is typed by hand.** Step 2 asks for "Model name" as free text, though its check
   already takes one look at the service's list of models. People copy a name with a wrong
   capital, a missing date or a stray space, and learn it only when the check fails.
3. **Reading looks stuck on battery.** The phone reads only while it charges, but This phone says
   "Reading · 0 of 89", and there is no way to read now.

Status: built, with the owner's decisions of 2026-11-24 folded in; see [As built](#as-built). It
extends the approved [Keeping the next step in view](step-flow-actions.md) to the phone, and changes
the details step of both setups.

## The owner's decisions

Settled on 2026-11-24: 1 to 4 as recommended, 5 and 6 as the owner chose for Read now.

| # | Decision | Where it lands |
|---|---|---|
| 1 | A typed name that isn't on the service's list is warned about, and the check's one question decides: if an answer comes back, it works | [Behaviour](#behaviour-state-by-state), [The check](#the-check-and-a-name-not-on-the-list) |
| 2 | The list is fetched only when the person taps "Choose from {host}'s list…", never by itself | [The design](#the-design) |
| 3 | The list is sorted A to Z, ignoring capitals, numbers in number order | [The list](#the-list) |
| 4 | Online answers' Connect on the computer changes in the same release | [On the computer](#on-the-computer-same-form-same-change) |
| 5 | Read now carries on after the person leaves Nocturne, as far as each phone allows: a foreground service with its notification on Android, iPhone's own progress or a short while then a pause on iPhone. Nothing keeps the screen on | [After the person leaves Nocturne](#after-the-person-leaves-nocturne) |
| 6 | Read now stops at 20 per cent and when the phone's battery saving is on, and then offers "Read anyway", which runs until done or Stop, never below 5 per cent | [Battery](#battery) |

## Mockups

In `docs/design/mockups/mind-online-setup/`. `phone.html` uses the phone parity kit, Android and
iPhone side by side; `?state=`, `?theme=dark`, `?text=200` (the phone's text at 200 per cent, at
the phone's real height) and `?end=1` (scrolled to the end, past any line the top edge would cut)
work on every state. `desktop.html` uses the app's own tokens like the step flow mockups: serve the
repository root over HTTP after `make ui/install`. `reading.html` shows This phone and "Reading what you kept" on battery, with the
same parameters as `phone.html`; at 200 per cent each screen starts at the mind group.

| Screen | File and state | Renders |
|---|---|---|
| Step 1, "Everything that is sent" open | `phone.html?state=what` | `what-light.png` |
| Step 4, agree | `phone.html?state=agree` | `agree-light.png`, `agree-dark.png`, `agree-200-light.png` |
| Step 2, URL and key filled | `phone.html?state=details&end=1` | `details-light.png` |
| Step 2, looking at the list | `phone.html?state=loading&end=1` | `loading-light.png` |
| The list, short | `phone.html?state=list` | `list-light.png`, `list-dark.png` |
| The list, long, searched | `phone.html?state=search` | `search-light.png` |
| The list, nothing matches | `phone.html?state=nomatch` | `nomatch-light.png` |
| Step 2, a name chosen | `phone.html?state=picked&end=1` | `picked-light.png`, `picked-dark.png`, `picked-200-light.png` |
| Step 2, the list failed: key refused | `phone.html?state=failed&end=1` | `failed-light.png`, `failed-dark.png` |
| Step 2, no list at that address | `phone.html?state=nolist&end=1` | `nolist-light.png` |
| Step 2, a typed name not on the list | `phone.html?state=notlisted&end=1` | `notlisted-light.png` |
| Online answers, Connect, a name chosen | `desktop.html?state=picked` | `desktop-picked-light.png`, `desktop-picked-dark.png` |
| Online answers, Connect, the list | `desktop.html?state=list` | `desktop-list-light.png`, `desktop-list-dark.png` |
| This phone, waiting to charge | `reading.html?state=waiting` | `reading-waiting-light.png`, `reading-waiting-dark.png`, `reading-waiting-200-light.png` |
| This phone, reading now | `reading.html?state=now` | `reading-now-light.png`, `reading-now-dark.png`, `reading-now-200-light.png` |
| Away: Android's notification, iPhone's own progress | `reading.html?state=away` | `reading-away-light.png`, `reading-away-dark.png`, `reading-away-200-light.png` |
| Away: iPhone paused it | `reading.html?state=paused` | `reading-paused-light.png`, `reading-paused-dark.png`, `reading-paused-200-light.png` |
| This phone, back after iPhone paused it | `reading.html?state=back` | `reading-back-light.png` |
| This phone, stopped at 20 per cent, Read anyway | `reading.html?state=low` | `reading-low-light.png`, `reading-low-dark.png`, `reading-low-200-light.png` |
| This phone, reading anyway | `reading.html?state=anyway` | `reading-anyway-light.png` |
| This phone, all read | `reading.html?state=done` | `reading-done-light.png` |
| This phone, paused by the limit | `reading.html?state=limit` | `reading-limit-light.png` |
| This phone, the service not answering | `reading.html?state=unreachable` | `reading-unreachable-light.png` |
| Reading what you kept, on battery | `reading.html?state=first-waiting` | `reading-first-waiting-light.png`, `reading-first-waiting-dark.png` |
| Reading what you kept, reading now | `reading.html?state=first-now` | `reading-first-now-light.png` |

### Before and after

Every "before" is captured from the phone app's own build with its scripted engine, at 412 by 915,
light. Every "after" is rendered from the mockups.

| View | Before | After |
|---|---|---|
| Step 4, top of the screen | `agree-before-light.png` | `agree-light.png` |
| Step 4, text at 200 per cent | `agree-200-before-light.png` | `agree-200-light.png` |
| Step 2, the model name | `details-before-light.png` (typed) | `picked-light.png` (chosen) |
| Step 2, a refused key | `failed-before-light.png` (found at Check) | `failed-light.png` (found at the list) |
| This phone, on battery with 89 waiting | `this-phone-before-light.png` | `reading-waiting-light.png` |
| This phone, dark | `this-phone-before-dark.png` | `reading-waiting-dark.png` |
| This phone, text at 200 per cent | `this-phone-before-200-light.png` | `reading-waiting-200-light.png` |
| Reading what you kept, on battery | `reading-before-light.png` | `reading-first-waiting-light.png` |

The scripted engine reports 12 of 46 read; the "before" captures of This phone and "Reading what you
kept" show the reported "0 of 89" in its place, with nothing else changed.

## Problem 1: buttons out of reach

### Today on the phone

The phone's bar (`ph-foot`) is the last thing in the page. It sits at the bottom only when the
words are short; otherwise it scrolls away with them. Positions are the top edge of the main button
at 412 by 915, measured in the scripted build. The screen shows up to about 915.

| Screen | Main button at 100 per cent | At 200 per cent |
|---|---|---|
| Where should Nocturne's mind be? | 789, in view | 1,392, hidden |
| Step 1 | 760, in view | 1,568, hidden |
| Step 1, "Everything that is sent" open | 1,439, hidden | 4,164, hidden |
| Step 2 | 760, in view | 1,071, hidden |
| Step 2, the key refused | 760, in view | 1,331, hidden |
| Step 3, an amount | 760, in view | 816: "Back" and the reason hidden |
| Step 4 | 1,398, hidden | 4,068, hidden |
| Save a backup (to a file) | 818, in view | 1,505, hidden |

### The rule on the phone

The computer's bar rule, carried over to the phone's sizes. Nothing changes in the words, the
buttons or their order.

| Rule | Detail |
|---|---|
| Place | The bar is pinned to the bottom of the screen, above the gesture bar or home indicator, on every step listed below, however long the words |
| Order | Unchanged: the main button first and full width, then the quiet one, then the line. The phone stacks them, as today |
| More to read | While words continue below the bar, the last 40 pixels fade into it. At the end of the words the fade goes. A rule line separates the bar from the words, as today |
| Reading order | The bar stays after the words in the page, so a screen reader meets the words first. Each step's heading still takes focus on arrival |
| Focus | The page keeps room at its bottom the height of the bar, so a focused field or button never hides behind it (WCAG 2.2, 2.4.11 Focus Not Obscured) |
| Large text | The bar stays pinned while it takes no more than a third of the screen's height. At 200 per cent on a 412 by 915 phone, step 4's bar takes about a fifth and step 2's, with its two-line reason, under a third, so both stay pinned |
| Short screens | When the bar would take more than a third, such as a phone on its side or text above 200 per cent, it sits after the words, as today |
| Keyboard | While the on-screen keyboard is up, the bar sits after the words, so the field and the keyboard have the room. It pins again when the keyboard goes |
| Reduced motion | The fade shows and goes without a transition |
| Back arrow | Unchanged. "Back" or "Not now" in the bar does what it does today |

Step 4 keeps "Turn on" usable from the start, as the computer's step 5 does. Asking the person to
scroll to the end first was not used: the fade already says there is more, and a button that waits
for scrolling is a hurdle that teaches people to scroll without reading.

### Which phone screens

The rule belongs to the bar itself, so every screen that ends in it gets it. Checked against
`ui/src/phone`:

| Flow | Screens | Why it needs it |
|---|---|---|
| Nocturne's mind online, `alone/screens/Mind.tsx` | Where should Nocturne's mind be?, steps 1 to 4 | Measured above: step 4 always, the rest at 200 per cent or with the expander open |
| Nightly backup, designed in [Backup on a phone of its own](phone-backup.md) | Steps 1 to 3, a recovery key, Make a recovery key | Step 1's expander and step 2's phrase and note run long. Its mockups already draw the bar pinned; this makes it the rule |
| Backup to a file and restore, `alone/screens/Backup.tsx` | Save a backup, Bring back your memory, Replace | Save a backup is hidden at 200 per cent |
| Moving your memory, `alone/screens/MoveScreens.tsx` | Found, size, failed, moved, used | Long explanations of what moves and what stays |
| First run, `alone/screens/Welcome.tsx` | Your memory lives on this phone, the mind offer | Long at 200 per cent |
| Permissions, `alone/screens/Permissions.tsx` | Calendar and Health permissions | Long at 200 per cent |
| Confirmations, `alone/screens/DeleteEverything.tsx`, `DeleteKept.tsx`, `Forget.tsx`, `PutAside.tsx` | Delete everything, delete a kept thing, forget, put aside | Their safeguards stay: Delete everything still waits for the typed word, and the safe choice keeps its place |
| The companion's journal, `screens/CheckIn.tsx`, `Journal.tsx`, `LookBack.tsx`, `Which.tsx` | Save a check-in, choose which one, back to today | A check-in's Save sits under a long list of feelings |

The morning page and an original (`PageScreen.tsx`, `Original.tsx`) already hold a fixed layout
with their bar in place, so nothing changes there. Screens whose one button sits under a short
message, such as "Reading what you kept" and "Backed up", get the rule too and look the same.

## Problem 2: the model name

### Today

Step 2 holds three free text fields, "API URL", "API key" and "Model name", then "Check". The check
(`crates/provider/src/client.rs`, `check()`) does two things in order:

1. One look at the service's list of models (`models()`). If the list has names and the typed one
   is not among them, the check stops: "{host} doesn't know a name like “{used}”." A service
   with no list, answering "not found", is let through.
2. One short question, with nothing the person kept. If an answer comes back, it works.

So Nocturne already fetches the list, and throws it away. The person never sees it.

The list a standard service gives holds little more than names, such as `reader-small` or
`reader-large-2026-09`. It says nothing about what each one is for, its price or its size, and
Nocturne does not guess any of that from a name.

### The design

The name field stays a text field the person can always type in. Under it, one more control:
"Choose from {host}'s list…". It takes the same one look the check already takes, at the moment the
person asks for it, and shows the names to choose from.

```mermaid
flowchart TD
    F[URL and key filled] --> T{Choose from the list?}
    T -->|types instead| C[Check]
    T -->|taps| L[One look at the list]
    L -->|names| S[The list opens: search, choose]
    L -->|no names, or no list| N[Type the name; the line says why]
    L -->|refused, unreachable, offline| E[The problem, with Try the list again]
    S -->|chooses| P[Name in the field: On host's list.]
    S -->|closes| C
    N --> C
    P --> C
    C -->|answers| W[It works. Continue]
```

- **One look, as allowed.** The look is the check's own: same address, same key, and recorded
  wherever the check's look is recorded today (on the phone, a "Connection check" row in Sent
  online). When "Check" follows with the same address and key, it
  uses the list from that look instead of looking again, so a check still takes one look at the
  list, which is what invariant 2 allows. Nothing new leaves the phone or the computer, and nothing
  goes anywhere else.
- **Only when asked.** The list is fetched when the person taps "Choose from {host}'s list…", never
  by itself while they type (decision 2).
- **The address and key decide.** Changing either drops the list, as it already drops a passed
  check. The name in the field stays.
- **Choosing is typing.** Choosing a name puts it in the field exactly as listed, as if typed. The
  check runs as today; choosing never passes the check by itself.
- **The list is not kept.** Leaving the step forgets it. Coming back and tapping again looks again.

### Behaviour, state by state

| State | When | The name field | Under it | The list control | Check |
|---|---|---|---|---|---|
| Not ready | The URL or key is empty | Typable | Nothing | "Choose from the service's list…", not pressable, with "Fill in the API URL and key first." | Not pressable until all three are filled, as today |
| Ready | URL and key filled | Typable | Nothing | "Choose from {host}'s list…" | As today |
| Looking | The person tapped it | Typable | Nothing | "Looking at {host}'s list…" with the breathing dot, not pressable | Not pressable while it looks |
| Listed | The look found names | Typable | Nothing | The list opens over the step (below) | As today |
| Chosen | A name was chosen in the list | The name, as listed | "On {host}'s list." | "Choose from {host}'s list…" again, to change it | Pressable; it uses the same list |
| Empty list | The look worked but listed no names | Typable, focused | "{host} didn't give a list at that address." and how to go on | "Try the list again" | Pressable once typed |
| No list | The address answered "not found", or with something that is not a list | As above | As above | As above | As above. The check then asks its question, as today |
| List failed, typing can help | The service was busy or had a problem on its side | Typable | The problem, then "Type the name exactly as its website shows it, or try the list again in a moment." | "Try the list again" | As today |
| List failed, typing can't help | The key was refused, no credit, nothing answered at the address, this phone or computer is offline, the address is not allowed | Typable | The problem, worded as the check words it, pointing at the URL or the key | "Try the list again" | As today, and it will fail the same way until the URL or key is fixed |
| Typed name not on the list | A list is known and the typed name is not in it | The typed name | "“{name}” isn't on {host}'s list. Check the spelling, or choose from the list. Capitals and dashes matter." | "Choose from {host}'s list…" | Pressable. The check asks its question; if an answer comes back, it works (decision 1) |
| Only the capitals differ | As above, and one listed name matches but for capitals | The typed name | "“{name}” isn't on {host}'s list, but “{listed}” is." with "Use “{listed}”" | As above | As above |
| Long list | More than 8 names | | | The list opens with a search field, focused | |
| Checked, then changed | The check passed, then the person chose or typed another name | The new name | As for its state | As for its state | "Check" again; Continue waits, as for any edit today |

The two failures are told apart because they need different help. When typing a name could still
work, Nocturne says so. When the key or the address is the problem, a typed name will fail at the
check too, so the words point at the key or the address instead of inviting a name.

"Not found" at the list's address is not proof that the address is wrong: some services have no
list. The words say both, and the check that follows settles it.

### The list

| Part | On the phone | On the computer |
|---|---|---|
| Shape | A sheet from the bottom, up to 86 per cent of the screen, full height while the keyboard is up | A dialog over Connect, 520 pixels wide |
| Title | "{host}'s list" | Same |
| Under the title | "{count} names, A to Z" | Same |
| Search | Shown when there are more than 8 names, focused on opening: "Search the list" | Same |
| Rows | One per name, exactly as listed, in the monospaced type so `l`, `1`, `O` and `0` differ. A radio button shows the name in the field, if it is listed | Same, 40 pixels tall |
| Order | A to Z, ignoring capitals, numbers in number order (decision 3) | Same |
| Choosing | One tap chooses and closes the list | One click, or Enter on the focused row. Arrow keys move without closing |
| Searching | Matches any part of the name, ignoring capitals. The matching part is marked. "{shown} of {count} have “{query}” in them." | Same |
| Nothing matches | "No name on the list has “{query}” in it." with "Use “{query}” as typed", which puts it in the field and closes the list | Same |
| Under the rows | "Not here? Close the list and type the name from the service's website." | Same |
| Closing | The close button, the system back gesture, or tapping outside. Nothing changes | "Close", Escape, or clicking outside |

No grouping, no labels, no "recommended" and no "best for": a standard service's list holds names
only, and anything Nocturne added would be a guess. Nocturne still recommends no service and no
name.

### The check and a name not on the list

Some services list names wrongly: they leave out a name they answer to, such as an older name kept
for a while, or one that only some accounts can use. Today's check stops those at the list. With
decision 1 it no longer does: a name not on a non-empty list is warned about under the field, and
the check still asks its one question. If an answer comes back, it works ("{host} answered. It
works."), and the name is used as typed. If not, the check's own outcome shows, as today. In
`check()`, a name missing from the list stops being a reason to return before the question.

### On the computer: same form, same change

Online answers' Connect (`ui/src/screens/online/Connect.tsx`) is the same form: the same three
details, the same check through the same client, and the same outcomes. Its labels differ ("Address",
"Key", "Which one to use", each with its technical word beside it), and it sits under Advanced
details. It gets the same list, in a dialog, with the computer's words for being offline. The
`desktop-*` renders show it. The phone and the computer would otherwise teach two ways to fill in one
form.

## Copy

New strings only. Everything else on both steps is unchanged. `{host}` is the address's host, as
the check already names it.

### Phone, step 2

| Key | Copy |
|---|---|
| `phone.mind.details.model.placeholder` | "Type it, or choose from the list" |
| `phone.mind.details.list` | "Choose from {host}’s list…" |
| `phone.mind.details.list.noHost` | "Choose from the service’s list…" |
| `phone.mind.details.list.needs` | "Fill in the API URL and key first." |
| `phone.mind.details.list.looking` | "Looking at {host}’s list…" |
| `phone.mind.details.list.again` | "Try the list again" |
| `phone.mind.details.list.on` | "On {host}’s list." |
| `phone.mind.details.list.notOn` | "“{name}” isn’t on {host}’s list. Check the spelling, or choose from the list. Capitals and dashes matter." |
| `phone.mind.details.list.capitals` | "“{name}” isn’t on {host}’s list, but “{listed}” is." |
| `phone.mind.details.list.useListed` | "Use “{listed}”" |
| `phone.mind.details.list.none` | "{host} didn’t give a list at that address." |
| `phone.mind.details.list.none.detail` | "Some services don’t. Type the name exactly as its website shows it, then check. If the check can’t reach it either, look at the API URL again." |
| `phone.mind.details.list.later.detail` | "Type the name exactly as its website shows it, or try the list again in a moment." |
| `phone.mind.details.list.keyRefused.detail` | "Check you copied all of it, then try the list again. Some services also need credit on the account before a key works." |
| `phone.mind.details.list.noInternet.detail` | "Connect to the internet, then try the list again. Nothing was sent." |
| `phone.mind.details.list.failed` | "The list couldn’t be fetched. Type the name, or try the list again." |

The titles of the failures reuse the check's own: `phone.mind.check.keyRefused`, `noCredit`,
`unreachable`, `noInternet`, `notHttps`, `addressNotAllowed`, `busy` and `serverError`, with their
details where the list adds nothing (`noCredit`, `unreachable`, `notHttps`, `addressNotAllowed`).
`busy` and `serverError` take `list.later.detail`; an empty or missing list takes `list.none`.

### Phone, the list

| Key | Copy |
|---|---|
| `phone.mind.list.title` | "{host}’s list" |
| `phone.mind.list.count` | "{count, plural, one {# name} other {# names}}, A to Z" |
| `phone.mind.list.search` | "Search the list" |
| `phone.mind.list.matches` | "{shown} of {count} have “{query}” in them." |
| `phone.mind.list.noMatch` | "No name on the list has “{query}” in it." |
| `phone.mind.list.useTyped` | "Use “{query}” as typed" |
| `phone.mind.list.note` | "Not here? Close the list and type the name from the service’s website." |
| `phone.mind.list.close` | "Close" |

### Computer, Connect

The same strings under `advanced.online.connect.list.*`, with these differences:

| Key | Copy |
|---|---|
| `advanced.online.connect.name.placeholder` (changed) | "Type it, or choose from the list" (was "The exact name from its website") |
| `advanced.online.connect.list.needs` | "Fill in the address and key first." |
| `advanced.online.connect.list.none.detail` | "Some services don’t. Type the name exactly as its website shows it, then check. If the check can’t reach it either, look at the address again." |
| `advanced.online.connect.list.noInternet.detail` | "Connect, then try the list again. Nothing was sent." |

The failure titles reuse `advanced.online.connect.*`, whose `noInternet` already says "This computer
isn’t connected to the internet."

## Accessibility

- **The control.** "Choose from {host}'s list…" is a button. The ellipsis says it opens something,
  as elsewhere. While it looks it is not pressable, and "Looking at {host}'s list…" is a polite
  status.
- **The list.** A modal sheet or dialog named by its title. Focus moves to the search field when
  there is one, otherwise to the chosen row or the first. Rows are a group of radio buttons named
  by the title, so a screen reader says "reader-small, radio button, 3 of 6". Arrow keys move,
  Space or Enter chooses, Escape closes. Focus returns to the name field when it closes.
- **The match count** is a polite live region, read once typing pauses, never on every key.
- **The field.** "On {host}'s list." and the not-on-the-list line are joined to the name field with
  `aria-describedby`, so they are read with it. Failures are alerts, as the check's are.
- **200 per cent text.** Rows grow and names wrap anywhere, since names have no spaces. The sheet
  scrolls; its title and search stay at its top. The step's bar stays pinned as above.
- **Targets.** Rows are at least 48 pixels on the phone, 40 on the computer; the close button is 44.
- **Colour.** The matching part is marked by a background and the chosen row by its radio, never by
  colour alone.
- **Reduced motion.** The sheet appears without sliding.
- **Light and dark.** Both, as the renders show.

## Problem 3: reading on battery

### Today

The phone reads what was kept only while it charges. Every pass of the mind stops at once when
the phone is not known to charge (`pass()` in `phone/src-tauri/src/mind/mod.rs` returns
`Pass::NotCharging`), even with Nocturne open in the person's hand. A phone that cannot say whether
it charges is taken as on battery. Questions asked in Ask are answered at once, charging or not.

Nothing on screen says so. This phone shows "Its mind · answers.example, online · Reading · 0 of 89"
and "Reading what you kept" shows a breathing dot over "0 of 89 read", so reading looks stuck. The
only line about charging, "Reading carries on when this phone allows it, often while it charges.",
is on the screen a person sees once, right after turning the mind on.

Reading while charging is right: reading many things uses the network and the processor, and a
person should not find their battery run down by something they didn't start. What is missing is
saying so, and a way for the person to choose otherwise.

### The design

- **Say what is happening.** On battery with things waiting, the status says "Waiting to charge ·
  89 to read", not "Reading · 0 of 89".
- **Say why, where it shows.** "Nocturne reads what you keep while this phone charges, so it
  doesn't run the battery down."
- **Offer Read now.** It reads straight away, uses some battery, and carries on after the person
  leaves Nocturne as far as the phone allows (below). It is the person's act: Nocturne offers it on
  This phone and on "Reading what you kept", and never starts it, suggests it in a notification or
  puts it on Today.
- **Protect the battery, and let the person overrule it.** Read now stops at 20 per cent or when
  the phone's own battery saving turns on, and offers "Read anyway", which runs until everything is
  read or the person stops it, never below 5 per cent.
- **Stop plainly.** Every stop says why, in Nocturne, and in a quiet notification if the person
  is elsewhere.

### Where the reading state shows

| Place | Today | After |
|---|---|---|
| This phone, "Its mind" row | "{host}, online · Reading · {done} of {count}" whenever things wait | The row's words by state (below), and the block under the row |
| "Reading what you kept", once after turning on | "{done} of {count} read" with a breathing dot, and the charging note | By state: waiting to charge, or reading now, with Read now or Stop |
| Android's notifications | Nothing | While Read now runs, its progress and Stop; a quiet line when it ends while the person is elsewhere |
| iPhone's Lock Screen and notifications | Nothing | From iOS 26, iPhone's own progress with its Stop; on any iPhone, a quiet line when iPhone pauses it |
| Today | Nothing while reading (phone-first's states) | Unchanged. Nothing about charging, so Today never nags |
| Ask | Finds by words while things wait to be read | Unchanged |

### After the person leaves Nocturne

Read now is the person's own act, so it carries on in the background as far as each phone allows.

**Android.** Read now runs as a foreground service of the `dataSync` type, which Android describes
for fetching data and moving it between the phone and a server, the closest fit for sending what
was kept and keeping what comes back.

| Part | Detail |
|---|---|
| The service | Started only from the person's tap, while Nocturne is in front, as Android requires. It ends itself when everything is read, at a battery stop, on a pause or trouble, or on Stop |
| The notification | Ongoing and silent, in its own channel "Reading now": "Reading what you kept", "{done} of {count}", a progress bar and "Stop". Tapping it opens This phone |
| The manifest | `FOREGROUND_SERVICE` and `FOREGROUND_SERVICE_DATA_SYNC`, and the service declared with `android:foregroundServiceType="dataSync"`. The type is declared in Play Console too |
| Notifications allowed | On Android 13 and later, the first Read now asks with Android's own question if Nocturne may not yet show notifications. Either answer starts reading. With no, progress shows in Nocturne and in Android's list of running apps, and This phone says so |
| Its limit | From Android 15, `dataSync` services may run about 6 hours a day in all. Reading what is waiting takes minutes, so this never binds; if it did, the service would stop as on Stop and say so |

The app targets SDK 37 (`compileSdk` and `targetSdk` 37, `minSdk` 26). One path serves every
version from 26: Android 14 made the type and its permission required, and the type is still
offered for this use at 37. Android also offers user-initiated data transfer jobs for transfers a
person starts (Android 14 and later); they would need a second path for older phones and give the
person nothing more, so they are not used. The build checks the foreground service rules of the
SDK it targets before release.

**iPhone.** iPhone decides how long an app may work once the person leaves it. The app supports
iOS 17 and later.

| iPhone | What happens after leaving Nocturne | What the person sees |
|---|---|---|
| iOS 26 and later | Read now runs as a continued processing task (`BGContinuedProcessingTask`), started by the person's tap. iPhone shows its progress itself, on the Lock Screen and at the top, with its own Stop. iPhone may end it early, more often while the phone is locked | "Reading what you kept · {done} of {count}" in iPhone's own progress. If iPhone ends it, the paused notification below |
| iOS 17 to 25 | Reading carries on for the short while iPhone gives an app as it leaves, often under a minute, then pauses | The paused notification below |
| Either | When reading pauses, one quiet notification if Nocturne may show them: "Reading paused" / "iPhone paused it when you left Nocturne, after {done} of {count}. Open Nocturne to carry on." It carries on by itself when the person opens Nocturne, since it is still their Read now, and on the charger in the background window iPhone already gives | Back in Nocturne: "iPhone paused reading while you were away, after {done}. It carries on now." |

Nothing keeps the screen on: Android does not need it, and on iPhone it would help only while the
person is looking at Nocturne, where reading carries on anyway.

**The notifications' words.** They are the status of the person's own act, not reminders, so the
rules about private things and reminders do not apply to them. They never name a kept thing, a
title, a question or a host: only counts and what happened.

### Battery

| Battery | Read now | Read anyway |
|---|---|---|
| On the charger | Not needed: reading runs by itself | Not needed |
| Above 20 per cent, no battery saving | Starts; runs until everything is read, Stop, a pause or trouble | Not shown |
| Reaches 20 per cent, or battery saving turns on | Stops, and says why | Offered: "Read anyway" |
| 20 per cent or below, or battery saving on, before starting | Not offered | Offered in its place |
| Above 5 per cent with Read anyway | | Runs until everything is read, Stop, a pause or trouble, battery saving or not |
| 5 per cent | Stops | Stops, and is not offered again until the battery is above 5 per cent or the phone charges |

**Why 20 per cent.** iPhone first offers Low Power Mode at 20 per cent; it is where a phone starts
telling its person to save what is left. Battery Saver on Android and Low Power Mode on iPhone are
the person's own wish to save battery, so Read now keeps to them unless told otherwise.

**Why 5 per cent.** At 5 per cent a phone is minutes from switching itself off. What is left
belongs to a call, a message or a map, not to reading that can wait for the charger. "Read anyway"
lets the person spend their battery on reading, but not their last few minutes.

### States

`{left}` is what still waits, `{count}` minus `{done}`. "The block" is the part under "Its mind".
"Away" is what Android's notification or iPhone's own progress shows while the person is in
another app.

| State | When | "Its mind" row | The block | Away |
|---|---|---|---|---|
| Nothing waiting | All read | "{host}, online" | Nothing | Nothing |
| Reading, charging | On the charger, things waiting | "{host}, online · Reading · {done} of {count}" (unchanged) | Nothing: there is nothing to do | Nothing |
| Waiting to charge | On battery, or the phone can't say, things waiting | "{host}, online · Waiting to charge · {left} to read" | The why line, the platform's cost line, "Read now" | Nothing |
| Reading now | The person pressed Read now | "{host}, online · Reading now · {done} of {count}" | A progress bar, the platform's line about leaving, "Stop" | Android: the ongoing notification. iPhone on iOS 26 and later: iPhone's own progress |
| Paused while away | iPhone only: iPhone paused it after the person left | Unchanged until they return | | "Reading paused" notification |
| Back after a pause | iPhone only: the person opened Nocturne again | "{host}, online · Reading now · {done} of {count}" | "iPhone paused reading while you were away, after {done}. It carries on now.", then as reading now | Nothing |
| All read | Nothing left | "{host}, online" | "Everything you kept is read.", until the person leaves This phone | Android: the ongoing notification becomes "Everything you kept is read." iPhone: its progress ends |
| Stopped at 20 per cent | The battery reached 20 per cent | "{host}, online · Waiting to charge · {left} to read" | "Read now stopped at 20% battery, after {done}. The rest is read while this phone charges." then the Read anyway line and "Read anyway" | "Read now stopped at 20% battery" / "{done} read. Open Nocturne to read anyway." |
| Low before starting | 20 per cent or below, not charging | "{host}, online · Waiting to charge · {left} to read" | "The battery is at {percent}%." then the why line, the Read anyway line and "Read anyway" | Nothing |
| Stopped for battery saving | Battery Saver or Low Power Mode turned on | As waiting | "Read now stopped because Battery Saver is on, after {done}." (iPhone: "Low Power Mode") then the Read anyway line for battery saving and "Read anyway" | "Read now stopped: Battery Saver is on" / "{done} read. Open Nocturne to read anyway." |
| Battery saving on before starting | Not charging | As waiting | "Battery Saver is on." (iPhone: "Low Power Mode is on.") then the why line, the Read anyway line for battery saving and "Read anyway" | Nothing |
| Reading anyway | The person pressed Read anyway | "{host}, online · Reading now · {done} of {count}" | "Reading anyway, down to 5% battery.", a progress bar, the platform's line about leaving, "Stop" | As reading now |
| Stopped at 5 per cent | The battery reached 5 per cent | As waiting | "Reading stopped at 5% battery, after {done}. The rest is read while this phone charges." No button; "Read now works above 5% battery, or while charging." | "Reading stopped at 5% battery" / "{done} read. The rest is read while this phone charges." |
| Charger plugged in | During Read now | "{host}, online · Reading · {done} of {count}" | "Charging, so reading carries on by itself.", then nothing | The notification or iPhone's progress ends; reading goes on as on the charger |
| Stopped by the person | "Stop" in Nocturne, the notification or iPhone's progress | As waiting | As waiting, with no stop line | Ends |
| Paused by the limit | The month's limit is reached, before or during | "{host}, online · Paused by your limit · {left} to read" | "You've reached this month's limit. The rest is read from {day}, or when you change the limit." with "Change limit". No Read now: the limit holds | "Read now stopped" / "{done} read. Open Nocturne to see why." |
| Not answering | The service stopped answering, before or during | "{host}, online · Not answering · {left} to read" | "{host} hasn't answered since {time}. Nocturne looks again on its own." and "Read now looks once more, then reads straight away, and uses some battery." with "Read now" | During: "Read now stopped" / "{done} read. Open Nocturne to see why." |
| Read now, still not answering | The person pressed Read now and the one look found nothing | As above | "{host} still isn't answering. Nocturne looks again on its own." with "Read now" | Nothing |
| Not online | This phone can't reach the internet | "{host}, online · Not online · {left} to read" | "This phone isn't online. What you keep waits to be read." "Read now" not pressable, "Read now works once this phone is online." | During: as not answering |
| Key refused, no credit | Before or during | "{host}, online · Stopped · {left} to read" | The problem's own words and action, as today ("Check the key…", "Check again"). No Read now | During: as not answering |
| Notifications off | Android: the person said no to notifications | As reading now | As reading now, with "Notifications are off for Nocturne, so its progress shows here." | Nothing in the notification shade; Android's list of running apps still shows Nocturne |

"Reading what you kept" shows the same states in its own words: waiting to charge with "OK" and
"Read now", reading now with "OK" and "Stop" ("OK" leaves the screen and reading carries on), and
each stop line with "OK".

A notification left after a stop goes away when the person opens Nocturne. Read now under "Not
answering" takes one look at the service's list, the same look the phone already makes on its own
schedule, listed in Sent online as a "Connection check". If it answers, reading now begins; if not,
the line says so and nothing else is sent.

Read now reads what was kept. Writing the morning page still waits for the charger, as today.

### Copy

| Key | Copy |
|---|---|
| `phone.mind.row.waiting` | "Waiting to charge · {left} to read" |
| `phone.mind.row.readingNow` | "Reading now · {done} of {count}" |
| `phone.mind.row.paused` | "Paused by your limit · {left} to read" |
| `phone.mind.row.notAnswering` | "Not answering · {left} to read" |
| `phone.mind.row.notOnline` | "Not online · {left} to read" |
| `phone.mind.row.stopped` | "Stopped · {left} to read" |
| `phone.mind.row.reading` (unchanged) | "Reading · {done} of {count}" |
| `phone.mind.readNow` | "Read now" |
| `phone.mind.readNow.anyway` | "Read anyway" |
| `phone.mind.readNow.stop` | "Stop" |
| `phone.mind.readNow.why` | "Nocturne reads what you keep while this phone charges, so it doesn’t run the battery down." |
| `phone.mind.readNow.cost` (Android) | "Read now reads straight away and uses some battery. It carries on if you leave Nocturne, with a notification to stop it." |
| `phone.mind.readNow.cost` (iPhone) | "Read now reads straight away and uses some battery. iPhone may pause it when you leave Nocturne; it carries on when you come back." |
| `phone.mind.readNow.leaving` (Android) | "You can leave Nocturne. Reading carries on, and the notification shows how far it has got." |
| `phone.mind.readNow.leaving` (iPhone) | "If you leave Nocturne, iPhone may pause reading. It carries on when you come back." |
| `phone.mind.readNow.back` (iPhone) | "iPhone paused reading while you were away, after {done}. It carries on now." |
| `phone.mind.readNow.noNotes` (Android) | "Notifications are off for Nocturne, so its progress shows here." |
| `phone.mind.readNow.done` | "Everything you kept is read." |
| `phone.mind.readNow.low` | "Read now stopped at 20% battery, after {done}. The rest is read while this phone charges." |
| `phone.mind.readNow.lowNow` | "The battery is at {percent}%." |
| `phone.mind.readNow.saver` (Android) | "Read now stopped because Battery Saver is on, after {done}." |
| `phone.mind.readNow.saver` (iPhone) | "Read now stopped because Low Power Mode is on, after {done}." |
| `phone.mind.readNow.saverNow` (Android) | "Battery Saver is on." |
| `phone.mind.readNow.saverNow` (iPhone) | "Low Power Mode is on." |
| `phone.mind.readNow.anywayWhat` | "Read anyway reads until everything is read or you stop it, down to 5% battery." |
| `phone.mind.readNow.anywayWhatSaver` (Android) | "Read anyway reads until everything is read or you stop it, even with Battery Saver on, down to 5% battery." |
| `phone.mind.readNow.anywayWhatSaver` (iPhone) | "Read anyway reads until everything is read or you stop it, even with Low Power Mode on, down to 5% battery." |
| `phone.mind.readNow.anywayOn` | "Reading anyway, down to 5% battery." |
| `phone.mind.readNow.floor` | "Reading stopped at 5% battery, after {done}. The rest is read while this phone charges." |
| `phone.mind.readNow.floorNeeds` | "Read now works above 5% battery, or while charging." |
| `phone.mind.readNow.charging` | "Charging, so reading carries on by itself." |
| `phone.mind.readNow.paused` | "You’ve reached this month’s limit. The rest is read from {day}, or when you change the limit." |
| `phone.mind.readNow.changeLimit` | "Change limit" |
| `phone.mind.readNow.notAnswering` | "{host} hasn’t answered since {time}. Nocturne looks again on its own." |
| `phone.mind.readNow.lookAgain` | "Read now looks once more, then reads straight away, and uses some battery." |
| `phone.mind.readNow.stillNot` | "{host} still isn’t answering. Nocturne looks again on its own." |
| `phone.mind.readNow.offline` | "This phone isn’t online. What you keep waits to be read." |
| `phone.mind.readNow.offlineNeeds` | "Read now works once this phone is online." |
| `phone.mind.reading.bodyWaiting` | "Nocturne reads the {count, plural, one {# thing} other {# things}} you kept while this phone charges, a few at a time. Your first page comes tomorrow morning." |
| `phone.mind.reading.waiting` | "Waiting to charge · {count} to read" |
| `phone.mind.reading.noteWaiting` | The why line, then the platform's cost line, as one paragraph |

"Reading what you kept" while charging keeps today's body, progress and note.

**Notifications and iPhone's progress.** No kept thing, title, question or host, ever.

| Key | Copy |
|---|---|
| `phone.notify.readNow.channel` (Android) | "Reading now" |
| `phone.notify.readNow.channelAbout` (Android) | "Shows while Nocturne reads what you kept, because you chose Read now." |
| `phone.notify.readNow.title` | "Reading what you kept" |
| `phone.notify.readNow.progress` | "{done} of {count}" |
| `phone.notify.readNow.stop` (Android) | "Stop" |
| `phone.notify.readNow.done` (Android) | "Everything you kept is read." |
| `phone.notify.readNow.low.title` | "Read now stopped at 20% battery" |
| `phone.notify.readNow.saver.title` (Android) | "Read now stopped: Battery Saver is on" |
| `phone.notify.readNow.saver.title` (iPhone) | "Read now stopped: Low Power Mode is on" |
| `phone.notify.readNow.anyway.text` | "{done} read. Open Nocturne to read anyway." |
| `phone.notify.readNow.floor.title` | "Reading stopped at 5% battery" |
| `phone.notify.readNow.floor.text` | "{done} read. The rest is read while this phone charges." |
| `phone.notify.readNow.stopped.title` | "Read now stopped" |
| `phone.notify.readNow.stopped.text` | "{done} read. Open Nocturne to see why." |
| `phone.notify.readNow.paused.title` (iPhone) | "Reading paused" |
| `phone.notify.readNow.paused.text` (iPhone) | "iPhone paused it when you left Nocturne, after {done} of {count}. Open Nocturne to carry on." |

On iPhone the stop notifications show only when iPhone has let Nocturne run until the stop; a stop
while Nocturne is paused cannot happen, since nothing reads then.

### Accessibility

- **Read now, Read anyway and Stop** are buttons of at least 44 pixels. Pressing Read now or Read
  anyway moves focus to Stop, which takes its place; Stop moves it back.
- **Not pressable** uses `aria-disabled`, so the button stays focusable and its reason, joined with
  `aria-describedby`, is read with it.
- **Progress.** The bar states "{done} of {count} read" to screen readers and is announced at most
  every ten seconds, as the backup's are. The row's words change without being announced.
- **Why it stopped** is a polite status, read once.
- **The notification** is silent and low in importance, with no sound or vibration. Its Stop is a
  real action, reachable with TalkBack, and its progress is read as "{done} of {count}". iPhone's
  own progress is read by VoiceOver as iPhone gives it.
- **200 per cent text.** The block's lines wrap and the buttons grow; the notification and Lock
  Screen renders at 200 per cent show their words whole.
- **Colour and motion.** No state is told by colour alone. With reduced motion the breathing dot is
  still.

### The code it implies

Not written here; for the build.

- **`pass()`** gains a "reading now" override: the charging gate returns `Pass::NotCharging` only
  when the phone is not charging and the person has not chosen Read now or Read anyway. The override
  lets the reader run; the scheduler's page and reflections keep waiting for the charger.
- **The override is set** only by the person's Read now or Read anyway, and **cleared** when a pass
  finds nothing left, at 20 per cent or with battery saving on (Read now), at 5 per cent (either), on
  Stop from Nocturne, the notification or iPhone's progress, on a pause by the limit, on trouble
  (not answering, key refused, no credit, not online), when the mind is turned off, and when
  Nocturne is closed for good. On iPhone a pause keeps it, so reading carries on when the person
  returns. It is never kept across launches.
- **Android.** A foreground service of type `dataSync`, declared in the manifest with
  `FOREGROUND_SERVICE` and `FOREGROUND_SERVICE_DATA_SYNC`, started from the person's tap, running the
  same passes as the app does in front, with a notification channel "Reading now" at low importance.
  It handles `Service.onTimeout` by stopping as on Stop.
- **iPhone.** On iOS 26 and later, a `BGContinuedProcessingTask` submitted from the person's tap,
  reporting progress as things are read, with its identifier in `BGTaskSchedulerPermittedIdentifiers`.
  On iOS 17 to 25, the time iPhone grants as the app leaves (`beginBackgroundTask`). On expiry,
  either way, reading pauses cleanly after the thing being read and one local notification is
  scheduled.
- **The phone's power reading** (`Power` in the system plugin) gains the battery level and whether
  battery saving is on, and `PhoneHost::battery_percent` returns the level.
- **The mind's status** tells the interface why reading waits (charging, on battery, reading now,
  reading anyway, paused while away, and why Read now stopped), so the words above are chosen from
  facts, never guessed.
- **Nothing new leaves the phone:** the same requests to the one chosen host, sooner.

## What this does not change

- The check, its question and its outcomes, apart from reusing the list from the last look and
  letting the question decide for a name not on the list.
- The words of steps 1, 3 and 4, and of every other screen named above. Only where the bar sits.
- The labels "API URL", "API key", "Model name" on the phone, and Connect's on the computer.
- Invariant 2: one look at the chosen service's list when it is checked, on the person's request.
  Read now sends the same requests to the same host, sooner, in the background only while the
  person's Read now lasts.
- When the phone reads on the charger, and that the page waits for it.

## Checks for the build

- At 412 by 915 and at 200 per cent text, every screen named in [Which phone
  screens](#which-phone-screens) shows its main button on arrival, light and dark, with axe clean.
- With the keyboard up on step 2, the focused field is in view and the bar sits after the words.
- With a stand-in service: a list of 6, of 214, an empty list, "not found", a refused key, a busy
  service and no network each give the state above, and the phone's Sent online lists exactly one
  connection check for each look.
- "Check" right after choosing makes no second look at the list.
- A name not on a list of names, which the stand-in answers to, passes the check after the warning.
- Changing the URL or key after a look drops the list.
- No grouping or description of a name appears anywhere.
- On battery with things waiting, This phone says "Waiting to charge" and nothing is read until Read
  now. Read now stops on Stop, at 20 per cent, with battery saving on, on a pause by the limit and on
  each trouble, saying why each time; Read anyway runs past 20 per cent and battery saving and stops
  at 5.
- On Android, Read now carries on with Nocturne in the background and the screen off, its
  notification shows progress and Stop, and the service ends at every stop. On iPhone 26 and later
  iPhone's own progress shows; on iPhone 17 to 25 reading pauses soon after leaving, the paused
  notification shows, and reading carries on when Nocturne opens.
- No notification names a kept thing, a question or a host.
- After Read now ends, no pass reads until the charger or the next Read now.

## As built

Where the page left a choice, the build reads it this way:

- **The look on the computer.** The computer's check goes through its own engine, which reads the
  service's list itself each time it is given an address. "Choose from {host}'s list…" takes its
  look with the same client the phone uses, listed in Sent online as a connection check, so a
  check after it still has the engine's own look.
- **The list kept for the check.** On the phone the check uses the list from the person's look
  once, when the address and key are the same and the look is under 15 minutes old.
- **Copy for each phone.** The lines that differ between Android and iPhone are keys ending in
  `.android` and `.iphone`, such as `phone.mind.readNow.cost.android`.
- **Change limit** opens the mind's setup, as "Change…" does, since the phone has no limit screen
  of its own.
- **The computer's dialog** uses the app's own dialog, with its title and Close as every dialog
  has them.
- **Done and charging lines** show while they hold, until Nocturne is closed or a new Read now
  starts, rather than only until the person leaves This phone.
