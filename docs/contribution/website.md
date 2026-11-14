---
type: Guide
title: The website
description: How the public landing page and these docs are built into one static site for Cloudflare, checked locally and refreshed.
status: draft
tags: [contribution, website, docs, cloudflare]
---

# The website

The public site is one directory of static files: a hand-written landing page and this docs
bundle rendered under `/docs/`. Everything lives in `site/`, and nothing is deployed from this
repository by a command. Publishing is the owner's decision.

| Path | What it is |
|---|---|
| `site/index.html`, `site/404.html` | The landing page and the not-found page, plain HTML with no inline script or style |
| `site/assets/` | The stylesheet, the theme switch and tour scripts, the self-hosted fonts and the screenshots |
| `site/docs-theme/` | The okf-wiki theme for `/docs/`: the app's colours and faces, and a link back to the landing page |
| `site/_headers` | Response headers, including the content security policies; the build fills in the docs' script hashes |
| `site/wrangler.jsonc` | A Cloudflare Worker with static assets only, serving `site/dist/` |
| `site/scripts/` | The build, a local server that applies `_headers`, the checks and the screenshot capture |

## Build

```bash
make site                                  # writes site/dist/
make site SITE_URL=https://example.org     # the origin for the canonical and Open Graph links
```

`make site` renders `docs/` with the pinned okf-wiki container image (podman, no network), then
copies the landing page and its assets into `site/dist/`, fills in the site origin and the pinned
Lemonade version from `lemonade.lock`, and writes `site/dist/_headers`. Set `OKF_WIKI` to a local
okf-wiki binary (and `OKF_WIKI_VENDOR` to its Mermaid bundle) to render without podman.

## Check

```bash
make site/serve                            # http://127.0.0.1:4180 with the headers applied
make -C site check                         # axe, policy errors, 360 px, keyboard, screenshots
```

The check runs axe on the landing page and on docs pages in both themes at 1440 and 390 px, fails
on any console or policy error and on sideways scrolling, tabs to the theme buttons, and confirms
the theme chosen on the landing page carries into the docs. A few findings come from markup the
renderer writes itself (linked section titles in the sidebar, wide tables, the search button);
they are reported as warnings because the theme cannot change them.

## Screenshots

The screenshots are the app's own screens on its demo data. The whole window appears once; the
other desktop pictures crop to the column where the page changes, so the reader does not have to
spot the difference between two full windows. To refresh them, run the mock interface, and for
the phone picture the phone app's scripted build, and capture again:

```bash
cd ui && npx vite --mode mock --port 4173  # in another terminal
cd ui && VITE_ENGINE=phone-mock PHONE_OUT_DIR=/tmp/phone npx vite build --config vite.phone.config.ts
python3 -m http.server -d /tmp/phone --bind 127.0.0.1 4174   # in another terminal
make -C site shots PHONE=http://127.0.0.1:4174   # writes site/assets/img/*.webp in both themes
node site/scripts/og.mjs http://127.0.0.1:4180   # the Open Graph image, from the served site
```

`make shots` prints where the highlighted part of each tour picture sits. When a recapture moves
it, copy the numbers into that step's `data-mark` in `site/index.html`.

On wide screens, when the reader has not asked for reduced motion, `assets/js/tour.js` turns the
Keep, Ask and morning page steps into one still frame beside the text: as each step reaches the
middle of the screen only the page in the frame cross-fades, and a short highlighter pass shows
where to look. Without the script, with reduced motion or on narrow screens, each step keeps its
own picture.

## Deploy

Deployment needs, from the owner: a Cloudflare account, a Worker name (`name` in
`site/wrangler.jsonc`), the domain to attach and to pass as `SITE_URL`, and an API token if a CI
job deploys. With those, `make site` and then `npx wrangler deploy` from `site/` publish
`site/dist/`. Cloudflare's own build runners have no podman, so build where podman or an okf-wiki
binary is available.

## What the page may say

The landing page claims only what the app does today. When a feature, a platform or a release
changes, change the page with it: no download button until a release exists, and Away from home
described as untested on real networks until those trials have run.
