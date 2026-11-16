---
type: Design
title: Quick keep shortcut through the desktop, copy
description: The approved words for the quick keep shortcut where the desktop holds it (Wayland, the Flatpak).
status: stable
tags: [approved, copy, settings, shortcut, wayland, flatpak]
---

# Quick keep shortcut through the desktop, copy

On a Wayland desktop, and so in the Flatpak on one, an app cannot listen for keys while another
app is in front. The app now asks the desktop to hold the quick keep shortcut through its global
shortcuts portal, where the desktop has one (GNOME 48 and later, KDE Plasma 5.27 and later). The
desktop may ask the person first in its own window, and it may choose other keys than the ones
suggested. Where the desktop has no such portal, Settings keeps today's words and the command for a desktop
shortcut.

The owner approved the proposed words below as written on 2026-11-16, and they are in the
interface. Each table keeps the words from before for the record.

## 1. The shortcut's name in the desktop's window

The desktop shows this name when it asks the person, and in its own keyboard settings.

| Now | Proposed |
|---|---|
| Quick keep shortcut | Keep something |

"Keep something" is already the heading of the small box the shortcut opens, so the name says
what pressing it does. The desktop shows the app's name next to it.

## 2. Settings, when the desktop holds the shortcut

Today the row shows the saved keys and "Change". The desktop may have bound other keys, and on
GNOME pressing new keys in Nocturne does not change what the desktop holds.

| Place | Now | Proposed |
|---|---|---|
| Keys shown | the saved keys | the keys the desktop reports, for example "Ctrl+Shift+Space" |
| Hint | Works from any app | Works from any app (unchanged) |
| Note under the row | none | Your desktop looks after this shortcut. |
| Button | Change | Change in desktop settings |

The button opens the desktop's own shortcut settings where the portal offers that (version 2);
where it does not, the button is not shown and the note reads "Your desktop looks after this
shortcut. Change it in your desktop's keyboard settings."

## 3. When the person says no in the desktop's window

| Now | Proposed |
|---|---|
| This shortcut isn't working here. Another app may be using it, or this desktop doesn't share keys with other apps. Choose another, or add a desktop shortcut. | The shortcut wasn't added. Choose Try again, or add a desktop shortcut. |

A "Try again" button asks the desktop again. "Set it to run:" and the command stay as they are.

## 3a. While the desktop is asking

Added after the first test on GNOME, where the desktop's window stayed open for minutes and the
row showed only the keys. Approved by the owner on 2026-11-16.

| Place | Now | Approved |
|---|---|---|
| Note under the row | none | Your desktop is asking whether to add this shortcut. |

No button shows while the desktop is asking.

## 4. The guide's paragraph (Desktops, Quick keep shortcut)

Proposed replacement for the paragraph that starts "On most current desktops":

> On most current desktops (those that use Wayland, which is the default on current GNOME and
> KDE), your desktop looks after the shortcut. The first time Nocturne starts, your desktop may
> ask whether to add it; allow it. To change the keys later, use your desktop's keyboard
> settings. If your desktop can't add it, Nocturne says so in its Settings and shows a command
> instead. Add it as a custom shortcut:

The numbered steps after it stay.

## 5. When "Change in desktop settings" does not open them

Today a failure is silent: the button does nothing visible. In the same place as the note under
the row, replacing it until the next time Settings is opened:

| Place | Now | Approved |
|---|---|---|
| Note under the row | Your desktop looks after this shortcut. | Your desktop's shortcut settings didn't open. Change the keys in your desktop's keyboard settings instead. |

The button stays, so the person can try again.

## Open questions

- Whether "Change in desktop settings" fits the row at the narrowest window width.
- Whether the note in section 2 is needed when the keys shown already come from the desktop.
