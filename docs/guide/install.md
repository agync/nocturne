---
type: Guide
title: Install and remove
description: Choose a package, set Nocturne up for the first time, and take it off again.
status: draft
tags: [guide, install, packages, setup]
---

# Install and remove

## What your computer needs

Nocturne works on a Linux desktop, on a 64-bit Intel or AMD computer. A Mac package is not published yet. The first time it runs, it checks the memory and free
space and offers the largest setup the computer can carry comfortably. A smaller setup means
slower answers and a slower overnight tidy; everything still works.

| Setup | Memory it needs | Free space it needs | One-off download |
|---|---|---|---|
| Large | 60 GB or more | about 28 GB | about 26 GB |
| Standard | 28 GB or more | about 11 GB | about 8.5 GB |
| Small | 14 GB or more | about 7 GB | about 5 GB |
| Tiny | 6 GB or more | about 5.2 GB | about 3.2 GB |

The free space figure includes 2 GB of room to spare. You can always choose a smaller setup than
the one offered: it uses less memory, space and battery. Tiny is made for 8 GB computers, which
report a little under 8 GB, and it runs on anything that reports 6 GB or more; everything works, but its answers are simpler and it may miss details.
If the computer has less memory than the Tiny setup needs, Nocturne says so plainly and downloads
nothing.

## Choose a package

You are given up to three packages and a list of fingerprints to check them (`SHA256SUMS`). Pick one package.

| Package | Choose it when |
|---|---|
| AppImage | You want to try Nocturne without installing it, or your desktop is not Fedora or Debian based |
| rpm | Your computer runs Fedora, Red Hat, openSUSE or something similar |
| deb | Your computer runs Debian, Ubuntu, Linux Mint or something similar |

Nocturne is not signed. Before you open a package, check it against the list that came with it. Put the package and `SHA256SUMS` in the same folder and run:

```bash
sha256sum --check --ignore-missing SHA256SUMS
```

The list covers all the packages, so `--ignore-missing` skips the ones you did not download. Each file it checks should say `OK`. If one does not, do not open it.

### AppImage

Make the file runnable, then open it:

```bash
chmod +x Nocturne_<version>_amd64.AppImage
./Nocturne_<version>_amd64.AppImage
```

Keep the file where you will find it again. It carries everything it needs, including its own
copy of the parts that run Nocturne's private reading engine.

An AppImage opens by mounting itself, which needs FUSE on your computer (most desktops have it).
If it will not open for that reason, run it this way instead, which unpacks it for the run:

```bash
./Nocturne_<version>_amd64.AppImage --appimage-extract-and-run
```

If you move the AppImage to another folder, turn "Start with this computer" off and on again in
Settings, and copy the shortcut command shown there again, so both point at the new place.

### rpm

```bash
sudo dnf install ./Nocturne-<version>-1.x86_64.rpm
```

Your software centre can open the file too. Nocturne then appears in your applications list.

### deb

```bash
sudo apt install ./Nocturne_<version>_amd64.deb
```

## The first run

Nocturne opens a short setup:

1. It looks at your computer and tells you which setup it will use.
2. It downloads that setup once. It asks before it downloads. This is the only time Nocturne uses
   the internet. You can keep things straight away while it downloads, and the download carries on
   by itself if the connection drops.
3. It tells you when it is ready. Your first page arrives the next morning.

After setup, Nocturne works with no internet at all.

## Take Nocturne off

Removing the app does not remove what you kept. Your things stay in your data folder until you
delete them yourself (see [Your data](your-data.md)).

- AppImage: delete the file.
- rpm: `sudo dnf remove nocturne`
- deb: `sudo apt remove nocturne`

If you turned on "Start with this computer", turn it off in Settings first. Otherwise your desktop
keeps a start-up entry for a program that is no longer there.
