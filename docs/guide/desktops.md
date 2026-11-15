---
type: Guide
title: On your desktop
description: The tray, the quick keep shortcut, notifications and starting with your computer.
status: draft
tags: [guide, desktop, tray, shortcut, notifications]
---

# On your desktop

Nocturne meets your desktop in four places: a small icon in the tray, a quick keep shortcut, one
notification and starting with your computer. Each has a plain fallback when your desktop does not
offer it.

## The tray

When your desktop has a notification area, Nocturne puts a small icon there. It shows what
Nocturne is doing (resting, reading, tidying, needs you, paused) and its menu lets you keep
something, ask, open today's page, pause for an hour, until tonight or until you resume, and quit.
Closing the window leaves Nocturne running in the tray, so the overnight tidy can still happen.

GNOME does not show a tray unless you add an extension that provides one (look for "AppIndicator"
in your extensions app). Without a tray:

- Closing the window quits Nocturne, and nothing is tidied while it is not running. Leave the
  window open or minimised overnight, or add the extension.
- Nothing is lost either way. What you kept is safe, and Nocturne catches up on the next start.

Most other desktops, such as KDE Plasma, show the tray without any extra step.

## Quick keep shortcut

The default shortcut is Ctrl+Shift+Space. It opens a small box above whatever you are doing, where
you can type, paste, drop a file or hold Space in the empty box to talk. Press Esc and it goes away. You can change
the keys in Settings.

On most current desktops (those that use Wayland, which is the default on current GNOME and KDE),
an app cannot listen for keys while you are in another app. If yours is one, Nocturne says so in
its Settings and shows a command instead. Add it as a custom shortcut:

1. Open your desktop's own settings (not Nocturne's), go to Keyboard, then Custom Shortcuts.
2. Add a shortcut, paste the command Nocturne shows under "Set it to run", and choose your keys.

On older desktops (those that use X11), the Ctrl+Shift+Space shortcut works directly.

## Notifications

When your morning page is ready, Nocturne can send one quiet note: the page is ready, and how many
things need you. It says nothing about what you kept. It arrives from your page time until six hours later, never after 9 pm, not if you have already
opened the page, and not while Nocturne is paused. In Settings, choose "Tell me when it's
ready" or "Don't interrupt me". If your desktop blocks notifications for Nocturne, the page is
still there when you open the app and the tray icon shows when something needs you.

## Start with this computer

Turn this on in Settings so the overnight tidy can run without you opening Nocturne. It adds a
start-up entry for you, and Nocturne then starts in the tray without opening a window. On a desktop
with no tray, it opens the window instead, so you can see it is running.

## A folder that keeps what you drop in it

In Settings you can choose an inbox folder. Anything saved there afterwards is kept once. Nocturne
only reads the folder: it never moves, changes or deletes what is in it. It reads subfolders up
to four levels down and skips hidden files and folders. Files already there when
you choose the folder are left alone.

## Sound

To keep a voice note, Nocturne needs permission to use the microphone. If it is switched off,
Nocturne says so and offers to open your sound settings. You can still type, or drop a recording
onto the window.

If a window cannot record at all, Nocturne says so once and marks the microphone button as
unavailable, with the reason, so pressing it again does nothing. Advanced details shows whether
recording is available.
