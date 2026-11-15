---
type: Guide
title: Faster with the NPU
description: What the NPU in some AMD computers does for Nocturne, why it may not be in use, and the steps to set it up on Linux.
status: draft
tags: [guide, npu, linux, speed, advanced]
---

# Faster with the NPU

Some AMD computers have an NPU, a separate part made for this kind of work. When Nocturne can use
it, voice notes and other recordings are read there, so answers and the overnight tidy are not
slowed by them. With about 32 GB of memory it also reads everything you keep and answers your
questions; with about 48 GB, or about 72 GB and more, it reads everything you keep while the graphics
answer. Everything works without it: recordings and items are then read the way they always were.
This page uses the word NPU because the advanced details do.

Settings shows a quiet note, "This computer has a part that could make Nocturne faster, but it
isn't set up", only when the NPU is there and one of the steps below would let Nocturne use it.
"How to set it up" opens the advanced details, which say exactly why and what to do.

## Which computers

An AMD Ryzen AI computer with an XDNA 2 NPU: the Ryzen AI 300 series (Strix Point, Kraken Point),
Ryzen AI Max (Strix Halo), the Ryzen AI 400 series (Gorgon Point) and the Z2 Extreme handhelds.
The Ryzen 7000, 8000 and Ryzen AI 200 series have an older NPU that is not supported.

Nocturne leaves the NPU alone, on purpose, when:

- the computer has about 8 GB of memory: reading recordings on the NPU needs about 0.6 GB more
  memory than the usual way, and that does not fit beside the rest;
- you chose a model for reading recordings yourself in "Choose what runs each job";
- it is a Mac, which has no NPU of this kind.

## See what Nocturne found

Settings, then "Show advanced details". The NPU row says one of:

| It says | Meaning |
|---|---|
| none found | No NPU, or one Nocturne cannot see at all |
| reads recordings | It is in use |
| reads recordings and what you keep | It also reads what you keep, while the graphics answer |
| reads, answers and reads recordings | It does all of it, and nothing runs on the graphics |
| not used, and a reason | The reason, the engine's own words when it gave any, and the steps |

## What Linux needs

Nocturne's private engine downloads its own copy of the NPU runtime (FastFlowLM, with the
libraries it needs) the first time setup sees a usable NPU. You do not install FastFlowLM or XRT
for Nocturne. The computer itself needs:

| Part | Needed | Check |
|---|---|---|
| Kernel driver | `amdxdna`, in Linux 7.0 and later, or `amdxdna-dkms` for older kernels | `lsmod \| grep amdxdna` and `uname -r` |
| NPU firmware | 1.1.0.0 or later, from `linux-firmware` (20260221 or later) | `cat /sys/bus/pci/drivers/amdxdna/*/fw_version` |
| Memory lock limit | high; unlimited is safest (the engine refused 8 MB and 64 MB in our tests) | `ulimit -l` |
| IOMMU | on: `amd_iommu=off` on the kernel command line hides the NPU | `cat /proc/cmdline` |

These are Lemonade's requirements for its NPU support, from its Linux guide (see Sources). Each
step below that installs or changes something on the system needs an administrator.

### Fedora

Fedora 44's kernel includes `amdxdna` and its `linux-firmware` carries firmware new enough
(checked on a Ryzen AI 9 HX 470 with kernel 7.3 and firmware 1.1.2.64). Keep the system up to
date, then set the memory lock limit (below).

### Ubuntu

- 26.04: the kernel includes the driver. Set the memory lock limit (below).
- 24.04 and 25.10: Lemonade's guide adds its PPA and the DKMS driver, then a restart:

```bash
sudo add-apt-repository ppa:lemonade-team/stable
sudo apt update
sudo apt install libxrt-npu2 amdxdna-dkms
sudo reboot
```

### Arch Linux

Lemonade's guide uses kernel 7.0 or later (`sudo pacman -S linux`) and a current
`linux-firmware`; for kernels 6.18 and 6.19 it uses `amdxdna-dkms` from the AUR.

### Other distributions

A kernel of 7.0 or later and `linux-firmware` of 20260221 or later, then the memory lock limit.

### The memory lock limit

Many systems allow only 8 MB, which the NPU refuses. Add these two lines to
`/etc/security/limits.conf` (or a file in `/etc/security/limits.d/`), then log out and back in:

```text
*    soft    memlock    unlimited
*    hard    memlock    unlimited
```

`ulimit -l` in a new terminal should then say `unlimited`.

### Afterwards

Quit Nocturne and open it again. If the advanced details now say the NPU's runtime or speech
model is not downloaded, choose "Run setup again" there: setup fetches them (about 0.66 GB) only
when they will be used.

### The Flatpak

Nocturne's Flatpak asks for access to all devices, so it can see the NPU, and it has the same
memory lock limit as your login. The steps above apply unchanged.

## Windows

Lemonade supports the XDNA 2 NPU on Windows too. The advanced details show what the engine
reports.

## Sources

- Lemonade, "LLMs on Linux with FastFlowLM" (`docs/flm_npu_linux.html` in the Lemonade
  repository): processors, firmware, kernel, memory lock and per-distribution steps.
- [Decision 0006](/adr/0006-npu-optional.md): what runs on the NPU and the measurements.
