"""Tests for scripts/nextest_summary.py: flaky and failed tests from nextest's JUnit reports."""

import io
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import nextest_summary as ns  # noqa: E402

# The shape nextest writes: a flaky pass, a pass, and a test that failed on both tries.
REPORT = """<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" failures="1" errors="0">
  <testsuite name="nocturne-engine::journal" tests="3" failures="1" errors="0">
    <testcase name="a_day_is_kept" classname="nocturne-engine::journal" time="0.10">
      <flakyFailure type="test failure" message="code=101">
        <system-out>first try</system-out>
      </flakyFailure>
    </testcase>
    <testcase name="a_page_is_read" classname="nocturne-engine::journal" time="0.01"/>
    <testcase name="a_line_cites" classname="nocturne-engine::journal" time="0.02">
      <failure type="test failure" message="code=101"/>
      <rerunFailure type="test failure" message="code=101"/>
    </testcase>
  </testsuite>
</testsuites>
"""

CLEAN = """<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="1" failures="0" errors="0">
  <testsuite name="nocturne-app::ipc" tests="1">
    <testcase name="every_command_answers" classname="nocturne-app::ipc"/>
  </testsuite>
</testsuites>
"""


class Summary(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def write(self, name, text):
        path = os.path.join(self.dir.name, name)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        return path

    def run_main(self, args):
        out = io.StringIO()
        with redirect_stdout(out):
            code = ns.main(args)
        return code, out.getvalue()

    def test_a_flaky_test_is_warned_and_listed(self):
        report = self.write("junit.xml", REPORT)
        summary = os.path.join(self.dir.name, "summary.md")
        code, out = self.run_main(["--summary", summary, report])
        self.assertEqual(code, 0)
        self.assertEqual(
            out,
            "::warning title=Flaky test::nocturne-engine::journal a_day_is_kept"
            " failed, then passed on the retry\n",
        )
        with open(summary, encoding="utf-8") as f:
            text = f.read()
        self.assertIn("3 tests, 1 flaky, 1 failed.", text)
        self.assertIn("**Flaky**", text)
        self.assertIn("- `nocturne-engine::journal a_day_is_kept`", text)
        self.assertIn("- `nocturne-engine::journal a_line_cites`", text)
        self.assertNotIn("a_page_is_read", text)

    def test_reports_add_up_and_a_missing_one_is_skipped(self):
        reports = [self.write("a.xml", REPORT), self.write("b.xml", CLEAN), "/nonexistent.xml"]
        code, out = self.run_main(reports)
        self.assertEqual(code, 0)
        self.assertIn("4 tests, 1 flaky, 1 failed.", out)

    def test_a_clean_run_says_so(self):
        out = self.run_main([self.write("junit.xml", CLEAN)])[1]
        self.assertIn("1 tests, 0 flaky, 0 failed.", out)
        self.assertNotIn("::warning", out)

    def test_no_report_at_all(self):
        out = self.run_main(["/nonexistent.xml"])[1]
        self.assertIn("No nextest report was written.", out)

    def test_usage(self):
        for args in ([], ["--summary"], ["--summary", "x"]):
            with self.subTest(args=args), redirect_stderr(io.StringIO()):
                self.assertEqual(ns.main(args), 2)


if __name__ == "__main__":
    unittest.main()
