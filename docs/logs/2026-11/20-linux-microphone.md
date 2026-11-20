---
type: Log Entry
title: "Let the app's own pages use the microphone on Linux."
description: "WebKitGTK refuses the microphone unless the app answers its request; the web view now allows the app's own pages to record on Linux."
tags: [log, update]
kind: Changed
status: stable
---

Voice notes failed on Linux with "Nocturne can’t hear you. Microphone access is turned off for
it.", in the Flatpak and in every other Linux build, although the microphone was on and the
Flatpak already carried `--socket=pulseaudio`. The interface never reached the desktop: on Linux
the app draws its screens in WebKitGTK, and WebKitGTK asks the app, not the system, whether a page
may have the microphone. A request the app does not answer is refused, so the request came back as
`NotAllowedError`, the same answer a real refusal gives, and the app read it as the person having
turned the microphone off.

`src-tauri/src/os/media.rs` now answers that request through Tauri's own permission hook, so every
window is covered, the quick capture window included. A page from the app's own origin may record
without a question, since holding Space or the record button is the consent, while a camera
request, and any page from anywhere else, is refused. macOS and Windows are untouched: their web
views ask the system, which asks the person.

Checked in a container against a real WebKitGTK, with a virtual display and a private sound source:
a page on the app's origin recorded one audio track, the same page on another origin was refused,
and a window without the answer was refused as before. The Flatpak itself was not rebuilt: it
already carries the sound socket.
