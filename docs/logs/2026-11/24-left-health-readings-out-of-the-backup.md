---
type: Log Entry
title: "Left health readings out of the backup."
description: "Raw health readings never go into the phone's backup on either platform; a restore reads them again from Health or Health Connect and says what doesn't come back."
tags: [log, update]
kind: Changed
status: stable
---

The owner ruled that raw health readings stay out of the phone's backup on iPhone and Android
alike, in every night, restore and bring in: App Store guideline 5.1.3 bars health data in iCloud,
and Android follows the same rule. Pages, reflections and day lines about health stay. A night
notes only the first day the phone held readings for, and a restore reads them again from Health
or Health Connect back to that day; on Android, older readings are asked for with the other kinds
wherever Health Connect offers them, and the done screen says when only the last 30 days come
back. [Backup on a phone of its own](/design/phone-backup.md#health-readings-after-a-restore)
records it as decision 8, and [0019](/adr/0019-a-locked-copy-online.md) is revised.
