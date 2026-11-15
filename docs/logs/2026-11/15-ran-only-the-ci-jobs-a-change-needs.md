---
type: Log Entry
title: "Ran only the CI jobs a change needs."
description: "CI picks its jobs from the changed paths and caches the Rust dependencies with rust-cache."
tags: [log, update]
kind: Update
status: stable
---

On a pull request, each CI job now runs only when a path it reads changed, so a docs or site change
runs the site and hook jobs and nothing cross-platform. A push to `main` still runs every job, and
no run there is dropped from the queue. The Rust jobs cache their dependencies with `Swatinem/rust-cache`, which
keeps the caches under the repository's limit, and the packaging scripts' tests have a job of their
own. Building lists which paths run each job.
