---
type: Design
title: The buttons in the keep or ask bar
description: Proposal to settle the add-a-file and hold-to-talk buttons inside the keep or ask bar, with measurements of what is off, two options with before and after captures, and a recommendation.
status: draft
tags: [proposal, ux, accessibility, desktop, layout, copy]
---

# The buttons in the keep or ask bar

The owner says the add-a-file and hold-to-talk buttons in the keep or ask bar "bug me somehow". This
page measures what is off, shows two ways to fix it and recommends one. Nothing here is built yet.

## What is off

Measured in the app at 1280 by 800, 100% text, from the live layout.

| What | Now | Why it reads wrong |
|---|---|---|
| Room around the round buttons | The field is 48 px tall with a 1 px border, so 46 px inside. The buttons are 44 px. **1 px** above and below, **6 px** to the right | Two outlines 1 px apart: the circles look jammed into the field, and the space around them is uneven |
| At 200% text | Field 90 px, buttons 88 px: **0 px** above and below. The icon stays 18 px inside an 88 px circle | The circles touch the field's border and the icon looks lost |
| Shapes inside the field | Field: 8 px corners. Shortcut: 3 px corners (not a token). Buttons: circles. Keep and Ask, once you type: 4 px corners, 32 px tall | Three corner styles and two button heights in one row; typing puts 32 px rectangles beside 44 px circles |
| Outlines | Field, shortcut and both buttons all have the same 1 px outline in the same colour | Four frames in one row; the shortcut, which you cannot press, looks like a button |
| Spacing | Gaps of 10 px (not a token), 16 px on the left, 6 px on the right | No rhythm; the right end is tighter than the left |
| Hover and pressed | No change at all | Nothing confirms the pointer is on a button |
| Keyboard focus | The 2 px ring sits 2 px outside the circle, 1 px from the field's border | The ring and the field's border nearly touch |
| The line beside the bar | "or drop files anywhere" starts with a lower-case "or" and sits outside the field | Seen on its own, or cut off at the window's edge, it reads as a stray "or" |
| Medium windows | At 900 px the line takes 129 px and the field shrinks to 467 px; at 760 px the words in the empty field are cut to "Keep son" | The hint and the shortcut crowd out what the field says |
| Icon centring | Paperclip drawing is 0.3 px right of centre at 18 px; the microphone is centred | Not the problem: both icons are centred within half a pixel |

The phone apps, alone and paired, have no such buttons. A phone's ask box holds only its text and
an Ask button, and Keep offers Take photo, Choose photo and Record as labelled tiles
(`phone-alone-keep.webp`, `phone-companion-ask.webp`). This proposal changes the bar on the
computer at every window size, including the narrow window where it sits at the bottom.

## Changes in both options

| Change | Detail |
|---|---|
| Even room around the buttons | Buttons 2 rem (32 px), the same height as Keep and Ask. The field grows with the text (3 rem, 48 px at 100%). The right padding equals the room above and below: **7 px** at 100%, **15 px** at 200% |
| One gap | 12 px (`--s-3`) between the text, the shortcut, the buttons and Keep and Ask |
| One corner | The buttons take 4 px corners (`--r-sm`), as Keep and Ask do; the shortcut takes 2 px (`--r-xs`) |
| A quieter shortcut | The shortcut keeps its frame, in the faint rule colour, so it reads as keys and not as a button. It hides when the field is narrower than 34 rem, so the words in the field are never cut |
| Icons grow with the text | 1.125 rem (18 px at 100%, 36 px at 200%) |
| Touch targets | On a narrow window each button keeps a 44 px target through the shared `hit` pattern; the 12 px gap keeps the two targets from overlapping |
| The line beside the bar | Reads "Drop files anywhere to keep them", never wraps, and shows only when the top bar has room for it beside the full-width field (60 rem and wider). Below that, dropping still works and still shows "Drop to keep" |

## Option A: quiet icons (recommended)

The field is the only frame. The two buttons are icons with no outline, a shade lighter than the
text at rest.

| State | Look |
|---|---|
| Rest | Icon in the second ink colour, no outline, no fill |
| Hover | A faint fill (ink at 8%), icon in full ink |
| Pressed | Ink at 14% |
| Keyboard focus | The usual 2 px focus ring on the button's own edge, icon in full ink |
| Recording | Ember icon on the soft ember fill, beside "Listening 0:01" |
| Cannot talk here | Icon in the third ink colour, the not-allowed pointer; the reason stays in the tooltip and is read out, as now |

## Option B: squared to the verbs

The buttons keep an outline but become 32 px squares with 4 px corners, the same shape and size as
the Ask button.

| State | Look |
|---|---|
| Rest | 1 px outline in the third ink colour, icon in full ink |
| Hover | Outline in full ink, ink at 6% |
| Pressed | Ink at 12% |
| Keyboard focus | The usual focus ring, 1 px outside the outline |
| Recording | Ember outline, soft ember fill, ember icon |
| Cannot talk here | Dashed outline, as now |

## Recommendation

Option A. The owner's complaint is that the buttons look stuck on; A removes the second and third
frames, so the field reads as one calm object and the only outlined things inside it are the ones
that change what happens next (Keep and Ask once there is text). B fixes the size and spacing but
still shows four outlines in a row when the bar is empty.

## Before and after

Every image is captured from the app in mock mode (`npm run dev` in `ui/`) in a headless browser at
twice the pixel density. "Before" is the app as it is; "A" and "B" are the same app with each
option's styles applied. Files are in `docs/design/mockups/keep-ask-buttons/`, named
`{before,a,b}-<view>.webp`.

| View | Window | File suffix |
|---|---|---|
| Empty, dark | 1280 by 800 | `desktop-dark` |
| Empty, light | 1280 by 800 | `desktop-light` |
| Pointer on add a file | 1280 by 800 | `hover` |
| Keyboard focus on add a file | 1280 by 800 | `focus` |
| Holding to talk | 1280 by 800 | `recording` |
| Text typed, Keep and Ask showing | 1280 by 800 | `typed` |
| Medium window | 900 by 600 | `medium` |
| Narrow window, dark | 390 by 844 | `narrow-dark` |
| Narrow window, light | 390 by 844 | `narrow-light` |
| Narrow window, holding to talk | 390 by 844 | `narrow-recording` |
| Text at 200% | 1280 by 800 | `text200` |

## Copy

| Key | Now | Proposed |
|---|---|---|
| `bar.dropHint` | or drop files anywhere | Drop files anywhere to keep them |

## Questions for the owner

| Question | Options | Recommendation |
|---|---|---|
| Which option | A, quiet icons; B, squared to the verbs | A |
| The line beside the bar | The new words above; keep the old words; drop the line, since dropping shows its own "Drop to keep" | The new words |
