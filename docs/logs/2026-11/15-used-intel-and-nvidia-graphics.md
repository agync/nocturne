---
type: Log Entry
title: "Use Intel and NVIDIA graphics."
description: "The runtime page and the install guide now say which backend each kind of graphics gets."
tags: [log, update]
kind: Update
status: stable
---

[The runtime page](/architecture/runtime.md) lists what the pinned engine reports for AMD, NVIDIA
and Intel graphics, why Intel graphics are found on the computer, and how a card's own memory is
treated. Intel and NVIDIA graphics now run on Vulkan, as AMD integrated graphics already did. The
[install guide](/guide/install.md#graphics-cards) says which graphics are used.
