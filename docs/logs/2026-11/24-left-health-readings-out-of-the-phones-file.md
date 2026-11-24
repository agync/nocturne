---
type: Log Entry
title: "Left health readings out of the phone's backup file."
description: "A phone's backup file leaves raw health readings out as its nightly backup does, a restore from a file reads them again, and a reading read again keeps its id."
tags: [log, update]
kind: Changed
status: stable
---

Health readings are left out of a phone's backup file too, as the owner ruled for backups in
general. Restoring a file on a phone, also an older one or a computer's, leaves any readings in it
out and reads them again from Health or Health Connect back to the first day they began. A reading
read again gets the id it had, so pages and reflections that cite it find it again. See
[Backups to a file](/design/phone-first.md#backups-to-a-file) and
[Health readings after a restore](/design/phone-backup.md#health-readings-after-a-restore).
