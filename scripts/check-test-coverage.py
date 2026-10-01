#!/usr/bin/env python3
"""Coverage gate between a nextest list manifest and a nextest JUnit report.

Given `cargo nextest list --message-format json` output for a selection and
the JUnit report written by `cargo nextest run` for the same selection, this
checker fails unless the two agree exactly:

  * every test the manifest selected appears in the report exactly once,
    executed, not failed, not skipped (JUnit may omit ignored tests, so the
    manifest's own `ignored` flag is also rejected for selected tests);
  * the report contains no test the manifest did not select (stale artifact
    or wrong-filter report);
  * manifest identities are nextest's canonical binary id: every rust-suites
    key must equal the suite's authoritative `binary-id` field
    (kind-qualified for non-test targets, e.g. `pkg::bin/tool`), which is
    exactly the classname nextest writes into its JUnit report, so bin-kind
    and any future target kinds pair without a name inventory or derived
    aliases;
  * unknown manifest/JUnit schemas, missing or empty inputs, duplicate JSON
    keys, duplicate identities, rust-suites keys that disagree with their
    `binary-id`, unrecognized status values, and malformed fields fail with
    explicit messages instead of passing silently. Additive schema changes
    (new fields) are tolerated.

Unit tests live in scripts/tests/; `--self-test` discovers and runs them.
"""

from __future__ import annotations

import argparse
import json
import sys
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

SELECTED_STATUS = "matches"
KNOWN_FILTER_STATUSES = frozenset({"matches", "mismatch"})
KNOWN_SUITE_STATUSES = frozenset({"listed", "skipped"})


class CoverageError(Exception):
    """Unparseable or unexpected input; the check cannot be evaluated."""


def _reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise CoverageError(f"manifest contains duplicate JSON key {key!r}")
        result[key] = value
    return result


def parse_manifest(text: str) -> tuple[dict, list]:
    try:
        data = json.loads(text, object_pairs_hook=_reject_duplicate_keys)
    except json.JSONDecodeError as exc:
        raise CoverageError(f"manifest is not valid JSON: {exc}") from exc
    if not isinstance(data, dict) or not isinstance(data.get("rust-suites"), dict):
        raise CoverageError(
            "unknown manifest schema: expected the JSON object written by "
            "`cargo nextest list --message-format json` (a 'rust-suites' mapping)"
        )
    expected: dict = {}
    ignored_selected: list = []
    for suite_key, suite in data["rust-suites"].items():
        if not isinstance(suite, dict) or not isinstance(suite.get("testcases"), dict):
            raise CoverageError(
                f"unknown manifest schema: suite {suite_key!r} lacks a 'testcases' mapping"
            )
        package = suite.get("package-name")
        binary = suite.get("binary-name")
        for field, value in (("package-name", package), ("binary-name", binary)):
            if not isinstance(value, str) or not value:
                raise CoverageError(
                    f"unknown manifest schema: suite {suite_key!r} needs a "
                    f"non-empty string '{field}'"
                )
        suite_status = suite.get("status", "listed")
        if suite_status not in KNOWN_SUITE_STATUSES:
            raise CoverageError(
                f"unknown manifest schema: suite {suite_key!r} has unrecognized "
                f"status {suite_status!r} (known: {sorted(KNOWN_SUITE_STATUSES)})"
            )
        binary_id = suite.get("binary-id")
        if not isinstance(binary_id, str) or not binary_id:
            raise CoverageError(
                f"unknown manifest schema: suite {suite_key!r} needs a "
                f"non-empty string 'binary-id'"
            )
        if binary_id != suite_key:
            raise CoverageError(
                f"unknown manifest schema: suite key {suite_key!r} does not "
                f"match its authoritative binary-id {binary_id!r}"
            )
        suite_id = binary_id
        for name, info in suite["testcases"].items():
            if not isinstance(info, dict):
                raise CoverageError(
                    f"unknown manifest schema: test {suite_id}::{name} is not an object"
                )
            filter_match = info.get("filter-match")
            if not isinstance(filter_match, dict):
                raise CoverageError(
                    f"unknown manifest schema: test {suite_id}::{name} lacks 'filter-match'"
                )
            status = filter_match.get("status")
            if status not in KNOWN_FILTER_STATUSES:
                raise CoverageError(
                    f"unknown manifest schema: test {suite_id}::{name} has unrecognized "
                    f"filter-match status {status!r} "
                    f"(known: {sorted(KNOWN_FILTER_STATUSES)})"
                )
            ignored = info.get("ignored")
            if not isinstance(ignored, bool):
                raise CoverageError(
                    f"unknown manifest schema: test {suite_id}::{name} needs a "
                    f"boolean 'ignored'"
                )
            if status != SELECTED_STATUS:
                continue
            if suite_status != "listed":
                raise CoverageError(
                    f"selected test {suite_id}::{name} sits in suite with "
                    f"unexpected status {suite_status!r}"
                )
            identity = (suite_id, name)
            if identity in expected:
                raise CoverageError(
                    f"manifest contains duplicate test identity {suite_id}::{name}"
                )
            expected[identity] = True
            if ignored:
                ignored_selected.append(f"{suite_id}::{name}")
    return expected, ignored_selected


def parse_junit(text: str) -> tuple[dict, list]:
    try:
        root = ET.fromstring(text)
    except ET.ParseError as exc:
        raise CoverageError(f"JUnit report is not valid XML: {exc}") from exc
    if root.tag not in ("testsuites", "testsuite"):
        raise CoverageError(
            f"unknown JUnit schema: root element <{root.tag}>, expected <testsuites>"
        )
    executed: dict = {}
    duplicates: list = []
    for suite in root.iter("testsuite"):
        suite_name = suite.get("name", "")
        for case in suite.findall("testcase"):
            name = case.get("name")
            if not name:
                raise CoverageError("unknown JUnit schema: <testcase> without a name")
            key = (case.get("classname") or suite_name, name)
            if key in executed:
                duplicates.append(key)
                continue
            if case.find("skipped") is not None or case.get("status") == "skipped":
                state = "skipped"
            elif case.find("failure") is not None or case.find("error") is not None:
                state = "failed"
            else:
                state = "passed"
            executed[key] = state
    return executed, duplicates


def compare(expected: dict, ignored_selected: list, executed: dict, duplicates: list) -> list:
    problems: list = []
    for key in duplicates:
        problems.append(f"duplicate JUnit record: {key[0]}::{key[1]}")
    for name in ignored_selected:
        problems.append(f"selected test is ignored (JUnit would omit it): {name}")
    for key, state in executed.items():
        if state != "passed":
            problems.append(f"JUnit test {state}: {key[0]}::{key[1]}")
    missing = [key for key in expected if key not in executed]
    extra = [key for key in executed if key not in expected]
    for key in missing[:10]:
        problems.append(
            f"selected test missing from JUnit (not executed, ignored, or stale report): "
            f"{key[0]}::{key[1]}"
        )
    if len(missing) > 10:
        problems.append(f"... and {len(missing) - 10} more selected tests missing from JUnit")
    for key in extra[:10]:
        problems.append(
            f"JUnit test not selected by manifest (stale report or wrong filter): "
            f"{key[0]}::{key[1]}"
        )
    if len(extra) > 10:
        problems.append(f"... and {len(extra) - 10} more JUnit tests not selected by manifest")
    if not expected:
        problems.append(
            "manifest selected 0 tests: the filter matched nothing (wrong filter or "
            "stale manifest)"
        )
    if not executed:
        problems.append("JUnit report contains 0 test cases (empty report)")
    return problems


def run_check(manifest_path: str, junit_path: str, label: str) -> int:
    prefix = f"[{label}] " if label else ""
    try:
        manifest_text = Path(manifest_path).read_text(encoding="utf-8-sig")
    except OSError as exc:
        print(f"{prefix}FAIL: cannot read manifest {manifest_path}: {exc}", file=sys.stderr)
        return 1
    try:
        junit_text = Path(junit_path).read_text(encoding="utf-8-sig")
    except OSError as exc:
        print(f"{prefix}FAIL: cannot read JUnit report {junit_path}: {exc}", file=sys.stderr)
        return 1
    try:
        expected, ignored_selected = parse_manifest(manifest_text)
        executed, duplicates = parse_junit(junit_text)
    except CoverageError as exc:
        print(f"{prefix}FAIL: {exc}", file=sys.stderr)
        return 1
    problems = compare(expected, ignored_selected, executed, duplicates)
    binaries = len({key[0] for key in expected})
    if problems:
        for problem in problems:
            print(f"{prefix}FAIL: {problem}", file=sys.stderr)
        print(
            f"{prefix}coverage check failed: {len(expected)} selected tests, "
            f"{len(executed)} JUnit records",
            file=sys.stderr,
        )
        return 1
    print(
        f"{prefix}OK: {len(expected)} selected tests across {binaries} binaries all "
        f"executed and passed"
    )
    return 0


def run_self_tests() -> int:
    tests_dir = Path(__file__).resolve().parent / "tests"
    suite = unittest.defaultTestLoader.discover(str(tests_dir), pattern="test_*.py")
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--manifest", help="nextest list JSON for the selection")
    parser.add_argument("--junit", help="JUnit report written by the same nextest run")
    parser.add_argument("--label", default="", help="prefix for progress and failure messages")
    parser.add_argument(
        "--self-test", action="store_true", help="run the unit tests in scripts/tests and exit"
    )
    args = parser.parse_args()
    if args.self_test:
        return run_self_tests()
    if not args.manifest or not args.junit:
        parser.error("--manifest and --junit are required (or pass --self-test)")
    return run_check(args.manifest, args.junit, args.label)


if __name__ == "__main__":
    sys.exit(main())
