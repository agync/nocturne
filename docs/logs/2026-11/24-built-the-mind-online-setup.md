---
type: Log Entry
title: "Built the online mind setup's three fixes."
description: "The step's bar is pinned on every phone step, the model name can be chosen from the service's own list on the phone and the computer, and reading on battery says it waits and offers Read now."
tags: [log, update]
kind: Changed
status: stable
---

[Setting up Nocturne's mind online](/design/mind-online-setup.md) is built, with the owner's six
decisions.

- The phone's step bar pins to the bottom of the screen while it takes no more than a third of it
  and the keyboard is down, and fades the words above it while more follow.
- Step 2 on the phone and Connect on the computer look at the service's list when the person taps
  for it, show the names A to Z with search past eight, and warn about a typed name that is not
  on it. A name not on the list no longer stops the check: its question decides.
- This phone and "Reading what you kept" say "Waiting to charge" on battery, and offer Read now,
  Stop and Read anyway. Read now stops at 20% or with battery saving on, Read anyway at 5%, and
  both at every other stop, each told with why. The page still waits for the charger.
- On Android, Read now runs as a data sync foreground service with its own quiet notification and
  Stop. On iPhone it uses iPhone's own progress from iOS 26, and the short time iPhone gives
  before that, with one "Reading paused" line.

What the build reads differently is under [As built](/design/mind-online-setup.md#as-built).
