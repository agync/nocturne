#!/usr/bin/env python3
"""Report the flaky and failed tests of nextest's JUnit reports, so a retry never hides one.

Usage: scripts/nextest_summary.py [--summary FILE] REPORT...

nextest retries a failed test once (.config/nextest.toml). A test that passes on the retry is
flaky: the run still passes, so this lists it in the job summary (FILE, appended in Markdown)
and prints a GitHub warning for it. A report that does not exist is skipped, since a run that
failed to build writes none.
"""

import os
import sys
import xml.etree.ElementTree as ET


def read(path):
    """The (tests, flaky, failed) of one report: a count and two lists of names."""
    tests, flaky, failed = 0, [], []
    for case in ET.parse(path).getroot().iter("testcase"):
        tests += 1
        name = f"{case.get('classname', '')} {case.get('name', '')}".strip()
        tags = {child.tag for child in case}
        if tags & {"failure", "error"}:
            failed.append(name)
        elif tags & {"flakyFailure", "flakyError"}:
            flaky.append(name)
    return tests, flaky, failed


def markdown(reports, tests, flaky, failed):
    lines = ["### Rust tests", ""]
    if not reports:
        return "\n".join(lines + ["No nextest report was written.", ""])
    lines.append(f"{tests} tests, {len(flaky)} flaky, {len(failed)} failed.")
    if flaky:
        lines += ["", "**Flaky** (failed, then passed on the retry; the run passes, the test needs fixing):", ""]
        lines += [f"- `{name}`" for name in flaky]
    if failed:
        lines += ["", "**Failed:**", ""]
        lines += [f"- `{name}`" for name in failed]
    return "\n".join(lines + [""])


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    summary = None
    if args[:1] == ["--summary"]:
        if len(args) < 2:
            args = []
        else:
            summary, args = args[1], args[2:]
    if not args:
        print("usage: nextest_summary.py [--summary FILE] REPORT...", file=sys.stderr)
        return 2
    reports = [p for p in args if os.path.isfile(p)]
    tests, flaky, failed = 0, [], []
    for path in reports:
        t, fl, fa = read(path)
        tests += t
        flaky += fl
        failed += fa
    for name in flaky:
        print(f"::warning title=Flaky test::{name} failed, then passed on the retry")
    text = markdown(reports, tests, flaky, failed)
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
