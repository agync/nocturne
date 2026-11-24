---
type: Log Entry
title: "Tied a backup deletion to its account."
description: "A deletion of the backup owed to an account the phone couldn't reach stays with that account when the person chooses another, and runs when Nocturne next reaches it."
tags: [log, update]
kind: Changed
status: stable
---

Delete everything while the phone is offline, then another account chosen before it is back
online: the deletion stays tied to the account the backup was in and runs the next time Nocturne
reaches that account. The account chosen since never receives it, and the change no longer drops
it. [Backup on a phone of its own](/design/phone-backup.md#decisions) records it as decision 7. On
the phone, each deletion owed is noted with its account, and a change of account and a deletion
never run at once.
