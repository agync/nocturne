---
type: Guide
title: Install and remove
description: Choose a package for your Mac, Windows or Linux computer, open it past the first-time warning, set Nocturne up and take it off again, or run it as a server in a container.
status: draft
tags: [guide, install, packages, setup]
---

# Install and remove

## What your computer needs

Nocturne works on:

- a Mac with an Apple chip (M1 or later), running macOS 26 or later. Older Intel Macs are not supported;
- a Windows 10 or 11 computer, 64-bit;
- a Linux desktop, on a 64-bit Intel or AMD computer.

The first time it runs, it checks the memory and free
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

Download the one file for your computer from the [latest release](https://github.com/agync/nocturne/releases/latest). Each has a fixed name, so these links always give the newest version:

| Computer | File | Choose it when |
|---|---|---|
| Mac | [Nocturne-macos-arm64.dmg](https://github.com/agync/nocturne/releases/latest/download/Nocturne-macos-arm64.dmg) | You have a Mac with an Apple chip |
| Windows | [Nocturne-windows-x64-setup.exe](https://github.com/agync/nocturne/releases/latest/download/Nocturne-windows-x64-setup.exe) | You use Windows at home. This is the usual choice |
| Windows | [Nocturne-windows-x64.msi](https://github.com/agync/nocturne/releases/latest/download/Nocturne-windows-x64.msi) | Your computer is managed by an organisation that installs `.msi` files |
| Linux | [Nocturne-linux-x86_64.AppImage](https://github.com/agync/nocturne/releases/latest/download/Nocturne-linux-x86_64.AppImage) | You want to try Nocturne without installing it, or your desktop is not Fedora or Debian based |
| Linux | [Nocturne-linux-x86_64.rpm](https://github.com/agync/nocturne/releases/latest/download/Nocturne-linux-x86_64.rpm) | Your computer runs Fedora, Red Hat, openSUSE or something similar |
| Linux | [Nocturne-linux-amd64.deb](https://github.com/agync/nocturne/releases/latest/download/Nocturne-linux-amd64.deb) | Your computer runs Debian, Ubuntu, Linux Mint or something similar |
| Linux on Arm | [Nocturne-linux-aarch64.AppImage](https://github.com/agync/nocturne/releases/latest/download/Nocturne-linux-aarch64.AppImage) | As the AppImage above, on a computer with an Arm chip |
| Linux on Arm | [Nocturne-linux-aarch64.rpm](https://github.com/agync/nocturne/releases/latest/download/Nocturne-linux-aarch64.rpm) | As the rpm above, on a computer with an Arm chip |
| Linux on Arm | [Nocturne-linux-arm64.deb](https://github.com/agync/nocturne/releases/latest/download/Nocturne-linux-arm64.deb) | As the deb above, on a computer with an Arm chip |
| Linux | Flatpak | You want your software centre to keep Nocturne up to date, on any Linux desktop, Arm included |

Most Linux computers have an Intel or AMD chip and take the plain Linux files. If yours has an Arm
chip, take the Linux on Arm files instead. To check, run `uname -m` in a terminal: `x86_64` means
the plain files, `aarch64` means the Arm ones. The Flatpak picks the right one by itself.

The release also has a list of fingerprints to check the files against (`SHA256SUMS`). The release
page names each file with its version number as well; those are the same files.

Nocturne is not signed by Apple or Microsoft, because each charges a yearly fee. Your Mac or
Windows computer therefore warns you the first time, and the next section shows what to click. The
Linux Flatpak is signed with Nocturne's own key. To be sure a download is the one we published,
check it against `SHA256SUMS` before you open it. Put the file and `SHA256SUMS` in the same folder and run:

```bash
sha256sum --check --ignore-missing SHA256SUMS
```

On a Mac, run `shasum -a 256 --check --ignore-missing SHA256SUMS` instead. On Windows, run
`Get-FileHash <file>` in PowerShell and compare the result with the line for that file in
`SHA256SUMS`. The list covers every file, so `--ignore-missing` skips the ones you did not
download. Each file it checks should say `OK`. If one does not, do not open it.

## The first time you open it

### On a Mac

1. Open the downloaded `.dmg` file, then drag Nocturne into Applications.
2. Open Nocturne from Applications. Your Mac says "Nocturne" Not Opened, because it could not check
   the app. Click Done.
3. Open System Settings, then Privacy & Security. Scroll down to the line saying Nocturne was
   blocked and click Open Anyway.
4. Enter your password, then click Open Anyway again.

You only do this once. After that, Nocturne opens like any other app. On macOS before Sequoia
(version 15), Control-click Nocturne in Applications and choose Open instead.

### On Windows

1. Your browser may warn that the download is not commonly downloaded. In Edge, open the `...` menu
   beside the file, choose Keep, then Show more, then Keep anyway.
2. Open the installer. Windows shows "Windows protected your PC". Click More info, then Run anyway.

You only do this once per download. If you see no warning, there is nothing to click.

### On Linux

There is no warning. See the notes for your package below.

## Linux packages

### AppImage

Make the file runnable, then open it:

```bash
chmod +x Nocturne-linux-x86_64.AppImage
./Nocturne-linux-x86_64.AppImage
```

On a computer with an Arm chip, the file is `Nocturne-linux-aarch64.AppImage`; use that name in
these commands. Keep the file where you will find it again. It carries everything it needs, including its own
copy of the parts that run Nocturne's private reading engine.

An AppImage opens by mounting itself, which needs FUSE on your computer (most desktops have it).
If it will not open for that reason, run it this way instead, which unpacks it for the run:

```bash
./Nocturne-linux-x86_64.AppImage --appimage-extract-and-run
```

If you move the AppImage to another folder, turn "Start with this computer" off and on again in
Settings, and copy the shortcut command shown there again, so both point at the new place.

### rpm

```bash
sudo dnf install ./Nocturne-linux-x86_64.rpm
```

On a computer with an Arm chip, install `Nocturne-linux-aarch64.rpm` the same way.

Your software centre can open the file too. Nocturne then appears in your applications list.

### deb

```bash
sudo apt install ./Nocturne-linux-amd64.deb
```

On a computer with an Arm chip, install `Nocturne-linux-arm64.deb` the same way.

### Flatpak

Add Nocturne's repository from [flatpak.nocturne.is](https://flatpak.nocturne.is), then install
the app. The page has a button that does both, or run:

```bash
flatpak remote-add --user --if-not-exists nocturne https://flatpak.nocturne.is/nocturne.flatpakrepo
flatpak install --user nocturne is.nocturne.Nocturne
```

Your software centre then shows Nocturne and its updates. The Flatpak keeps everything it holds in
its own folder and sees only the files you choose or drop on it. Starting with your computer is not
available in the Flatpak yet.

## Run it as a server

This part is for technical people, and the server is experimental. Instead of a desktop app,
Nocturne runs in a container on a machine that is always on, and you use it from a browser. It is
the same app with the same private engine, and it keeps everything in one volume. What needs a
desktop (the tray, starting with the computer, notifications, printing, choosing folders, Away from
home) is not there. [Decision 0012](/adr/0012-server-mode.md) has the details.

Releases publish the image as `ghcr.io/agync/nocturne`. Until one has, build it yourself with
`make server/image` ([building](/contribution/building.md#the-server-image)) and use
`localhost/nocturne:dev` in place of the name below.

```bash
podman run -d --name nocturne \
  -p 127.0.0.1:55066:55066 \
  -v nocturne-data:/data \
  ghcr.io/agync/nocturne:latest
podman exec nocturne nocturne-server token     # the sign-in link
```

Docker takes the same flags (`docker run ...`, `docker exec ...`). Open the link it prints,
`http://127.0.0.1:55066/#token=...`, in a new tab, once in each browser you use; the browser keeps
you signed in. The link is never written to the container's log. To publish another port on this
machine, say so with `-e NOCTURNE_PUBLIC_ORIGIN=http://127.0.0.1:<port>`.

| Command | What it does |
|---|---|
| `podman exec nocturne nocturne-server token` | Prints the sign-in link |
| `podman exec nocturne nocturne-server token --rotate` | Makes a new link; the old one stops working at once |
| `podman exec nocturne nocturne-server health` | Exits 0 while the server is up; the image's health check runs it |

The link is the only key, so keep it like a password. If you think it leaked, rotate it: within a few
seconds the old link stops working in every browser that used it.

### Reach it from other devices

The port is published on this machine's loopback only, and the server accepts plain HTTP only from
this machine. To use it from a phone or another computer, put a TLS proxy in front and tell the
server its address with `NOCTURNE_PUBLIC_ORIGIN`. It must be `https://`, with no path. The server
does not start with a plain `http://` address other than this machine's.

With [Caddy](https://caddyserver.com), which gets a certificate for the name by itself:

```text
nocturne.example.com {
    reverse_proxy 127.0.0.1:55066
}
```

With Tailscale, inside your own tailnet:

```bash
tailscale serve --bg --https=443 http://127.0.0.1:55066
```

Then start the container with the address the proxy serves, for example
`-e NOCTURNE_PUBLIC_ORIGIN=https://nocturne.example.com` or
`-e NOCTURNE_PUBLIC_ORIGIN=https://<machine>.<tailnet>.ts.net`, and print the link again: it now
starts with that address. If the proxy already serves something else on that name and port, give
Nocturne a port of its own (`--https=8443`) and include the port in the address.

### Compose

```yaml
services:
  nocturne:
    image: ghcr.io/agync/nocturne:latest
    ports:
      - "127.0.0.1:55066:55066"
    volumes:
      - nocturne-data:/data
    environment:
      NOCTURNE_PUBLIC_ORIGIN: https://nocturne.example.com
    restart: unless-stopped
volumes:
  nocturne-data: {}
```

`podman compose up -d` or `docker compose up -d` starts it.

### Your data, updates and removal

Everything lives in the `nocturne-data` volume: what you kept, the memory, the downloaded setup,
the logs (`logs/`) and the sign-in token. The first visit offers the usual setup, which downloads
once, with your consent, into the volume. To keep a folder of your own instead of a named volume,
make it owned by the container's user first (`podman unshare chown 10001:10001 <folder>` with
rootless podman) and mount it with `-v <folder>:/data:Z`.

To update, pull the new image and start a new container on the same volume:

```bash
podman pull ghcr.io/agync/nocturne:latest
podman rm -f nocturne
podman run -d --name nocturne -p 127.0.0.1:55066:55066 -v nocturne-data:/data ghcr.io/agync/nocturne:latest
```

To take it off, remove the container. The volume stays until you remove it yourself with
`podman volume rm nocturne-data`, which deletes everything you kept.

### Graphics cards

On a computer you use directly, Nocturne uses the graphics in it when the private engine finds
some that work: AMD, Intel or NVIDIA. Without any, it runs on the processor, which is slower but
does everything.

In a container the image runs on the processor and needs nothing more. A graphics card is
optional, and the published image does not carry the drivers for one. Do not pass a graphics device
to it.

For an AMD Radeon that Nocturne uses through Vulkan (the usual case for one built into the
processor), build the image with the Vulkan drivers and pass the device in:

```bash
podman build --format docker --build-arg EXTRA_PACKAGES=mesa-vulkan-drivers \
  -f Containerfile -t localhost/nocturne:vulkan .
podman run -d --name nocturne --device /dev/dri \
  -p 127.0.0.1:55066:55066 -v nocturne-data:/data localhost/nocturne:vulkan
```

With rootless podman, your own user must be able to open `/dev/dri/renderD128`; if it can only
through the `render` group, add `--group-add keep-groups`. This was tried with rootless podman on a
Radeon 890M, where the model servers opened the card. A discrete AMD card would use ROCm, which
needs `--device /dev/kfd --device /dev/dri`. Intel graphics take the same Vulkan build and device.
An NVIDIA card needs NVIDIA's own driver and container tooling.

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

- Mac: drag Nocturne from Applications to the Bin.
- Windows: open Settings, Apps, find Nocturne and choose Uninstall.
- AppImage: delete the file.
- rpm: `sudo dnf remove nocturne`
- deb: `sudo apt remove nocturne`
- Flatpak: `flatpak uninstall --user is.nocturne.Nocturne`. Your things stay in
  `~/.var/app/is.nocturne.Nocturne/` until you delete that folder.

If you turned on "Start with this computer", turn it off in Settings first. Otherwise your desktop
keeps a start-up entry for a program that is no longer there.
