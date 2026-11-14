---
type: Log Entry
title: "Allowed providers on your own network."
description: "Online answers accept a server on this computer or the person's own network, over plain HTTP there, and the preview runs Online answers and choosing models."
tags: [log, update]
kind: Update
status: stable
---

Online answers now take a provider on this computer or the person's own network (loopback, private, shared and unique-local addresses, and local names such as `.local`, `.ts.net` and one-label names), over `https://` or plain `http://`, with no warning. Anywhere else stays `https://` only. Addresses that are never a provider (unspecified, link-local and metadata, multicast, reserved, documentation ranges) and Nocturne's own ports stay refused. The decision, architecture and guide pages and invariant 2 say so, and the building page says the preview now runs Online answers and choosing models.
