"""Tests for scripts/netlog.py with recorded strace lines."""

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


if __name__ == "__main__":
    unittest.main()
