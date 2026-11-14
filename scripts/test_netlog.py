"""Tests for scripts/netlog.py with recorded strace lines."""

import contextlib
import io
import os
import unittest

import netlog

V4 = '{sa_family=AF_INET, sin_port=htons(%d), sin_addr=inet_addr("%s")}'
V6 = '{sa_family=AF_INET6, sin6_port=htons(%d), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "%s", &sin6_addr), sin6_scope_id=0}'


def connect(pid, addr):
    return f"{pid} connect(5, {addr}, 16) = 0\n"


class Verdicts(unittest.TestCase):
    def test_loopback_v4_v6_and_unix_pass(self):
        lines = [
            connect(10, V4 % (8080, "127.0.0.1")),
            connect(10, V6 % (8080, "::1")),
            '10 connect(6, {sa_family=AF_UNIX, sun_path="/tmp/.X11-unix/X99"}, 20) = 0\n',
            '10 connect(7, {sa_family=AF_UNIX, sun_path=@"/tmp/dbus-abc"}, 20) = 0\n',
            '10 connect(8, {sa_family=AF_UNIX, sun_path="/tmp/scratch/runtime/bus"}, 20) = 0\n',
        ]
        result = netlog.analyse(lines, "/tmp/scratch")
        self.assertEqual(result["failures"], [])
        self.assertEqual(result["calls"], 5)

    def test_public_v4_fails(self):
        result = netlog.analyse([connect(42, V4 % (443, "93.184.216.34"))])
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("pid 42", result["failures"][0])
        self.assertIn("93.184.216.34", result["failures"][0])

    def test_public_v6_fails(self):
        result = netlog.analyse([connect(43, V6 % (443, "2606:4700::1111"))])
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("pid 43", result["failures"][0])
        self.assertIn("2606:4700::1111", result["failures"][0])

    def test_port_53_on_loopback_fails(self):
        line = f'44 sendto(9, "\\1\\0", 30, MSG_NOSIGNAL, {V4 % (53, "127.0.0.53")}, 16) = 30\n'
        result = netlog.analyse([line])
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("port 53", result["failures"][0])

    def test_mapped_v4_in_v6_checked(self):
        bad = netlog.analyse([connect(45, V6 % (80, "::ffff:1.2.3.4"))])
        self.assertEqual(len(bad["failures"]), 1)
        good = netlog.analyse([connect(45, V6 % (80, "::ffff:127.0.0.1"))])
        self.assertEqual(good["failures"], [])

    def test_unfinished_and_resumed_lines_joined(self):
        lines = [
            f"46 connect(5, {V4 % (443, '8.8.8.8')}, 16 <unfinished ...>\n",
            "47 getpid() = 47\n",
            "46 <... connect resumed>) = -1 ENETUNREACH (Network is unreachable)\n",
        ]
        result = netlog.analyse(lines)
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("pid 46", result["failures"][0])

    def test_netlink_is_ignored_but_the_real_session_bus_fails(self):
        netlink = "48 connect(3, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 0\n"
        self.assertEqual(netlog.analyse([netlink, connect(1, V4 % (1, "127.0.0.1"))])["failures"], [])
        bus = '48 connect(4, {sa_family=AF_UNIX, sun_path="/run/user/1000/bus"}, 22) = 0\n'
        result = netlog.analyse([bus], "/tmp/scratch")
        self.assertEqual(len(result["failures"]), 1)
        self.assertIn("/run/user/1000/bus", result["failures"][0])

    def test_an_unreadable_internet_address_fails_instead_of_passing(self):
        line = "60 connect(5, {sa_family=AF_INET, sin_port=htons(80), sin_addr=weird}, 16) = 0\n"
        result = netlog.analyse([line])
        self.assertTrue(any("unreadable" in f for f in result["failures"]))

    def test_other_socket_families_fail(self):
        line = "61 connect(5, {sa_family=AF_PACKET, sll_protocol=htons(ETH_P_ALL)}, 20) = 0\n"
        self.assertTrue(netlog.analyse([line])["failures"])

    def test_bind_to_a_public_or_wildcard_address_fails_and_loopback_passes(self):
        wild = "62 bind(5, " + V4 % (8080, "0.0.0.0") + ", 16) = 0\n"
        loop = "62 bind(5, " + V4 % (8080, "127.0.0.1") + ", 16) = 0\n"
        self.assertEqual(len(netlog.analyse([wild])["failures"]), 1)
        self.assertEqual(netlog.analyse([loop])["failures"], [])

    def test_io_uring_fails(self):
        result = netlog.analyse(["63 io_uring_setup(256, {flags=0}) = 5\n", connect(1, V4 % (1, "127.0.0.1"))])
        self.assertTrue(any("io_uring" in f for f in result["failures"]))

    def test_an_empty_trace_fails(self):
        self.assertTrue(netlog.analyse([])["failures"])
        self.assertTrue(netlog.analyse(["64 getpid() = 64\n"])["failures"])

    def test_sendmsg_and_sendmmsg_destinations_checked(self):
        msg = f"49 sendmsg(5, {{msg_name={V4 % (9, '1.1.1.1')}, msg_namelen=16, msg_iov=[]}}, 0) = 4\n"
        mmsg = (
            f"50 sendmmsg(5, [{{msg_hdr={{msg_name={V6 % (9, '2001:db8::1')}, msg_namelen=28}}, "
            "msg_len=4}], 1, 0) = 1\n"
        )
        result = netlog.analyse([msg, mmsg])
        self.assertEqual(len(result["failures"]), 2)

    def test_pid_bracket_form(self):
        result = netlog.analyse([f"[pid 51] connect(5, {V4 % (80, '9.9.9.9')}, 16) = -1\n"])
        self.assertIn("pid 51", result["failures"][0])

    def test_name_service_sockets_fail(self):
        for path in (
            "/run/systemd/resolve/io.systemd.Resolve",
            "/run/avahi-daemon/socket",
            "/var/run/nscd/socket",
        ):
            line = f'52 connect(5, {{sa_family=AF_UNIX, sun_path="{path}"}}, 40) = -1 ENOENT\n'
            result = netlog.analyse([line])
            self.assertEqual(len(result["failures"]), 1, path)

    def test_system_bus_is_reported_not_failed(self):
        line = '53 connect(5, {sa_family=AF_UNIX, sun_path="/run/dbus/system_bus_socket"}, 40) = 0\n'
        result = netlog.analyse([line])
        self.assertEqual(result["failures"], [])
        self.assertTrue(result["notes"])

    def test_unrelated_calls_are_not_counted(self):
        result = netlog.analyse(["54 recvfrom(5, \"x\", 1, 0, NULL, NULL) = 1\n", "garbage\n"])
        self.assertEqual(result["calls"], 0)


APP = "/opt/nocturne/nocturne"
HELPER = "/data/remote/helpers/cloudflared/2026.11.0/cloudflared"
LEMOND = "/opt/nocturne/lemonade/lemond"


def execve(pid, path):
    return f'{pid} execve("{path}", ["x"], 0x7ffd /* 3 vars */) = 0\n'


def clone(parent, child, flags="CLONE_VM|CLONE_FS|SIGCHLD"):
    return f"{parent} clone(child_stack=NULL, flags={flags}) = {child}\n"


def udp_bind(pid, addr, port=0, fd=7):
    return [
        f"{pid} socket(AF_INET, SOCK_DGRAM|SOCK_CLOEXEC, IPPROTO_IP) = {fd}\n",
        f"{pid} bind({fd}, {V4 % (port, addr)}, 16) = 0\n",
    ]


def app_and_engine():
    return [execve(100, APP), clone(100, 300), execve(300, LEMOND)]


def processes():
    """An app that started a helper and the engine; the helper's thread shares its program."""
    return [
        execve(100, APP),
        clone(100, 200),
        execve(200, HELPER),
        clone(200, 201, "CLONE_VM|CLONE_THREAD|CLONE_SIGHAND"),
        clone(100, 300),
        execve(300, LEMOND),
    ]


class RemoteMode(unittest.TestCase):
    def judge(self, lines, mode="remote"):
        failures = netlog.analyse(processes() + lines, "/tmp/scratch", mode)["failures"]
        return [f for f in failures if "executed the helper" not in f]

    def test_app_pid_non_loopback_fails(self):
        failures = self.judge([connect(100, V4 % (443, "93.184.216.34"))])
        self.assertEqual(len(failures), 1)
        self.assertIn("pid 100 (app)", failures[0])

    def test_helper_pid_non_loopback_passes_in_remote_mode(self):
        lines = [connect(200, V4 % (7844, "198.51.100.1")), connect(201, V6 % (443, "2606:4700::1111"))]
        result = netlog.analyse(processes() + lines, "/tmp/scratch", "remote")
        self.assertEqual(result["failures"], [])
        self.assertIn("198.51.100.1:7844", result["helper_traffic"])
        self.assertIn("2606:4700::1111:443", result["helper_traffic"])

    def test_helper_pid_non_loopback_fails_in_off_mode(self):
        failures = self.judge([connect(200, V4 % (7844, "198.51.100.1"))], mode="off")
        self.assertEqual(len(failures), 1)
        self.assertIn("pid 200 (helper)", failures[0])

    def test_helper_any_interface_udp_bind_passes_in_remote_mode(self):
        self.assertEqual(self.judge(udp_bind(200, "0.0.0.0")), [])
        self.assertEqual(self.judge(udp_bind(201, "0.0.0.0", fd=9)), [])

    def test_tailnet_helper_listen_fails_in_every_mode(self):
        tailnet = "/data/remote/helpers/tailnet/1.104.6/nocturne-tailnet"
        lines = [execve(100, APP), clone(100, 200), execve(200, tailnet)]
        for mode in ("remote", "off"):
            trace = lines + udp_bind(200, "127.0.0.1") + ["200 listen(8, 128) = 0\n"]
            failures = netlog.analyse(trace, "/tmp/scratch", mode)["failures"]
            failures = [f for f in failures if "executed the helper" not in f]
            self.assertEqual(len(failures), 1, failures)
            self.assertIn("pid 200 (helper)", failures[0])
            self.assertIn("must never listen", failures[0])

    def test_cloudflared_may_listen_for_its_metrics(self):
        self.assertEqual(self.judge(udp_bind(200, "0.0.0.0") + ["200 listen(8, 128) = 0\n"]), [])

    def test_helper_udp_bind_on_every_interface_passes_without_a_listen(self):
        result = netlog.analyse(processes() + udp_bind(200, "0.0.0.0"), "/tmp/scratch", "remote")
        self.assertEqual(result["failures"], [])
        self.assertIn("0.0.0.0:0 (UDP bind)", result["helper_traffic"])

    def test_helper_tcp_bind_on_every_interface_still_fails(self):
        lines = [
            "200 socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 8\n",
            "200 bind(8, " + V4 % (8080, "0.0.0.0") + ", 16) = 0\n",
        ]
        self.assertEqual(len(self.judge(lines)), 1)

    def test_app_any_interface_bind_fails(self):
        failures = self.judge(udp_bind(100, "0.0.0.0"))
        self.assertEqual(len(failures), 1)
        self.assertIn("pid 100 (app)", failures[0])

    def test_lemond_pid_non_loopback_always_fails(self):
        for mode in ("remote", "off"):
            failures = self.judge([connect(300, V4 % (443, "93.184.216.34"))], mode)
            self.assertEqual(len(failures), 1, mode)
            self.assertIn("pid 300 (lemond)", failures[0])
        failures = self.judge(udp_bind(300, "0.0.0.0"))
        self.assertEqual(len(failures), 1)

    def test_loopback_for_everyone_passes(self):
        lines = [connect(p, V4 % (4100, "127.0.0.1")) for p in (100, 200, 300)]
        self.assertEqual(self.judge(lines), [])

    def test_a_remote_trace_without_helper_traffic_can_be_refused(self):
        log = "/tmp/scratch-netlog-none.log"
        with open(log, "w") as handle:
            handle.writelines(processes() + [connect(100, V4 % (4100, "127.0.0.1"))])
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                code = netlog.main([log, "--mode", "remote", "--require-helper-traffic", "--scratch", "/tmp/scratch"])
        finally:
            os.remove(log)
        self.assertEqual(code, 1)


class OffMode(unittest.TestCase):
    def test_off_mode_fails_on_app_listen(self):
        lines = app_and_engine() + ["100 listen(5, 128) = 0\n", connect(100, V4 % (1, "127.0.0.1"))]
        failures = netlog.analyse(lines, "/tmp/scratch")["failures"]
        self.assertEqual(len(failures), 1)
        self.assertIn("listen", failures[0])

    def test_off_mode_lets_the_engine_listen_and_remote_mode_lets_the_app(self):
        lines = app_and_engine() + ["300 listen(5, 128) = 0\n", "100 listen(6, 128) = 0\n", connect(100, V4 % (1, "127.0.0.1"))]
        off = netlog.analyse(lines, "/tmp/scratch", "off")["failures"]
        self.assertEqual(len(off), 1)
        self.assertIn("pid 100", off[0])
        self.assertEqual(netlog.analyse(lines, "/tmp/scratch", "remote")["failures"], [])

    def test_off_mode_fails_on_helper_exec(self):
        lines = [execve(100, APP), clone(100, 200), execve(200, HELPER), connect(100, V4 % (1, "127.0.0.1"))]
        failures = netlog.analyse(lines, "/tmp/scratch")["failures"]
        self.assertEqual(len(failures), 1)
        self.assertIn("helper", failures[0])
        self.assertEqual(netlog.analyse(lines, "/tmp/scratch", "remote")["failures"], [])

    def test_off_mode_fails_on_listen_by_a_program_the_app_started(self):
        lines = [execve(100, APP), clone(100, 400), execve(400, "/usr/lib/webkit/WebKitNetworkProcess"), "400 listen(5, 1) = 0\n", connect(100, V4 % (1, "127.0.0.1"))]
        for exe in (None, APP):
            failures = netlog.analyse(lines, "/tmp/scratch", "off", app_exe=exe)["failures"]
            self.assertEqual(len(failures), 1, exe)
            self.assertIn("pid 400", failures[0])
        self.assertEqual(netlog.analyse(lines, "/tmp/scratch", "remote", app_exe=APP)["failures"], [])

    def test_explicit_helper_folders(self):
        lines = [execve(100, "/custom/h/cloudflared"), connect(100, V4 % (1, "127.0.0.1"))]
        self.assertEqual(netlog.analyse(lines, "/tmp/scratch")["failures"], [])
        failures = netlog.analyse(lines, "/tmp/scratch", helpers=["/custom/h/"])["failures"]
        self.assertEqual(len(failures), 1)


PROVIDER = "198.51.100.7:8443"
T0 = 1_000_000.0


def stamped(pid, at, address, port=8443):
    return f"{pid} {at:.6f} connect(7, {V4 % (port, address)}, 16) = -1 ENETUNREACH (Network is unreachable)\n"


def to_provider(pid, at):
    return stamped(pid, at, "198.51.100.7")


def online(lines, marks, provider=PROVIDER, trace=None):
    trace = app_and_engine() if trace is None else trace
    return netlog.analyse(trace + lines, "/tmp/scratch", "online", provider=provider, marks=marks)["failures"]


class OnlineMode(unittest.TestCase):
    def test_lemond_to_configured_provider_passes_in_online_mode(self):
        marks = [(T0, "check"), (T0 + 10, "ask"), (T0 + 20, "ask")]
        lines = [to_provider(300, T0 + 0.2), to_provider(300, T0 + 10.3), to_provider(300, T0 + 20.1)]
        result = netlog.analyse(app_and_engine() + lines, "/tmp/scratch", "online", provider=PROVIDER, marks=marks)
        self.assertEqual(result["failures"], [])
        self.assertEqual(result["provider_traffic"], {PROVIDER: 3})

    def test_lemond_to_other_host_fails_in_online_mode(self):
        marks = [(T0, "ask")]
        failures = online([stamped(300, T0 + 0.1, "203.0.113.9")], marks)
        self.assertEqual(len(failures), 1)
        self.assertIn("203.0.113.9", failures[0])
        wrong_port = online([stamped(300, T0 + 0.1, "198.51.100.7", 443)], marks)
        self.assertEqual(len(wrong_port), 1)

    def test_lemond_provider_traffic_fails_in_off_mode(self):
        for kwargs in ({}, {"provider": PROVIDER, "marks": [(T0, "ask")]}):
            failures = netlog.analyse(
                app_and_engine() + [to_provider(300, T0 + 0.1)], "/tmp/scratch", "off", **kwargs
            )["failures"]
            self.assertEqual(len(failures), 1, kwargs)
            self.assertIn("pid 300 (lemond)", failures[0])

    def test_app_pid_non_loopback_fails_in_online_mode(self):
        marks = [(T0, "ask")]
        failures = online([to_provider(100, T0 + 0.1), stamped(100, T0 + 0.2, "93.184.216.34", 443)], marks)
        self.assertEqual(len(failures), 2)
        self.assertTrue(all("pid 100 (app)" in f for f in failures))

    def test_lemond_connects_bounded_by_asks(self):
        marks = [(T0, "ask"), (T0 + 10, "ask")]
        twice = online([to_provider(300, T0 + 0.1), to_provider(300, T0 + 0.2)], marks)
        self.assertEqual(len(twice), 1)
        self.assertIn("second connection", twice[0])
        before = online([to_provider(300, T0 - 5)], marks)
        self.assertEqual(len(before), 1)
        self.assertIn("before any launch, check or ask", before[0])
        self.assertEqual(online([to_provider(300, T0 + 0.1), to_provider(300, T0 + 10.1)], marks), [])

    def test_one_connection_per_launch_beside_the_asks(self):
        marks = [(T0, "launch"), (T0 + 10, "ask")]
        self.assertEqual(online([to_provider(300, T0 + 1), to_provider(300, T0 + 10.2)], marks), [])
        twice = online([to_provider(300, T0 + 1), to_provider(300, T0 + 2)], marks)
        self.assertEqual(len(twice), 1)
        self.assertIn("second connection", twice[0])

    def test_lemond_connect_after_off_mark_fails(self):
        marks = [(T0, "ask"), (T0 + 30, "off")]
        failures = online([to_provider(300, T0 + 0.1), to_provider(300, T0 + 31)], marks)
        self.assertEqual(len(failures), 1)
        self.assertIn("after the off mark", failures[0])

    def test_lemond_connect_during_idle_fails(self):
        marks = [(T0, "ask"), (T0 + 5, "idle-start"), (T0 + 605, "idle-end"), (T0 + 610, "ask")]
        lines = [to_provider(300, T0 + 0.1), to_provider(300, T0 + 300), to_provider(300, T0 + 610.2)]
        failures = online(lines, marks)
        self.assertEqual(len(failures), 1)
        self.assertIn("idle", failures[0])

    def test_an_unstamped_provider_connection_fails_and_the_log_reads_stamps(self):
        marks = [(T0, "ask")]
        unstamped = connect(300, V4 % (8443, "198.51.100.7"))
        failures = online([unstamped], marks)
        self.assertEqual(len(failures), 1)
        self.assertIn("timestamp", failures[0])
        joined = [
            f"300 {T0 + 0.1:.6f} connect(7, {V4 % (8443, '198.51.100.7')}, 16 <unfinished ...>\n",
            f"301 {T0 + 0.2:.6f} getpid() = 301\n",
            f"300 {T0 + 0.3:.6f} <... connect resumed>) = -1 ENETUNREACH (Network is unreachable)\n",
        ]
        self.assertEqual(online(joined, marks), [])

    def test_the_app_may_not_listen_in_online_mode(self):
        failures = online(["100 listen(5, 128) = 0\n", "300 listen(6, 128) = 0\n", connect(300, V4 % (1, "127.0.0.1"))], [(T0, "ask")])
        self.assertEqual(len(failures), 1)
        self.assertIn("pid 100", failures[0])

    def test_provider_traffic_can_be_required(self):
        log = "/tmp/scratch-netlog-online.log"
        marks = "/tmp/scratch-netlog-online.marks"
        with open(log, "w") as handle:
            handle.writelines(app_and_engine() + [connect(300, V4 % (4100, "127.0.0.1"))])
        with open(marks, "w") as handle:
            handle.write(f"{T0} ask\n")
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                args = [log, "--mode", "online", "--provider", PROVIDER, "--marks", marks, "--scratch", "/tmp/scratch"]
                self.assertEqual(netlog.main(args), 0)
                self.assertEqual(netlog.main(args + ["--require-provider-traffic"]), 1)
        finally:
            os.remove(log)
            os.remove(marks)


if __name__ == "__main__":
    unittest.main()
