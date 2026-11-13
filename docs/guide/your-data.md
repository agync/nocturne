---
type: Guide
title: Your data
description: Where everything is kept, how to export, let go or delete it, and how Nocturne shows that nothing leaves your computer.
status: draft
tags: [guide, privacy, data, export, delete]
---

# Your data

Everything Nocturne holds is on your computer, in one folder, and you can see all of it.

## Where it is

| System | Folder |
|---|---|
| Linux | `~/.local/share/app.nocturne.desktop/` |
| Windows | `%APPDATA%\app.nocturne.desktop\` |

Inside are the things you kept and what Nocturne remembers from them (in `store`), Nocturne's
reading setup (in `engine`) and a few small log files (in `logs`) that hold counts and error
kinds, never what you kept. Privacy in the app has an "Open folder" button that takes you there.

## Look at what it holds

- Today and Past pages show the morning pages.
- What Nocturne remembers lists everything it has learned, and every line leads back to the photo,
  recording or note it came from.
- Recently kept lists what you kept, and what Nocturne made of it.

## Take a copy

Privacy has "Export everything". It writes a folder of readable files: every memory as text, every
page, and all your originals exactly as you kept them. Nothing is changed or removed by exporting.

## Let go, bring back, forget

Nocturne does not delete anything quietly.

- Things you have not used fade after 30 days and are let go after 90. "Fading & let go" lists
  them, and one tap brings any of them back for another 30 days.
- "Forget completely" in that list is the only way a single thing is removed, and it asks first.
- Unconfirmed things from the web are never treated as true until you say so.

## Delete everything

Privacy has "Delete everything". It removes everything Nocturne holds from this computer: what you
kept, what it remembers and every page. You have to type the words it shows before it goes ahead,
and it cannot be undone, so export first if you might want a copy. Afterwards Nocturne starts again
from setup.

## How you can tell nothing leaves your computer

Nocturne has no account, no analytics and no crash reports. Its reading engine is a private one that
listens only on your own computer's internal address, with a fresh key every time Nocturne starts.

You do not have to take that on trust:

- Privacy lists everything Nocturne ever downloaded, with the day and the size. Setup and any
  update you allowed are the only entries.
- Privacy and the page footer say "Sent to the internet: Nothing" when Nocturne can measure it. On
  Linux it cannot count what a program sends, so the app says "Not measured on this computer" and
  shows the list of downloads instead.
- Before each release, the people who build Nocturne run it with its network cut to the
  computer's own internal address, and check that every attempt to reach anywhere else is a
  failure. The check also confirms that Nocturne still starts, shows its window, runs its reading
  engine and keeps a file in that state. The result goes in the release notes.
- You can run the same sort of check yourself: switch off your internet and use Nocturne. Nothing
  changes.
