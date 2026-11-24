---
type: Log Entry
title: "Worded a recovery key the account did not take."
description: "A recovery key opens the backup only once it is saved and the account has taken it; the backup design now says what shows when the account can't be reached then, and that Make a recovery key waits while the phrase is needed again."
tags: [log, update]
kind: Changed
status: stable
---

In [Backup on a phone of its own](/design/phone-backup.md), a recovery key opens the backup only
once the person saved it and the account took it, so "Cancel" always leaves an earlier key working.
When the account can't be reached at that moment, the key screen says the key is saved but doesn't
open the backup yet, with "Try again". While a new account waits for the phrase to be set again,
This phone does not offer "Make a recovery key…".
