---
type: Design
title: Help with Nocturne, from the ask box
description: Approved design for answering how-to questions about Nocturne from the guide that ships with it, written on this computer with guide receipts beside memory receipts, a "Did you mean help with Nocturne?" prompt when unsure, a guide reader in Settings, and an evaluation gate before each size of computer gets written help.
status: stable
tags: [approved, ux, ask, help, guide, copy]
---

# Help with Nocturne, from the ask box

A tester who wanted to use Nocturne on their phone did not open the guide. They asked Nocturne
itself how to connect the phone. The ask box answers only from what the person kept, so the best it
could do was "Nothing you've kept answers that." and an offer to keep a photo. If they had once
kept a note about Gran's new phone number, it might have offered that as a near match. The help
they needed was in the guide.

This page is the design for Nocturne helping someone use Nocturne, from the same ask box, without
becoming a general chat assistant and without blurring what is the person's own memory. The owner
chose the flow on 2026-11-16 (see Owner decisions). The copy below is approved; the build follows
this page.

## Mockups

In `docs/design/mockups/in-app-help/`, rendered at 1280 by 800 in the dark theme. Each HTML file
imports the app's own stylesheet for its tokens and fonts, so serve the repository root over HTTP
after `make ui/install` and open it there; add `?theme=light` or `?theme=dark`, and `&state=` to
pick a view.

| View | File and state | Render |
|---|---|---|
| A confident help answer, written here | `answer.html`, `confident` | `answer-confident-dark.png` |
| A confident answer from the guide and what you kept | `answer.html`, `mixed` | `answer-mixed-dark.png` |
| The unsure prompt | `answer.html`, `unsure` | `answer-unsure-dark.png` |
| The guide's own words, on a size of computer that has not passed the gate | `answer.html`, `words` | `answer-words-dark.png` |
| The guide reader, opened from an answer | `guide.html` | `guide-reader-dark.png` |
| The empty Ask screen, today and after | `empty-ask.html`, `before` and `after` | `empty-ask-before-dark.png`, `empty-ask-after-dark.png` |
| Settings, today and after | `settings.html`, `before` and `after` | `settings-before-dark.png`, `settings-after-dark.png` |

In the "after" views a dashed outline marks what is new; it is not part of the design.

## The problem

- People ask the thing in front of them. The ask box is the one place in Nocturne that invites a
  question in plain words, so "how do I" questions arrive there.
- The answer today is wrong in kind, not just empty. "Nothing you've kept answers that. A photo, a
  note or a voice note about it would do it." tells someone who wanted help to keep a photo.
- Memories can answer a help question badly. A question with "phone", "key", "page" or "delete" in
  it finds memories with those words.
- The guide exists and is written for this reader, but it is on the website. Sending people there
  means a browser, a network visit and possibly a guide for a different version.

## Owner decisions

Made on 2026-11-16, from the three options first proposed (A, help mixed into the answer; B, a help
card above what you kept; C, help only when clearly about Nocturne).

| Question | Decision |
|---|---|
| How help appears | A when the match is confident: the guide sections join the facts, one answer is written, and guide receipts sit beside memory receipts. C's unsure path when it is not: the normal answer, then "Did you mean help with Nocturne?" |
| Buttons | A help answer can carry the open-screen button, and every help answer offers "Open the guide" |
| "Answers only from what you kept" | Reworded everywhere it is claimed, without naming the guide (see The claim) |
| The guide reader | Gets its own entry in Settings |
| Paired phones | No help in the first version |
| Help that knows the current state | Later |
| Guide accuracy | The in-scope guide pages are checked and marked stable in the same change that ships help |
| "Ways people ask" | In each guide page's front matter |
| Online answers | Help answers are always written on this computer (see Two risks, settled) |
| Blending on small computers | An evaluation gate per size of computer; a size that fails shows the guide's own words (see Two risks, settled) |

Option B's card survives only as the fallback for a size of computer that has not passed the gate.

## What people will ask

Written the way a person who is not technical would type them, from the guide's own topics.

| They ask | The guide section that answers |
|---|---|
| How do I get Nocturne on my phone? | Away from home: Turn it on, Add a phone |
| Can I use Nocturne on my phone when I'm out? | Away from home: the opening, What the phone can do |
| I've lost my phone. What do I do? | Away from home: Remove a lost phone |
| My phone says it can't connect any more | Away from home: When the address changes |
| How do I keep something without opening the window? | On your desktop: Quick keep shortcut |
| Where has the little icon by the clock gone? | On your desktop: The tray |
| Can it tell me when my page is ready? | On your desktop: Notifications |
| How do I make it start when I turn the computer on? | On your desktop: Start with this computer |
| Can it pick up everything I save in Downloads? | On your desktop: A folder that keeps what you drop in it |
| Why can't it hear me? | On your desktop: Sound |
| How do I move everything to my new laptop? | Your data: Back up and restore |
| How do I get rid of a photo I kept? | Your data: Let go, bring back, forget |
| A note I kept has disappeared | Your data: Let go, bring back, forget |
| Does anything I keep go on the internet? | Your data: How you can tell nothing leaves your computer |
| Can it use the AI I already pay for? | Online answers: Turn it on |
| How do I stop it sending my questions online? | Online answers: Turn it off and forget the key |
| Can it remind me about the dentist? | Using Nocturne: What this version does not do |
| How do I take Nocturne off my computer? | Install and remove: Take Nocturne off |

Questions that must stay memory questions, because they share words with the guide:

| They ask | Why it is not help |
|---|---|
| What is Gran's new phone number? | "phone" is about Gran, not Away from home |
| When did Leo get his new phone? | A fact from the person's life |
| Where did I put the spare key? | "key" is not the Online answers key |
| What did the plumber say about turning the water off? | "turn off" is not Nocturne's |
| How do I get to the new dentist? | "how do I" about the person's own plans |

Mixed questions, which need both the guide and what the person kept:

| They ask | The guide | What you kept |
|---|---|---|
| How do I get rid of the photo of Gran's letter? | Let go, bring back, forget | The photo |
| The note about the boiler has gone. How do I get it back? | Let go, bring back, forget | The boiler note |
| Can I see Leo's trip form on my phone when I'm out? | What the phone can do | The trip form |
| Will my phone see Gran's medication list if I mark it private? | What the phone can do | The medication list |
| How do I move my notes about the house sale to my new laptop? | Back up and restore | The house sale notes |
| Did my question about half term go online? | What is sent | The half term note |

All three lists are the evaluation set for the gate below.

## The flow

1. **Search both.** The question is searched against memories and against the guide sections in
   scope, together (see Finding the right section).
2. **Confident match.** The matching guide sections join the facts, marked as guide text, and the
   mind writes one answer from both. Each sentence cites its source: a guide section or a memory.
   Guide receipts sit beside memory receipts. The answer is always written on this computer. It
   ends with "Open {screen}" when a cited section is about a screen, and always with "Open the
   guide".
3. **Unsure match.** The normal memory answer, exactly as today, followed by "Did you mean help
   with Nocturne?" and "Show the guide's answer". Pressing it asks the same question again as a
   confident help question and replaces the answer on screen, the way "Ask on this computer"
   already does.
4. **No match.** Today's behaviour.
5. **On a size of computer that has not passed the gate**, a confident match shows the guide's own
   words instead of a written answer: a card labelled "From the Nocturne guide" with the page's
   opening sentence and the first sentence of each step, its receipts and both buttons, and the
   memory answer below it under "From what you kept".

### Finding the right section

- **No tool call and no extra model call.** The answer pipeline already turns every question into
  a meaning vector with the finding model before it searches memories. The guide sections are
  searched with that same vector, beside the memory search, and with the same stemmed word search.
- **Ways people ask.** Meaning alone will not reliably join "connect my phone" to "Away from home".
  Each guide page lists, in its front matter, the ways people ask for each of its sections. A
  phrase match is the strongest signal there is. For example, in `docs/guide/away-from-home.md`:

  ```yaml
  asked:
    Add a phone: [connect my phone, pair a phone, get it on my phone, phone app]
    Remove a lost phone: [lost my phone, phone was stolen, stop a phone]
  ```

- **A help signal.** The question reads as being about Nocturne: it names Nocturne, a screen or a
  control (from the control chip registry), or it asks how to do something rather than what is
  true.
- **Confident** is a phrase match, or a close meaning match together with a help signal.
  **Unsure** is one without the other. The thresholds are set from the evaluation set.
- The guide is read for meaning on this computer, with the same finding model as memories, and
  again when the person changes that model, alongside the re-read of memories.

### Receipts

Every guide sentence in an answer cites a guide section, the same way every other sentence cites
an original. A guide receipt is the existing receipt card (or, in Earlier turns, the existing
receipt chip) with a book where the thumbnail goes, the section heading as its title, the guide's
sentence as its quote and "Nocturne guide · {page}" where the date goes. Pressing it opens the
section in the guide reader. The bold spans in the answer work as they do today: hovering one
outlines its receipt, whichever kind it is.

"That's wrong", "Keep forever" and "Delete this…" act only on the memories an answer used, as they
do today. A guide section is not a memory, so it never appears in their list, and an answer that
used no memories shows none of them.

### Buttons

- **"Open {screen}"**, primary, where `{screen}` is that screen's own title from the copy deck:
  "Open Away from home", "Open Privacy". At most one, for the first cited section that is about a
  screen. Back returns to the answer, which the ask screen already restores.
- **"Open the guide"**, secondary, on every help answer. It opens the guide reader at the first
  cited section.
- Inside the answer and the guide reader, controls are named with **control chips** from the
  parallel proposal "Control chips and easier reading in setup" (`docs/design/control-chips.md`):
  the control's own words in a frame, never pressable. This design uses that shared component and
  does not redesign it.

### The guide reader

A plain, read-only screen titled "Nocturne guide". It shows one guide page: its title, its opening
sentence, its sections, and "On this page" and "In this guide" lists to move around. Opened from an
answer, it scrolls to the cited section and marks it; Back says "Ask". Opened from Settings, it
shows the guide's first page; Back says "Settings". A line at the foot of the lists says which
version the guide came with and that reading it sends nothing.

In Settings it is the last row, after "Choose what runs each job": "Nocturne guide", "How to do
things in Nocturne", with "Read ›" on the right, like the rows for Away from home and Online answers.

## Two risks, settled

### Online answers: help is always written on this computer

With Online answers on, a mixed answer would send guide text to the person's provider, which
invariant 2 does not allow. There were two ways out: always write help answers here, or widen the
invariant to allow the guide, which is public. **Help answers are always written on this
computer.**

- Invariant 2 is the privacy promise, and it lists exactly what may leave. Widening it for
  something that is not personal is still a widening, with a decision record, a new kind of line
  in Sent online and new words in the setup that tells people what is sent. Keeping help here costs
  none of that.
- The help part gains little from a large online mind: the steps are already written, and the gate
  below decides whether the mind on this computer may rewrite them at all.
- Help never spends the person's online limit and never waits on their provider.
- The cost: on a small computer with Online answers on, a mixed question that would have been
  written online is written here instead. The mark says so, and the gate keeps a small computer
  that blends from writing help at all.

What the person sees:

| Online answers | Mark above "You asked" | Receipts |
|---|---|---|
| Off | "Answered from the Nocturne guide", or "Answered from the Nocturne guide and 2 things you kept" | Guide receipts and memory receipts, side by side |
| On, or "Online" chosen for this question | The laptop icon, then "Answered on this computer from the Nocturne guide · help is always written here", or the same with "and 2 things you kept". No "See what was sent", because nothing was sent | The same |
| On, an unsure match | The normal mark for wherever the memory answer was written. "Show the guide's answer" then asks again here, with the mark above | The same as today, then guide receipts after asking again |

Nothing about a help answer is added to Sent online, because nothing left the computer.

### Blending on small computers: a gate for each size

A small mind given guide steps and a memory about Gran's phone can write "Add Gran's phone". Before
written help ships on a size of computer (Tiny, Small, Standard, Large, each with its preset
models), that size passes this gate. It runs on the three lists above, against a fixed set of
memories seeded to share words with the guide (Gran's phone number, Leo's phone, the spare key, the
plumber, the dentist's address, the photo of Gran's letter, the boiler note, the trip form, the
medication list, the house sale notes, the half term note). Each question is asked three times on
this computer.

A size passes when all of these hold, in every run:

| Check | Pass |
|---|---|
| Finds the right section | The expected guide section is cited for at least 16 of the 18 help questions |
| No help where none was asked | None of the 5 memory questions gets a confident help answer or a guide receipt; the unsure prompt appears for at most 1 of them |
| No blending | No sentence that cites a guide section holds a name, number, date, place or thing from a memory, and no sentence that cites a memory holds a step, screen or control from the guide. Zero allowed. A script compares each sentence with its cited sources, and a person reads every sentence the script cannot settle |
| Faithful steps | Every screen or control named in a guide sentence is spelled as the control's own words (the chip registry), steps keep the guide's order, and no step is added. Zero misses allowed |
| Both halves | At least 5 of the 6 mixed questions cite both a guide section and the right memory |
| Speed | Within the existing answer target: the first useful sentence in under 3 seconds at the 95th percentile, with the mind awake |

**A size that fails** shows the guide's own words for confident matches (flow step 5, the `words`
mockup) and keeps the unsure prompt. Nothing else changes for it. The size is tested again whenever
its answering model, the answer instructions, its preset or the in-scope guide sections change,
and the release checklist records which sizes write help. A person who picked their own answering
model in Choosing models gets the guide's own words, because that model has not been through the
gate.

## The claim

The site, the guide and the app say Nocturne "answers only from what you kept". Help makes that
untrue as written. The owner asked for wording that does not name the guide, towards "answers only
from what Nocturne knows".

**Proposed: "answers only from what it can show you".** "What Nocturne knows" was considered and
not chosen: the mind inside Nocturne "knows" a great deal from its training, so the phrase invites
general questions, which is the very thing [the overview](/overview.md) rules out. "What it can show
you" is true of both sources, because every answer shows its originals or its guide sections
underneath, and it ties the promise to the receipts, which a person can check.

| Where | Before | After |
|---|---|---|
| `site/i18n/en.json`, `ask.body` (shown by `site/index.html`) | Ask in plain words. Nocturne answers only from what you kept, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. | Ask in plain words. Nocturne answers only from what it can show you, with the originals underneath so you can check. When it does not know, it says so. If it gets something wrong, tell it what is right and it remembers. |
| `site/i18n/de.json`, `ask.body.text` | …antwortet nur aus dem, was du festgehalten hast, mit den Originalen darunter… | …antwortet nur mit dem, was es dir zeigen kann, mit den Originalen darunter… |
| `site/i18n/fr.json`, `ask.body.text` | …répond uniquement à partir de ce que vous avez gardé, avec les originaux en dessous… | …répond uniquement à partir de ce qu’il peut vous montrer, avec les originaux en dessous… |
| `site/i18n/es.json`, `ask.body.text` | …responde solo a partir de lo que guardaste, con los originales debajo… | …responde solo a partir de lo que puede mostrarte, con los originales debajo… |
| `site/i18n/it.json`, `ask.body.text` | …risponde solo in base a ciò che hai conservato, con gli originali sotto… | …risponde solo in base a ciò che può mostrarti, con gli originali sotto… |
| `site/i18n/nl.json`, `ask.body.text` | …antwoordt alleen uit wat jij hebt bewaard, met de originelen eronder… | …antwoordt alleen met wat het je kan laten zien, met de originelen eronder… |
| `site/i18n/{de,fr,es,it,nl}.json`, `ask.body.from` | The old English sentence | The new English sentence, with the new translation, since the site build refuses a translation made from older English |
| `docs/design/site-copy/{de,fr,es,it,nl}.md`, the `ask.body` rows | The old English and translation | The new English and translation, as above |
| `docs/guide/index.md`, What Nocturne does | Ask it a question and it answers only from what you kept, and shows you the originals it used. | Ask it a question and it answers only from what it can show you, with the originals it used underneath. |
| `ui/src/copy/en-GB/ask.json`, `ask.empty.body` | Nocturne answers only from what you’ve kept, and shows where each answer came from. | Nocturne answers only from what it can show you, with where each answer came from underneath. It can also help you use Nocturne. |
| `ui/src/copy/en-GB/ask.json`, `ask.nothing.body` | Nocturne only answers from what you keep. A photo, a note or a voice note about it would do it. | Nocturne answers from what you keep. A photo, a note or a voice note about it would do it. |
| `docs/overview.md`, Out of scope | A general chat assistant. Questions are answered from what the person kept. | A general chat assistant. Questions are answered from what the person kept, and questions about using Nocturne from its own guide. |

The five translations are drafts for the usual language check in
[the site in five languages](site-localisation.md), with each language's glossary.

Not changed:

- `phone.ask.empty.body` and `phone.ask.nothing.body`. A paired phone gets no help in the first
  version, so "answers only from what you've kept" stays true there. They change when phones get
  help.
- `docs/design/landing-trust-copy.md`, which records what was approved on its day.
- `ask.empty.title`, "Ask about anything you’ve kept.", which invites rather than limits.

## Copy

Approved for the en-GB copy deck. `{screen}` is the target screen's own title key, `{page}` and
`{section}` the guide's own headings, `{version}` the app's version.

### Answers

| Key | Words |
|---|---|
| `ask.mark.guide` | Answered from the Nocturne guide |
| `ask.mark.guideAndKept` | Answered from the Nocturne guide and {count, plural, one {# thing} other {# things}} you kept |
| `ask.mark.local.guide` | Answered on this computer from the Nocturne guide · help is always written here |
| `ask.mark.local.guideAndKept` | Answered on this computer from the Nocturne guide and {count, plural, one {# thing} other {# things}} you kept · help is always written here |
| `ask.help.open` | Open {screen} |
| `ask.help.openGuide` | Open the guide |
| `ask.help.receipt.meta` | Nocturne guide · {page} |
| `ask.help.receipt.open` | Read “{section}” in the Nocturne guide |
| `ask.announce.done.guide` | Answer ready. From the Nocturne guide. {answer} |
| `ask.help.didYouMean` | Did you mean help with Nocturne? |
| `ask.help.didYouMean.hint` | The Nocturne guide covers this in {page}. |
| `ask.help.didYouMean.show` | Show the guide’s answer |

### The guide's own words (a size that has not passed the gate)

| Key | Words |
|---|---|
| `ask.help.label` | From the Nocturne guide |
| `ask.help.kept` | From what you kept |
| `ask.help.kept.nothing` | Nothing you’ve kept is about this. |
| `ask.help.receipts` | Where this came from |

### The guide reader and Settings

| Key | Words |
|---|---|
| `screen.guide.title` | Nocturne guide |
| `guide.page.contents` | On this page |
| `guide.contents` | In this guide |
| `guide.version` | This guide came with Nocturne {version}. It is on this computer, and reading it sends nothing. |
| `guide.row.label` | Nocturne guide |
| `guide.row.hint` | How to do things in Nocturne |
| `guide.row.value` | Read |
| `guide.row.open` | Open the Nocturne guide |

### The empty Ask screen

| Key | Words |
|---|---|
| `ask.empty.body` | As in The claim |
| `ask.example.4`, new | How do I use Nocturne on my phone? |

## Rules

1. **Never a memory.** Guide text is not kept, read, filed, tidied or linked. It never reaches the
   morning page, export, a backup, "What Nocturne remembers" or search. Citing it does not count as
   use of anything, and no correction can be written from it. A help answer moves into Earlier
   like any answer and is forgotten with the conversation.
2. **Local only.** The guide ships inside the app, built from `docs/guide` at the release's commit,
   so it always describes the version installed. It is never fetched, updated or checked online.
   Help answers are always written on this computer, and which help was shown is not counted or
   logged.
3. **Receipts.** Every guide sentence cites its guide section, and guide receipts say "Nocturne
   guide", never "you kept".
4. **No jargon.** A section is in scope only if it passes the same checks as interface copy: the
   jargon test in `ui/src/copy/jargon.test.ts` and the extra words Away from home and Online
   answers avoid in [experience principles](experience-principles.md).
5. **Not on a paired phone** in the first version. A phone's answers are as today.
6. **Not aware of state** in the first version. Help says what the guide says; the screen the
   button opens shows how things are.
7. **Gated.** Written help ships on a size of computer only after it passes the gate above.

## Which guide pages are in scope

Every section was scanned against rule 4. "Reword" sections are wanted but hold a word the
interface may not say; they are reworded in the change that ships help.

| Page | In | Reword first | Out |
|---|---|---|---|
| Using Nocturne (index) | What Nocturne does, What this version does not do | | |
| On your desktop | All six sections | | |
| Away from home | Turn it on, Add a phone, What the phone can do, When the address changes, Switching connection, Remove a lost phone, Turn it off | Most private sign-in ("certificate"), What the companies can see ("certificate", "server") | |
| Online answers | What is sent, What is never sent, Turn it off and forget the key, Where the key is kept | Turn it on ("server", "tailnet"), What the provider sees, Cost and the limit, When it does not work, See what was sent (each says "the provider", where the interface allows only "your provider") | |
| Your data | Where it is, Look at what it holds, Back up and restore, Let go, bring back, forget, How you can tell nothing leaves your computer | Take a copy, Delete everything ("model choices") | |
| Install and remove | The first run, Take Nocturne off | | The rest: it is read before Nocturne is installed, and Run it as a server is for another reader |
| Faster with the NPU | | | All: technical words throughout, and steps that need administrator rights, which Nocturne never asks for |
| Choosing models | | | All: it belongs to the advanced view, and says "model" by design |

## Who keeps it accurate

Help makes the guide part of the interface: a wrong step in the guide becomes a wrong instruction
inside Nocturne.

- **One text.** Help is the guide. There is no second help text to drift from it.
- **Checked and stable in the same change.** The change that ships help checks every in-scope page
  against the app, rewords the sections above, adds each page's `asked` list and moves those pages
  from `status: draft` to `status: stable`. Only stable pages are bundled.
- **Names checked by a test.** A screen or control a guide section names is marked as a control
  chip, and the chip registry maps it to its copy key. The build fails when a section names one the
  registry does not know, or whose words have changed. The check found three drifts already, each
  fixed in that change:
  - "Open Away from home and turn it on.", where the first-time button says "Set up…".
  - "choose Scan the code", where the phone says "Scan the new code".
  - "Forget removes a thing for good", where the button on an answer says "Delete this…".
- **The person who changes a screen changes its section**, in the same change, as the project's
  rules already ask of docs. The designer approves the guide's words as for any copy.
- **The release checklist** gains two lines: the in-app guide was read against this build, and
  which sizes of computer passed the gate.

## Localisation, later

Nocturne speaks English (UK) only today, so help is English only. When the app gains a language,
the in-scope sections are translated with it, using the site's glossary, and so are their `asked`
lists. A section with no translation is not used in that language rather than shown in English.

## Considered and not chosen

- **A tool call to "look up help".** Small local minds are unreliable at choosing tools, and each
  call adds a wait. The search already has the question's meaning.
- **Linking to the guide on the website.** A network visit Nocturne does not need, out of the app,
  and possibly about a different version.
- **Option B, a help card above every answer.** It kept the two sources apart on screen, but the
  owner preferred one answer. Its card is kept as the fallback for a size that fails the gate.
- **Option C alone.** A confident wrong guess would hide the person's own answer.
- **Widening invariant 2 for the guide.** See Two risks, settled.

## Not changed

- How memory answers are found, written, marked and cited.
- Online answers, what it sends and its log.
- The keep and ask bar, and how it decides between keep and ask.
- The control chip, which the control chips proposal designs.
- Answers on a paired phone.

## Review

A first draft offered three options. The owner chose the flow, the claim rewording, the Settings
entry and the accuracy rules on 2026-11-16, and asked for the two risks above to be settled in the
design rather than left open. This page records those decisions and is approved.
