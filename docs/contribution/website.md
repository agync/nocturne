---
type: Guide
title: The website
description: How the public landing page and these docs are built into one static site, checked locally, refreshed and deployed with Cloudflare Workers Builds.
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
| `site/assets/` | The stylesheet, the theme switch and tour scripts, the self-hosted fonts, the screenshots and the Open Graph image |
| `site/docs-theme/` | The okf-wiki theme for `/docs/`: the app's colours and faces, and a link back to the landing page |
| `site/_headers` | Response headers, including the content security policies; the build fills in the docs' script hashes |
| `site/wrangler.jsonc` | The Cloudflare Worker `nocturne`: static assets only, serving `site/dist/`, with the empty `previews` block pull request builds need |
| `site/toolchain.sh` | Builds the docs renderer from source for a host with no container runtime |
| `site/brand/` | The sources of the social card and the touch icon; not deployed |
| `site/scripts/` | The build, a local server that applies `_headers`, the checks, the screenshot capture and the brand images |

## Build

```bash
make site                                  # writes site/dist/
make site SITE_URL=https://example.org     # the origin for the canonical and Open Graph links
```

`SITE_URL` defaults to `https://nocturne.is`. `make site` renders `docs/` with okf-wiki, then
copies the landing page and its assets into `site/dist/`, fills in the site origin and the pinned
Lemonade version from `lemonade.lock`, and writes `site/dist/_headers`.

Two renderers give the same output:

| Renderer | Used when |
|---|---|
| The pinned container image (`OKF_WIKI_IMAGE`, okf-wiki v1.2.0, run with podman and no network) | podman is installed |
| The binary in `site/.toolchain/` | podman is missing, which is the case on a Cloudflare build image |

`site/toolchain.sh` fills `site/.toolchain/` (git-ignored). It downloads the okf-wiki v1.2.0
release archive for the host (the release the image carries), checks it against the sha256 values
committed in `site/okf-wiki.sha256`, and unpacks the binary and the Mermaid bundle the docs'
diagrams need. It needs only curl. `make site` runs the script by itself when podman is absent
and the binary is not there yet, so the build command on Cloudflare is just `make site`.
`OKF_WIKI` and `OKF_WIKI_VENDOR` name a different binary and Mermaid bundle. To move the pin,
change `VERSION` in `toolchain.sh`, replace `okf-wiki.sha256` with that release's `checksums.txt`,
and move `OKF_WIKI_IMAGE` to match.

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

The screenshots are the app's own screens on its demo data. The desktop pictures crop to the
column where the page changes, so the reader does not have to spot the difference between two
full windows. The capture also writes the whole window, the fading list and the phone, which the
landing page does not show at the moment. To refresh them, run the mock interface, and for
the phone picture the phone app's scripted build, and capture again:

```bash
cd ui && npx vite --mode mock --port 4173  # in another terminal
cd ui && VITE_ENGINE=phone-mock PHONE_OUT_DIR=/tmp/phone npx vite build --config vite.phone.config.ts
python3 -m http.server -d /tmp/phone --bind 127.0.0.1 4174   # in another terminal
make -C site shots PHONE=http://127.0.0.1:4174   # writes site/assets/img/*.webp in both themes
```

`make shots` prints where the highlighted part of each tour picture sits. When a recapture moves
it, copy the numbers into that step's `data-mark` in `site/index.html`.

On wide screens, when the reader has not asked for reduced motion, `assets/js/tour.js` turns the
Keep, Ask and morning page steps into one still frame beside the text: as each step reaches the
middle of the screen only the page in the frame cross-fades, and a short highlighter pass shows
where to look. Without the script, with reduced motion or on narrow screens, each step keeps its
own picture.

## Brand images

The mark is the app icon, a half moon on a dark tile, and `site/favicon.svg` draws it. Every other
brand image derives from it or from the morning page screenshot, and `make -C site brand` renders
them all again with a headless browser:

| Image | Source | Used by |
|---|---|---|
| `docs/assets/social-preview.png`, 1280 x 640 | `site/brand/card.html` | The repository's social preview |
| `site/assets/img/og.png`, 1200 x 630 | `site/brand/card.html` | The link preview of the landing page, the install page and every docs page |
| `docs/assets/logo.svg` and `logo.png`, 512 x 512 | A copy of `site/favicon.svg` | The README header |
| `site/apple-touch-icon.png`, 180 x 180 | `site/brand/touch-icon.svg`, the icon without rounded corners | Phones and tablets, which round it themselves |
| `site/favicon-32.png` | `site/favicon.svg` | Browsers that cannot show an SVG favicon |

The card sets the landing page's headline and its one-line description on the dark theme, beside
the light morning page with demo data, in the faces the site ships. Its words, its mark and its
address stay inside the central 1100 x 540, clear of the edges GitHub crops on some pages. Change
the card when the landing page's headline changes, and render it again after `make shots`
refreshes the morning page.

GitHub does not read the social preview from the repository: after a change, upload
`docs/assets/social-preview.png` under the repository's Settings, General, Social preview. The docs
theme adds the Open Graph image through its `head` slot (`site/docs-theme/head.html`), and the
build fills in the site origin there as it does on the landing page.

## Deploy

The site is the Worker `nocturne`, static assets only, built by Cloudflare Workers Builds from
this repository. The Worker's name in the dashboard must match `name` in `site/wrangler.jsonc`.
Under the Worker's Settings, Build:

| Setting | Value |
|---|---|
| Production branch | `main` |
| Root directory | `site` |
| Build command | `make site` |
| Deploy command | `npx wrangler deploy` |
| Preview command | `npx wrangler preview` |

The root directory is `site` because it is where the build command runs and where the deploy
command finds `wrangler.jsonc`. The output path is not a dashboard field: the deploy command
takes it from `assets.directory`, so the build only has to finish writing `dist/` first. The
build image has curl and make but no container runtime, which is why it renders with the
binary from `toolchain.sh`.

Pull request builds run `npx wrangler preview`, which serves the branch at a preview address and
never touches production. It reads its own `previews` block and refuses to run without one; every
setting here is top level, so the block is empty.

`nocturne.is` is attached under the Worker's Domains and Routes. `flatpak.nocturne.is` is not
part of this Worker: it is a separate proxied CNAME record that points at GitHub Pages, which
serves the Flatpak repository (see [The Flatpak](flatpak.md)).

A local deploy is `make site` and then `npx wrangler deploy` from `site/`, after `wrangler login`.

## What the page may say

The landing page claims only what the app does today. When a feature, a platform or a release
changes, change the page with it: no download button until a release exists, and Away from home
described as untested on real networks until those trials have run.
