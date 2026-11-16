---
type: Design
title: Control chips and easier reading in setup
description: A proposal to show a named place or control in the copy with its own icon and words, to mark buttons that open more, and to set measurable reading rules for setup, starting from the Tiny box on step 1.
status: stable
tags: [approved, ux, accessibility, copy, setup, dyslexia]
---

# Control chips and easier reading in setup

A tester with dyslexia read step 1 of setup with Tiny chosen. Under the size choice is a box that
says "On a small computer, you can later choose to have answers written online. See Settings." with
a quiet "Why this one?" button and "Set up · 3.2 GB" beside it. They could not read
the box, and asked for icons that look like the buttons the words name.

This page sets three changes and one set of rules. All of it is in the interface.

The mockup is `docs/design/mockups/control-chips/setup-box.html`, rendered to
`before-after-light.png` and `before-after-dark.png` beside it. Each render shows the box before and
after, the "Why this one?" text open, and where the chips lead once setup is done. The page copies
the app's colour tokens and loads its fonts from `ui/node_modules`, so open it after
`make ui/install`; add `?theme=dark` or `?theme=light`.

## The problem

The box asks the reader to hold four things in one sentence: a condition ("On a small computer"),
a time ("later"), a choice ("have answers written online") and a place ("See Settings"). Four
findings make it worse than its length suggests:

| Finding | Why it matters |
|---|---|
| "See Settings" names a place with nothing to look for | A reader who loses their place in text scans for shapes. A bare word in the middle of a sentence gives them none, and nothing ties it to the Settings link in the rail |
| The condition is wrong | The box shows whenever Tiny is chosen, on any computer. "On a small computer" also collides with the size called Small two cards above it, so a careful reader wonders whether it applies to Small too |
| The place does not exist yet | During setup there is no rail and no Settings. "See Settings" sounds like an instruction to act now |
| "Why this one?" looks like a sentence, not a control | It is a quiet button: accent text, no shape, no sign that it opens anything |

Two smaller things sit beside it. The Settings link in the rail is text only: the gear in the icon
set is drawn nowhere in the app, so "an icon that looks like the button" has no button to look
like yet. And the box is the only running text on the card at full ink, so it pulls the eye before the
reader knows why.

### Who else it helps

- People who read in a second language, who match words to the screen rather than parse them.
- People using screen magnification, who see a few words at a time and need landmarks.
- Older readers and anyone tired, skimming, or reading on a small window.
- Translators: a chip carries the control's own words, so a translated sentence can never name a
  control by a word the control does not use.
- Everyone, because the inventory below found copy that already disagrees with its controls:
  "Settings, then Advanced details, then Online answers" where the controls say "Show advanced
  details" and "Connect an online service", and "Scan the code" where the phone's button says "Scan
  the new code".

## Decisions

| Question | Decision |
|---|---|
| What a chip is | The icon and exact words of a place or control in Nocturne, set inside a sentence, framed so it reads as a reference |
| Does a chip navigate | No. It is never clickable, in setup or anywhere else. When the copy wants the person to go somewhere now, it uses a real link or button, as "Away from home settings" and "Open online answers" already do. One look, one meaning |
| Does it copy the control's shape | No. It copies the icon and the words, which are what the eye matches. Copying the shape would make a second button that does nothing |
| Icon | Only when the real control shows that icon. A chip never teaches an icon the person cannot find |
| Settings in the rail | The rail's Settings link gains the gear, beside its word, so the Settings chip can carry it. This is a change to the rail and is part of this proposal |
| Disclosure buttons | A chevron before the words, pointing right when shut and down when open, on every button that shows or hides text in place |
| Dyslexia fonts | Out of scope. Controlled studies of Dyslexie and OpenDyslexic found no gain in reading speed or accuracy (Kuster and others, 2018; Wery and Diliberto, 2017). The reading rules below work on spacing, line length and short sentences instead, which every reader gets |

## The chip

### When copy uses a chip

A chip is used when a sentence tells the person to go to, find or press something in Nocturne
that is not on the screen they are reading. In a path, each control the person presses gets a chip,
in order, with the words that control shows.

- At most two chips in a sentence, or three when the sentence is a path joined by "then" or
  "under".
- The chip's words are the control's or the screen's own words, from its own copy key. A sentence
  is written around the chip, never the chip around the sentence.
- The sentence must still read correctly as plain text with the frames removed, because that is
  what a screen reader, a copy and paste, and a notification hear.

### When it does not

- The control is on the same screen. The real control is the anchor; a chip beside it would be a
  second, dead copy. "Press Keep when you are done" stays plain while Keep is on screen.
- The place is the one the person is on. "You can try again from Privacy" on the Privacy screen
  becomes "You can try again."
- A feature is named in passing, not as a destination: "Online answers are paused", "Away from home
  is on", the Privacy promise.
- Another app's or the desktop's controls: Safari's Share, the desktop's keyboard settings, a
  service's own pages. Nocturne cannot draw them faithfully. These keep the exact words in bold, as
  the phone guide already does.
- Headings, button labels, toasts, notices, tray menus and system notifications. These are short,
  announced as plain text or drawn by the system. Where a notice needs to send the person somewhere,
  it uses its action button.
- Keyboard keys. A key cap is a separate question (see Open questions).

### How it looks

| Property | Value | Why |
|---|---|---|
| Frame | 1px `--rule` border, `--r-sm` corners, `--bg` fill | The same corner as a button, but a hairline and a tint instead of an ink border or a solid fill, so it reads as a label laid in the text |
| Text | The sentence's size, weight 500, `--ink` | Same size so the line does not jump; weight so the eye finds it |
| Icon | 14px Lucide line icon at the app's 1.5 stroke, 5px before the words | Matches the rail icon at a size that sits inside a line of text |
| Padding | 0 above and below, 6px each side | Keeps the line height of the paragraph |
| Wrapping | Never breaks inside a chip | A chip split over two lines stops being one shape |
| Pointer | The text cursor, no hover, no focus | Nothing to press, so nothing reacts |

Against the buttons on the same screen: primary is solid ink, secondary has an ink border and is
40px tall, quiet is accent text. A chip is none of these: lighter edge, tinted fill, text height,
inside a sentence.

### What a screen reader hears

The chip is a plain span. The icon is hidden from assistive technology, as every icon beside a
label already is. There is no role, no label override and no tooltip. A screen reader reads the
sentence as written: "Find it in Settings, under Online answers." It never hears "button" or
"link" for something that is neither.

### Light, dark and high contrast

The chip uses tokens only, so it follows the theme. In light it is a pale grey label on the cream
sheet; in dark it is a darker inset on the dark sheet, with the same ink text as the sentence. The
text meets AA in both because it is `--ink` on `--bg`. The border is decoration and is not relied
on to carry meaning; the icon and the weight do that. In forced colours the border becomes the
system text colour and stays visible. In print it is a thin black outline.

### Build notes

For whoever builds it, not part of the design: copy marks a chip as an argument the screen fills,
for example `Find it in {settings}, under {onlineAnswers}.`, so the deck stays plain text. A small
registry maps each chip to the copy key of its control and its icon, if any. A test fails when a
chip names a control that is not in the registry, or when a registry icon is not the one the real
control draws. The existing tests that match whole sentences by text will need to match the
sentence across the chip.

## The setup box: before and after

| | Words |
|---|---|
| Before, `setup.tiny.online` | On a small computer, you can later choose to have answers written online. See Settings. |
| After, `setup.tiny.online` | Later, an online service you choose can write your answers. Nothing is sent until you turn it on. |
| After, new `setup.tiny.online.where` | Find it in [⚙ Settings], under [Online answers]. |

Square brackets mark a chip; ⚙ marks the gear.

- One idea per sentence: what is possible, that it is off, where it lives. Seven to ten words each.
- "Later" comes first, so the reader knows before anything else that there is nothing to do now.
- "Small computer" goes: it was wrong for a big computer with Tiny chosen, and it echoed the Small
  size.
- "An online service you choose" matches the Online answers row in Settings word for word.
- "Nothing is sent until you turn it on" keeps the welcome screen's promise in sight at the moment
  the person reads the word "online".
- The chips work during setup because they do not navigate: they tell the person what to look for
  later, and the gear and the words will be there in the rail.

Layout: the rule above the box stays. The two sentences sit at `--ink` in one paragraph, and the
"Find it in" line sits below at `--ink2`, 8px apart. Both paragraphs are capped at 60 characters a
line, at line height 1.5.

## The disclosure

"Why this one?" keeps its words and its quiet style, and gains a chevron before them: pointing
right while shut, turning down when open, over 120ms, with no turn under reduced motion. It already
announces "collapsed" and "expanded"; the chevron makes the same state visible.

The app has two disclosure looks today. Online answers' "What exactly is sent?" points right and
turns down; Back up's "Show advanced details" points down and flips up. This proposal makes the
first one the rule and moves Back up to it, so a chevron means the same thing on every screen.

The open text is rewritten to the setup reading rules below, as a short lead, a list of the four
sizes and a closing line:

| | Words |
|---|---|
| Before, `setup.why.body` | Nocturne picks the biggest mind this computer can run comfortably, which depends on its computer memory: {ram} GB here. Large needs {large} GB or more, Standard {standard} GB, Small {small} GB and Tiny {tiny} GB. You can always choose a smaller one: it uses less memory, space and battery. Small makes answers and tidying slower; Tiny also gives simpler answers that may miss details. Everything still works. |
| After, `setup.why.lead` and `setup.why.here` | Nocturne picks the biggest mind this computer runs comfortably. It depends on computer memory. (New paragraph.) This computer has {ram} GB. |
| After, table | Large: {large} GB or more · Standard: {standard} GB or more · Small: {small} GB or more · Tiny: {tiny} GB or more, one row each, the size in the first column |
| After, `setup.why.close` | A smaller mind uses less memory, space and battery. Everything still works. |

The line about Small and Tiny being slower and simpler is dropped from here because each size's own
card already says it.

## Inventory

Every string in `ui/src/copy/en-GB/` that sends the person to a place or control, from a search for
Settings, Privacy, Away from home, Online answers, Advanced details, "choose", "open", "press",
"see" and quoted control names. Strings that only name a feature in passing, and real links and
buttons, are left out. [Square brackets] mark a chip; ⚙ marks the gear.

| Key | Now | Proposed | Chips |
|---|---|---|---|
| `setup.tiny.online` | On a small computer, you can later choose to have answers written online. See Settings. | Later, an online service you choose can write your answers. Nothing is sent until you turn it on. Find it in [⚙ Settings], under [Online answers]. | Settings, Online answers |
| `models.aloud.only` | {name} is the only voice Nocturne can use for now. Choose its voice and speed in Settings, under Voice. | {name} is the only voice Nocturne can use for now. Choose its voice and speed in [⚙ Settings], under [Voice]. | Settings, Voice |
| `backup.aside.out.phones.body` | You add them again in Away from home. | You add them again in [⚙ Settings], under [Away from home]. | Settings, Away from home |
| `backup.saved.body` | To use it on another computer, install Nocturne there and choose Restore from a backup. You’ll need the passphrase. | On another computer, install Nocturne and choose [Restore from a backup]. You’ll need the passphrase. | Restore from a backup |
| `restore.confirm.body` | … You can bring back what was here from Privacy. | Same words. | Privacy |
| `restore.restored.online.later` | Off until you paste the key for your provider again. You can do that in Settings later. | Off until you paste the key for your provider again. Do that later in [⚙ Settings], under [Online answers]. | Settings, Online answers |
| `restore.restored.before.body` | Put aside {day}. Bring it back any time from Privacy. | Same words. | Privacy |
| `privacy.backup.restore.kept` | Restoring didn’t finish, so Nocturne kept what was here. You can try again from Privacy. | Restoring didn’t finish, so Nocturne kept what was here. You can try again. | None: it shows on Privacy |
| `models.confirm.listed` | The download is listed on the Privacy page, like every download you agree to. | The download is listed in [Privacy], like every download you agree to. | Privacy |
| `away.ask.body` | … and the download shows on the Privacy screen like every other. | … and the download shows in [Privacy], like every other. | Privacy |
| `online.off.setUpHint` | You’ll need an account with an online service. Its details go under Advanced details. | You’ll need an account with an online service. | None: the setup's step 2 takes the person there |
| `online.explain.needs.body` | An account with an online service. Its details are added under Advanced details, where Nocturne checks that they work. | An account with an online service. You add its details in step 2, and Nocturne checks that they work. | None |
| `online.service.lead` | Nocturne needs to know which service writes your answers. Its details are added under Advanced details, where Nocturne also checks that they work. | Same words. | None: "Open Advanced details" is on the same screen |
| `online.service.where` | Settings, then Advanced details, then Online answers. | Later, find it in [⚙ Settings], then [Show advanced details], then [Connect an online service]. | Path of three, with the words each control shows: the button says "Show advanced details" and the link says "Connect an online service" |
| `online.send.lead` | Nocturne only sends what each answer needs. You can change these later in Settings. | Nocturne only sends what each answer needs. You can change these later in [⚙ Settings], under [Online answers]. | Settings, Online answers |
| `online.limit.hint` | Then questions are answered on this computer until the 1st. Settings shows how many are left. | Then questions are answered on this computer until the 1st. [⚙ Settings] shows how many are left, under [Online answers]. | Settings, Online answers |
| `online.explain.aside.off` | Turn it off at any time, in Settings or on the Privacy page. | Turn it off at any time in [⚙ Settings] or [Privacy]. | Settings, Privacy |
| `online.consent.mind.rest`, new `online.consent.mind.where` | Turn it off at any time in Settings or on the Privacy page. Every answer says where it was written, and “See what was sent” shows the exact words. | Turn it off at any time in [⚙ Settings] or [Privacy]. (New paragraph.) Every answer says where it was written. [See what was sent] shows the exact words. | Settings, Privacy, See what was sent |
| `online.consent.declined` | Not turned on. To use this service, check it again from Advanced details. | Same words. | None: "Go to Advanced details" is on the same screen |
| `away.relink.how` | On each phone, open Nocturne and choose Scan the code, then point it at your computer. … | On each phone, open Nocturne and choose [Scan the new code], then point it at your computer. … | Scan the new code; fixes a mismatch with the phone's button |
| `away.problem.accountBlocked` | … Try another account, or turn Away from home off. | Same words. | None: on the Away from home screen |
| `phone.welcome.body` | On your computer, open Nocturne, choose Away from home, then Add a phone. Scan the code with this phone, or type it here. | On your computer, open Nocturne and choose [⚙ Settings], then [Away from home], then [Add a phone]. Scan the code with this phone, or type it here. | Path of three; adds the missing first step |
| `phone.scan.hint.pair` | Point at the code on your computer’s screen, under “Add a phone”. | Point at the code on your computer’s screen, under [Add a phone]. | Add a phone |
| `phone.scan.hint.relink` | Point at the code on your computer’s screen. It’s under “Your phones need the new address”. | Point at the code on your computer’s screen. It’s under [Your phones need the new address]. | The rail line of the same words |
| `phone.address.where` | On your computer, open Nocturne and choose “Your phones need the new address”. | On your computer, open Nocturne and choose [Your phones need the new address]. | As above |
| `phone.relink.here.body` | … Open Nocturne from the home screen, choose “Scan the new code”, then point it at your computer. | … Open Nocturne from the home screen, choose [Scan the new code], then point it at your computer. | Scan the new code |
| `phone.check.body` | If they match, choose **Allow** on your computer. If they don’t, choose Don’t allow there and start again. | If they match, choose [Allow] on your computer. If they don’t, choose [Don’t allow] there and start again. | Allow, Don’t allow; both now marked alike |
| `phone.ask.mark.sent` | What was sent is listed on your computer, under Online answers. | What was sent is listed on your computer. Choose [Privacy], then [See what was sent]. | Privacy, See what was sent; the link sits on the Privacy screen, in its Online answers section, so it is a path and not a heading |
| `phone.iphone.step1` to `step3`, `phone.allowed.step1`, `step2` | Tap **Share** … | Same words. | None: Safari's and the browser's own controls stay bold |
| `capture.recording.hint.click` | Press Keep when you are done · long recordings are fine | Same words. | None: Keep is on screen |
| `settings.shortcut.declined` | … Choose Try again, or add a desktop shortcut. | Same words. | None: Try again is on screen |

## Reading rules for setup

These apply to every screen in setup, and are written to be checked by a test or a measurement,
not by eye. The references are the British Dyslexia Association's style guide and WCAG 2.2, criteria
1.4.8 (AAA) and 1.4.12 (AA).

| Rule | Measure | Now |
|---|---|---|
| Line length | Running text capped at 60 characters (`max-width: 60ch`) | The card's text runs to about 83 characters at 15px |
| Line height | At least 1.5 for running text | 1.47 |
| Paragraph spacing | At least 8px inside a box, at least 16px between boxes | Met |
| Sentences per paragraph | At most two | `setup.why.body` has five, `setup.unsupported.disk.detail` four, `setup.unsupported.memory.detail` three |
| Words per sentence | Aim for 15, never more than 20 | `setup.tier.detail.simpler` runs 27 words without a full stop; `setup.tonight.body` has 21 |
| Ideas per sentence | One. At most one condition, and no time and place in the same sentence as a condition | `setup.tiny.online` has three |
| Asides | No parentheses or semicolons | `setup.needs` uses parentheses; `setup.result.standard`, `setup.result.small`, `setup.why.body`, `setup.keep.failed` and `setup.action.failed` use semicolons |
| Lists in a line | At most three facts joined by "·" on one line | Small's and Tiny's cards join four |
| Size of text that informs a choice | At least 15px | The size cards' details are 13px |
| Emphasis | Weight only. No italics, no underlines that are not links, no capitals in sentences. Capitals only in labels of five words or fewer | Met; the step label is the one capitals label |
| Alignment | Left, ragged right, no hyphenation | Met |
| Numbers and units | A no-break space between a number and its unit, so "9 GB" never splits | The Standard card splits "9" and "GB" across lines at the default window size |
| Text spacing override | Nothing clips or overlaps with line height 1.5, paragraph spacing 2em, letter spacing 0.12em and word spacing 0.16em | Not tested |

Most of the copy rules can live in a test beside the existing voice test: count sentences and words
per `setup.*` string, and fail on parentheses, semicolons and more than two "·" in a line. The
others are checks in the setup screen tests.

### Applying them to step 1

These strings are on the same card, and fail the rules above. They are proposals for the same
approval.

| Key | Now | Proposed |
|---|---|---|
| `setup.needs`, new `setup.needs.once` | Nocturne needs {size} of space (you have {free} free). It downloads once, and you can start keeping things straight away. | Nocturne needs {size} of space. This computer has {free} free. It downloads once, and you can keep things straight away. (Two paragraphs: the first two sentences, then the last.) |
| `setup.tier.detail.simpler` | For computers with {ram} GB memory or more · {size} download · uses the least memory, space and battery · answers are simpler and may miss details | Two lines. "{ram} GB memory or more · {size} download" and "Least memory, space and battery. Simpler answers." |
| `setup.tier.detail.slower` | For computers with {ram} GB memory or more · {size} download · slower · uses less memory, space and battery | Two lines. "{ram} GB memory or more · {size} download" and "Slower. Less memory, space and battery." |
| `setup.tier.detail` and `setup.tier.detail.fastest` | For computers with {ram} GB memory or more · {size} download, and the same with "· fastest" | The same first line as the other two cards, "{ram} GB memory or more · {size} download", so the four cards read alike; Large adds the line "Fastest." |
| `setup.unsupported.memory.detail`, `setup.unsupported.disk.detail` | Three and four sentences in one paragraph | Two paragraphs each, two sentences at most: what this computer has and what Nocturne needs, then what happens next |
| `setup.tonight.body` | When this computer is plugged in and idle, Nocturne reads what you kept, links it together and writes your first page. If the lid is closed, it catches up when you open it. | When this computer is plugged in and idle, Nocturne reads what you kept and writes your first page. If the lid is closed, it catches up when you open it. |
| `setup.keep.failed`, `setup.action.failed` | "… Nothing is lost; try again." and "… Nothing was lost; try again in a moment." | The semicolon becomes ", so": "Nothing is lost, so try again." and "Nothing was lost, so try again in a moment." |
| `setup.result.standard` | Nocturne will use a lighter mind. Everything works; tidying takes a little longer. | Nocturne will use a lighter mind. Everything works, and tidying takes a little longer. |
| `setup.result.small` | Nocturne will use its smallest mind. Everything works; answers and tidying are slower. | Nocturne will use its smallest mind. Everything works, but answers and tidying are slower. |

The size cards' details move from 13px to 15px. The mockup leaves the rest of the card as built so
the change to the box is easy to see.

## Not changed

- The headline, the size cards' order and look, and the "Set up · 3.2 GB" button.
- The quiet style of "Why this one?" and its words.
- The app's fonts. No dyslexia font.

## Owner decisions

Agreed by the owner on 2026-11-16:

1. The rail's Settings link gains the gear, so the Settings chip shows an icon the person can find.
2. The reading rules apply beyond setup, starting with Online answers and Away from home.
3. Keys get a key cap, a sibling of the chip, for "Press /" and "Ctrl Shift Space".
4. The setup box stays on step 1, in its shorter form: Tiny is where the question "will answers be
   good enough?" is asked. A note after the first Tiny answer is a separate proposal if wanted.

## Left to the build

- The rail's Privacy link gets no icon: the lock already means "kept private". Its chip has no icon.
- Check the path in `phone.ask.mark.sent` against the screens when the chip is built.
- Back up's "Show advanced details" moves to the right-then-down chevron with this change, so the
  app has one disclosure style. The button there reads "Advanced details", and so does the one on
  Restore from a backup; both take the chevron.
- A chip for "Show advanced details" has no icon: the chevron on the real control shows whether it
  is open, so copying it would teach the wrong shape.
- The key cap is built and drawn in `bar.hint.slash`, which only a screen reader hears today. No
  visible running text names a key yet.

## Review

Approved by the owner on 2026-11-16.
