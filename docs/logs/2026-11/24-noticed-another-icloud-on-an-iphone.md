---
type: Log Entry
title: "Noticed another iCloud on an iPhone."
description: "An iPhone signed in to another iCloud counts as another backup account, as another Google account does on Android, so a deletion owed to the first never reaches the second."
tags: [log, update]
kind: Changed
status: stable
---

An iPhone now notes an opaque mark of the iCloud account it is signed in to and looks at it before
each night and each deletion. Another iCloud signed in is another account, as another Google
account is on Android: the backup starts afresh there, and a deletion owed to the earlier iCloud
waits until the iPhone is signed in to it again. See
[States in This phone](/design/phone-backup.md#states-in-this-phone).
