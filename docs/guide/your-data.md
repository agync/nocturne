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
| Mac | `~/Library/Application Support/app.nocturne.desktop/` |

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
page, and all your originals exactly as you kept them. If you have used Online answers, it also holds
the Sent online log as readable text and your model choices (as `online/sent.md` and
`models.json`); it never holds the key. Nothing is changed or removed by exporting.

## Let go, bring back, forget

Nocturne does not delete anything quietly.

- Things you have not used fade after 30 days and are let go after 90. That is the Normal
  setting. Change it under Letting go in Settings. "Fading & let go" lists them, and one tap
  brings any of them back for another 30 days.
- Forget removes a thing for good, and it asks first. You find it on a memory, on something you
  kept, on an answer, and in Fading & let go.
- Forgetting a memory keeps the original and its transcript. Tick "Also delete the original" to
  remove that too. Anything you exported or printed is out of Nocturne's reach.
- Unconfirmed things from the web are never treated as true until you say so.

## Delete everything

Privacy has "Delete everything". It removes everything Nocturne holds from this computer: what you
kept, what it remembers and every page. You have to type the words it shows before it goes ahead,
and it cannot be undone, so export first if you might want a copy. It keeps the reading setup it
downloaded and the log files (the Sent online log, the key and your model choices are removed),
turns off "Start with this computer" and resets the quick keep shortcut. It also clears what the Nocturne window keeps for itself outside that folder: on a Mac,
the folders named `app.nocturne.desktop` in `~/Library/WebKit` and `~/Library/Caches`, and the saved
window state; on Linux, the window's storage inside the data folder and `~/.cache/app.nocturne.desktop`.
Afterwards Nocturne starts again from setup.

## How you can tell nothing leaves your computer

Nocturne has no account, no analytics and no crash reports. Its reading engine is a private one that
listens only on your own computer's internal address, with a fresh key every time Nocturne starts.

Away from home and Online answers are the two exceptions, and each is off until you turn it on.
While Online answers is on, your private engine sends a question and the few remembered sentences it
needs to the one address you entered, and the Sent online log lists exactly what left
([Online answers](online-answers.md)). While Away from home is on, sealed
traffic goes to your own paired phones through the connection you chose, and that connection's helper
talks to its company. Privacy then shows what was sent to your phones, and
[Away from home](away-from-home.md) lists what each company can see.

You do not have to take that on trust:

- Privacy lists everything Nocturne ever downloaded, with the day and the size. Setup and any
  update you allowed are the only entries.
- Nocturne only goes online to download what you agree to. It does not measure what a program
  sends while it runs, so it does not claim a figure for that.
- Each release is checked to send nothing else. The people who build Nocturne run it with its
  network cut to the computer's own internal address, and treat every attempt to reach anywhere
  else as a failure. The check also confirms that Nocturne still starts, shows its window, runs its reading
  engine and keeps a file in that state. The result goes in the release notes.
- You can run the same sort of check yourself: switch off your internet and use Nocturne. Nothing
  changes.
