#!/usr/bin/env python3
"""Judge an strace log for outbound traffic. Used by scripts/net-check.sh.

Usage: scripts/netlog.py LOG [LOG ...] [--scratch DIR]

Reads logs written by `strace -f -e trace=%network,io_uring_setup`. Fails (exit 1) on:
  * any AF_INET or AF_INET6 destination outside 127.0.0.0/8 and ::1 (an IPv4-mapped IPv6
    address is judged as the IPv4 address it wraps), and any bind to a non-loopback address;
  * any destination port 53, even on loopback (a name lookup is an attempt to reach out);
  * an AF_INET or AF_INET6 address the parser cannot read, and any socket family other than
    unix, netlink and the two internet families;
  * any local socket that is not the scratch folder, the display, or the private session bus
    (so the real session bus, and the resolver, Avahi and nscd sockets, all fail);
  * io_uring, which can send without a traced network call;
  * a log with no network calls at all, which proves nothing.
The system bus is reported and does not fail.
"""

import argparse
import ipaddress
import re
import sys

CALLS = ("connect", "bind", "sendto", "sendmsg", "sendmmsg")
PRIVATE_BUS = ("/tmp/dbus-", "@/tmp/dbus-")
DISPLAY = ("/tmp/.X11-unix/", "@/tmp/.X11-unix/")
FAMILIES_OK = ("AF_UNIX", "AF_NETLINK", "AF_UNSPEC", "AF_INET", "AF_INET6")
LINE = re.compile(r"^(?:\[pid\s+(\d+)\]\s+|(\d+)\s+)?(.*)$")
START = re.compile(r"^(\w+)\(")
RESUMED = re.compile(r"^<\.\.\. (\w+) resumed>(.*)$")
SOCKADDR = re.compile(r"\{sa_family=(AF_[A-Z0-9_]+)(.*?)\}")
V4 = re.compile(r'sin_port=htons\((\d+)\), sin_addr=inet_addr\("([^"]+)"\)')
V6 = re.compile(r'sin6_port=htons\((\d+)\).*?inet_pton\(AF_INET6, "([^"]+)"')
UNIX = re.compile(r'sun_path=(@?)"([^"]*)"')

FORBIDDEN_SOCKETS = (
    "/run/systemd/resolve/",
    "/var/run/systemd/resolve/",
    "/run/avahi-daemon/",
    "/var/run/avahi-daemon/",
    "/run/nscd/",
    "/var/run/nscd/",
)
SYSTEM_BUS = ("/run/dbus/system_bus_socket", "/var/run/dbus/system_bus_socket")


def join_lines(lines):
    """Join strace's `<unfinished ...>` and `<... resumed>` halves into whole calls."""
    pending = {}
    for raw in lines:
        raw = raw.rstrip("\n")
        m = LINE.match(raw)
        pid = m.group(1) or m.group(2) or "?"
        body = m.group(3)
        resumed = RESUMED.match(body)
        if resumed:
            head = pending.pop((pid, resumed.group(1)), None)
            if head is not None:
                yield pid, head + resumed.group(2)
            continue
        if body.endswith("<unfinished ...>"):
            started = START.match(body)
            if started:
                pending[(pid, started.group(1))] = body[: -len("<unfinished ...>")]
            continue
        yield pid, body


def judge_inet(family, port, address, binding=False):
    """Return a failure reason, or None when the destination is acceptable."""
    try:
        ip = ipaddress.ip_address(address)
    except ValueError:
        return f"unparseable address {address}"
    shown = address
    if isinstance(ip, ipaddress.IPv6Address) and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    if port == 53 and not binding:
        return f"name lookup on port 53 at {shown}"
    if ip.is_loopback:
        return None
    if not binding and isinstance(ip, ipaddress.IPv4Address) and str(ip) == "0.0.0.0":
        return None
    if binding:
        return f"bind to {shown} port {port}, which is not loopback"
    return f"{family} destination {shown} port {port} is not loopback"


def local_failure(path, scratch):
    """Why a local socket path is not allowed, or None."""
    if not path.startswith("@") and path.startswith(FORBIDDEN_SOCKETS):
        return "name-service socket"
    if path in SYSTEM_BUS:
        return None
    if scratch and path.lstrip("@").startswith(scratch):
        return None
    if path.startswith(DISPLAY) or path.startswith(PRIVATE_BUS):
        return None
    return "local socket outside the scratch folder, the display and the private session bus"


def analyse(lines, scratch=None):
    failures, local, notes, loopback = [], {}, [], {}
    calls = 0
    for pid, body in join_lines(lines):
        started = START.match(body)
        if started and started.group(1) == "io_uring_setup":
            failures.append(f"pid {pid}: io_uring_setup, which can send without a traced call")
            continue
        if not started or started.group(1) not in CALLS:
            continue
        calls += 1
        call = started.group(1)
        for match in SOCKADDR.finditer(body):
            family, rest = match.group(1), match.group(2)
            if family not in FAMILIES_OK:
                failures.append(f"pid {pid}: {call} on socket family {family}")
                continue
            if family in ("AF_INET", "AF_INET6"):
                found = (V4 if family == "AF_INET" else V6).search(rest)
                if not found:
                    failures.append(f"pid {pid}: {call} with an unreadable {family} address: {match.group(0)[:80]}")
                    continue
                port, address = int(found.group(1)), found.group(2)
                reason = judge_inet(family, port, address, binding=(call == "bind"))
                if reason:
                    failures.append(f"pid {pid}: {call} {reason}")
                else:
                    key = f"{address}:{port}"
                    loopback[key] = loopback.get(key, 0) + 1
            elif family == "AF_UNIX":
                found = UNIX.search(rest)
                if not found:
                    continue
                path = ("@" if found.group(1) else "") + found.group(2)
                local.setdefault(path, set()).add(pid)
                if path in SYSTEM_BUS:
                    notes.append(f"pid {pid}: system bus {path}")
                    continue
                reason = local_failure(path, scratch)
                if reason:
                    failures.append(f"pid {pid}: {call} to {reason} {path}")
    if calls == 0:
        failures.append("the trace holds no network calls, so it proves nothing")
    return {
        "calls": calls,
        "failures": failures,
        "local": local,
        "notes": sorted(set(notes)),
        "loopback": loopback,
        "scratch": scratch,
    }


def classify(path, scratch):
    if scratch and path.lstrip("@").startswith(scratch):
        return "scratch folder"
    if path.startswith(DISPLAY):
        return "display"
    if path.startswith(PRIVATE_BUS):
        return "private session bus"
    if path in SYSTEM_BUS:
        return "system bus"
    return "NOT ALLOWED"


def report(result):
    print(f"traced {result['calls']} connect/send calls")
    if result["loopback"]:
        print("loopback destinations:")
        for key, count in sorted(result["loopback"].items()):
            print(f"  {key}  x{count}")
    if result["local"]:
        print("local sockets:")
        for path in sorted(result["local"]):
            pids = ",".join(sorted(result["local"][path]))
            print(f"  {path}  [{classify(path, result['scratch'])}]  pids {pids}")
    for note in result["notes"]:
        print(f"note: {note}")
    if result["failures"]:
        print("outbound traffic found:")
        for failure in result["failures"]:
            print(f"  FAIL {failure}")
        return 1
    print("no traffic left this computer")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("logs", nargs="+")
    parser.add_argument("--scratch")
    args = parser.parse_args(argv)
    lines = []
    for path in args.logs:
        with open(path, encoding="utf-8", errors="replace") as handle:
            lines.extend(handle.readlines())
    return report(analyse(lines, args.scratch))


if __name__ == "__main__":
    sys.exit(main())
