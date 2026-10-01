"""Unit tests for scripts/check-test-coverage.py.

Fixtures mirror the real schemas of `cargo nextest list --message-format json`
(rust-suites/testcases/filter-match/ignored) and nextest's quick-junit output
(testsuites/testsuite/testcase) using synthetic names only. Identities are
nextest's canonical binary id as emitted by 0.9.145: each rust-suites key
equals the suite's `binary-id` and the JUnit classname (the bare package for
the lib binary, `package::binary` for integration tests, and the
kind-qualified `package::bin/<binary>` for bin targets). The checker never
derives identities from package/binary names, so new target kinds pair
automatically.
"""

from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

_HELPER = Path(__file__).resolve().parents[1] / "check-test-coverage.py"
_spec = importlib.util.spec_from_file_location("check_test_coverage", _HELPER)
ctc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ctc)


def selected(ignored=False):
    return {"ignored": ignored, "filter-match": {"status": "matches"}}


def mismatch(reason="expression", ignored=False):
    return {"ignored": ignored, "filter-match": {"status": "mismatch", "reason": reason}}


def suite(binary_id, binary, package="demo", status="listed", kind="lib", cases=None):
    # `binary_id` must be the canonical rust-suites key (callers map the
    # suite under that same key); `kind`/`binary` mirror real metadata.
    return {
        "package-name": package,
        "binary-id": binary_id,
        "binary-name": binary,
        "package-id": "path+file:///demo#1.0.0",
        "kind": kind,
        "binary-path": "/demo/target/ci/deps/x",
        "build-platform": "target",
        "cwd": "/demo",
        "status": status,
        "testcases": cases if cases is not None else {},
    }


BIN_KEY = "demo::bin/demo-agent-wrapper"
BIN_CASE = "tests::wrapper_args_parse"


def bin_suite(cases=None):
    return suite(
        BIN_KEY,
        binary="demo-agent-wrapper",
        kind="bin",
        cases=cases if cases is not None else {BIN_CASE: selected()},
    )


def manifest_json(rust_suites, **extra_top):
    data = {
        "rust-build-meta": {"target-directory": "/demo/target"},
        "test-count": 0,
        "rust-suites": rust_suites,
    }
    data.update(extra_top)
    return json.dumps(data)


def junit_report(suites):
    parts = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<testsuites name="nextest-run" tests="3" failures="0" errors="0" uuid="u"'
        ' timestamp="t" time="0.1">',
    ]
    for classname, cases in suites:
        parts.append(
            f'    <testsuite name="{classname}" tests="{len(cases)}"'
            ' disabled="0" errors="0" failures="0">'
        )
        for name, body in cases:
            parts.append(
                f'        <testcase name="{name}" classname="{classname}"'
                f' timestamp="t" time="0.01">{body}</testcase>'
            )
        parts.append("    </testsuite>")
    parts.append("</testsuites>")
    return "\n".join(parts) + "\n"


VALID_SUITES = {
    "demo": suite(
        "demo",
        binary="demo",
        kind="lib",
        cases={
            "storage::safety::tests::alpha": selected(),
            "utils::scan::scan_utils_tests::beta": selected(),
            "other::gamma": mismatch(),
        }
    ),
    "demo::demo_selection_test": suite(
        "demo::demo_selection_test",
        binary="demo_selection_test",
        kind="test",
        cases={"delta_runs_and_passes": selected()},
    ),
}

VALID_JUNIT = junit_report(
    [
        ("demo", [("storage::safety::tests::alpha", ""), ("utils::scan::scan_utils_tests::beta", "")]),
        ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
    ]
)


class ManifestSchema(unittest.TestCase):
    def parse(self, text):
        return ctc.parse_manifest(text)

    def test_rejects_non_json(self):
        with self.assertRaises(ctc.CoverageError):
            self.parse("not json")

    def test_rejects_non_object(self):
        with self.assertRaises(ctc.CoverageError):
            self.parse("[]")

    def test_rejects_missing_rust_suites(self):
        with self.assertRaises(ctc.CoverageError):
            self.parse('{"test-count": 0}')

    def test_rejects_duplicate_json_keys(self):
        with self.assertRaisesRegex(ctc.CoverageError, "duplicate JSON key"):
            self.parse('{"rust-suites": {}, "rust-suites": null}')
        with self.assertRaisesRegex(ctc.CoverageError, "duplicate JSON key"):
            self.parse(manifest_json({}).replace("}}", '}, "test-count": 1}'))

    def test_rejects_missing_filter_match(self):
        bad = json.loads(manifest_json({"demo": suite("demo", binary="demo", cases={"alpha": {"ignored": False}})}))
        with self.assertRaisesRegex(ctc.CoverageError, "lacks 'filter-match'"):
            self.parse(json.dumps(bad))

    def test_rejects_unknown_filter_match_status_despite_valid_rows(self):
        cases = {
            "storage::safety::tests::alpha": selected(),
            "beta": {"ignored": False, "filter-match": {"status": "maybe"}},
        }
        with self.assertRaisesRegex(ctc.CoverageError, "unrecognized filter-match status 'maybe'"):
            self.parse(manifest_json({"demo": suite("demo", binary="demo", cases=cases)}))

    def test_rejects_missing_filter_match_status(self):
        cases = {"alpha": {"ignored": False, "filter-match": {"reason": "expression"}}}
        with self.assertRaisesRegex(ctc.CoverageError, "filter-match status None"):
            self.parse(manifest_json({"demo": suite("demo", binary="demo", cases=cases)}))

    def test_rejects_missing_ignored(self):
        cases = {"alpha": {"filter-match": {"status": "matches"}}}
        with self.assertRaisesRegex(ctc.CoverageError, "boolean 'ignored'"):
            self.parse(manifest_json({"demo": suite("demo", binary="demo", cases=cases)}))

    def test_rejects_non_bool_ignored(self):
        for bad_ignored in ("false", 0, None, []):
            cases = {"alpha": {"ignored": bad_ignored, "filter-match": {"status": "matches"}}}
            with self.assertRaisesRegex(ctc.CoverageError, "boolean 'ignored'"):
                self.parse(manifest_json({"demo": suite("demo", binary="demo", cases=cases)}))

    def test_rejects_missing_or_invalid_identity_fields(self):
        for mutate in (
            lambda s: s.pop("package-name"),
            lambda s: s.pop("binary-name"),
            lambda s: s.pop("binary-id"),
            lambda s: s.update({"package-name": ""}),
            lambda s: s.update({"binary-name": 3}),
            lambda s: s.update({"binary-id": ""}),
            lambda s: s.update({"binary-id": ["demo"]}),
        ):
            bad = json.loads(manifest_json({"demo": suite("demo", binary="demo", cases={"alpha": selected()})}))
            mutate(bad["rust-suites"]["demo"])
            with self.assertRaisesRegex(ctc.CoverageError, "non-empty string"):
                self.parse(json.dumps(bad))

    def test_rejects_unknown_suite_status_even_without_selection(self):
        bad = suite("demo", binary="demo", status="maybe", cases={"alpha": mismatch()})
        with self.assertRaisesRegex(ctc.CoverageError, "unrecognized status 'maybe'"):
            self.parse(manifest_json({"demo": bad}))

    def test_rejects_selected_test_in_skipped_suite(self):
        bad = suite("demo", binary="demo", status="skipped", cases={"alpha": selected()})
        with self.assertRaisesRegex(ctc.CoverageError, "unexpected status 'skipped'"):
            self.parse(manifest_json({"demo": bad}))

    def test_accepts_skipped_suite_without_selection(self):
        ok = suite("demo", binary="demo", status="skipped", cases={"alpha": mismatch()})
        expected, _ = self.parse(manifest_json({"demo": ok}))
        self.assertEqual(expected, {})

    def test_rejects_duplicate_canonical_suite_identities(self):
        # Two suite keys cannot share one canonical binary id: the second
        # entry's key disagrees with the authoritative binary-id it repeats,
        # and a same-name testcase would collide on the derived identity.
        first = suite("demo::dup_test", binary="dup_test", kind="test", cases={"alpha": selected()})
        second = suite("demo::dup_test", binary="dup_test", kind="test", cases={"alpha": selected()})
        with self.assertRaisesRegex(ctc.CoverageError, "binary-id"):
            self.parse(manifest_json({"first": first, "second": second}))

    def test_rejects_suite_key_binary_id_disagreement(self):
        # Aliases are not identities: neither the legacy package::binary
        # derivation for a bin-kind target nor any other string (e.g. the
        # suite's binary-path) may stand in for the canonical suite key.
        for bad_id in ("demo::demo-agent-wrapper", "/demo/target/ci/deps/x", "demo"):
            bad = bin_suite()
            bad["binary-id"] = bad_id
            with self.assertRaisesRegex(
                ctc.CoverageError, "does not match its authoritative binary-id"
            ):
                self.parse(manifest_json({BIN_KEY: bad}))

    def test_accepts_additive_fields_at_every_level(self):
        data = json.loads(manifest_json(dict(VALID_SUITES)))
        data["future-top"] = {"anything": True}
        data["rust-suites"]["demo"]["future-suite"] = [1, 2]
        data["rust-suites"]["demo"]["testcases"]["storage::safety::tests::alpha"]["future-case"] = "x"
        data["rust-suites"]["demo"]["testcases"]["storage::safety::tests::alpha"]["filter-match"][
            "future-fm"
        ] = 0
        expected, ignored = self.parse(json.dumps(data))
        self.assertEqual(len(expected), 3)
        self.assertEqual(ignored, [])


class ManifestSelection(unittest.TestCase):
    def test_selected_ignored_flagged(self):
        bad = json.loads(manifest_json(dict(VALID_SUITES)))
        bad["rust-suites"]["demo"]["testcases"]["storage::safety::tests::alpha"]["ignored"] = True
        expected, ignored_selected = ctc.parse_manifest(json.dumps(bad))
        self.assertEqual(len(expected), 3)
        self.assertEqual(ignored_selected, ["demo::storage::safety::tests::alpha"])

    def test_integration_suite_identity(self):
        expected, _ = ctc.parse_manifest(manifest_json(dict(VALID_SUITES)))
        self.assertIn(("demo::demo_selection_test", "delta_runs_and_passes"), expected)
        self.assertIn(("demo", "storage::safety::tests::alpha"), expected)

    def test_bin_kind_identity_is_the_canonical_suite_key(self):
        suites = copy.deepcopy(VALID_SUITES)
        suites[BIN_KEY] = bin_suite()
        expected, _ = ctc.parse_manifest(manifest_json(suites))
        self.assertIn((BIN_KEY, BIN_CASE), expected)


class BinKindPairing(unittest.TestCase):
    # Regression for the proven CI blocker: a bin-kind suite (canonical
    # rust-suites key == binary-id == JUnit classname, e.g.
    # lotar::bin/lotar-agent-wrapper) previously failed because the checker
    # derived `package::binary-name` identities instead.

    def problems(self, manifest, junit):
        expected, ignored_selected = ctc.parse_manifest(manifest)
        executed, duplicates = ctc.parse_junit(junit)
        return ctc.compare(expected, ignored_selected, executed, duplicates)

    def test_bin_kind_pairs_by_canonical_binary_id(self):
        # Failed before the fix (derived demo::demo-agent-wrapper identity);
        # pairs exactly now without any bin-specific exception in the checker.
        suites = copy.deepcopy(VALID_SUITES)
        suites[BIN_KEY] = bin_suite()
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", ""),
                        ("utils::scan::scan_utils_tests::beta", ""),
                    ],
                ),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
                (BIN_KEY, [(BIN_CASE, "")]),
            ]
        )
        self.assertEqual(self.problems(manifest_json(suites), junit), [])

    def test_bin_kind_classname_kind_drop_rejected(self):
        # The legacy derived classname (kind segment dropped) must fail in
        # both directions: canonical selection missing, dropped identity
        # reported as not selected.
        suites = copy.deepcopy(VALID_SUITES)
        suites[BIN_KEY] = bin_suite()
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", ""),
                        ("utils::scan::scan_utils_tests::beta", ""),
                    ],
                ),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
                ("demo::demo-agent-wrapper", [(BIN_CASE, "")]),
            ]
        )
        problems = self.problems(manifest_json(suites), junit)
        self.assertTrue(
            any(
                f"{BIN_KEY}::{BIN_CASE}" in p and "missing from JUnit" in p
                for p in problems
            )
        )
        self.assertTrue(
            any(
                f"demo::demo-agent-wrapper::{BIN_CASE}" in p
                and "not selected" in p
                for p in problems
            )
        )

    def test_unknown_future_binary_kind_pairs_via_suite_key(self):
        # No kind inventory: a hypothetical future target kind pairs purely
        # through its generated canonical suite key / binary-id.
        future_key = "demo::custom-kind/demo-tool"
        suites = copy.deepcopy(VALID_SUITES)
        suites[future_key] = suite(
            future_key, binary="demo-tool", kind="custom-kind", cases={"runs": selected()}
        )
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", ""),
                        ("utils::scan::scan_utils_tests::beta", ""),
                    ],
                ),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
                (future_key, [("runs", "")]),
            ]
        )
        self.assertEqual(self.problems(manifest_json(suites), junit), [])

    def test_same_name_across_lib_and_bin_cannot_alias(self):
        suites = copy.deepcopy(VALID_SUITES)
        suites["demo"]["testcases"][BIN_CASE] = selected()
        suites[BIN_KEY] = bin_suite()
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", ""),
                        ("utils::scan::scan_utils_tests::beta", ""),
                    ],
                ),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
                (BIN_KEY, [(BIN_CASE, "")]),
            ]
        )
        problems = self.problems(manifest_json(suites), junit)
        self.assertTrue(
            any(
                f"demo::{BIN_CASE}" in p and "missing from JUnit" in p for p in problems
            )
        )


class JUnitParsing(unittest.TestCase):
    def test_rejects_bad_xml(self):
        with self.assertRaises(ctc.CoverageError):
            ctc.parse_junit("<testsuites>")

    def test_rejects_wrong_root(self):
        with self.assertRaises(ctc.CoverageError):
            ctc.parse_junit("<html><body/></html>")

    def test_rejects_nameless_testcase(self):
        xml = '<testsuites><testsuite><testcase classname="x"/></testsuite></testsuites>'
        with self.assertRaises(ctc.CoverageError):
            ctc.parse_junit(xml)

    def test_detects_failure_error_and_skipped_children(self):
        xml = junit_report(
            [
                (
                    "demo",
                    [
                        ("a", "<failure type=\"assert\">boom</failure>"),
                        ("b", "<error type=\"panic\"/>"),
                        ("c", "<skipped/>"),
                    ],
                )
            ]
        )
        executed, _ = ctc.parse_junit(xml)
        self.assertEqual(executed[("demo", "a")], "failed")
        self.assertEqual(executed[("demo", "b")], "failed")
        self.assertEqual(executed[("demo", "c")], "skipped")

    def test_missing_classname_falls_back_to_suite_name(self):
        # Intentional legacy compatibility (documented): nextest's quick-junit
        # always writes classname; a producer that omits it is keyed by the
        # testsuite name, which still has to match a canonical identity
        # exactly or the pairing fails closed.
        xml = (
            '<testsuites><testsuite name="demo" tests="1">'
            '<testcase name="alpha"/></testsuite></testsuites>'
        )
        executed, _ = ctc.parse_junit(xml)
        self.assertEqual(executed, {("demo", "alpha"): "passed"})

    def test_detects_skipped_status_attribute(self):
        xml = junit_report([("demo", [("a", "")])]).replace(
            'time="0.01">', 'time="0.01" status="skipped">'
        )
        executed, _ = ctc.parse_junit(xml)
        self.assertEqual(executed[("demo", "a")], "skipped")


class Comparison(unittest.TestCase):
    def problems(self, manifest, junit):
        expected, ignored_selected = ctc.parse_manifest(manifest)
        executed, duplicates = ctc.parse_junit(junit)
        return ctc.compare(expected, ignored_selected, executed, duplicates)

    def test_success(self):
        self.assertEqual(self.problems(manifest_json(dict(VALID_SUITES)), VALID_JUNIT), [])

    def test_new_and_renamed_tests_track_manifest(self):
        suites = json.loads(manifest_json(dict(VALID_SUITES)))
        cases = suites["rust-suites"]["demo"]["testcases"]
        del cases["storage::safety::tests::alpha"]
        cases["storage::safety::tests::alpha_renamed"] = selected()
        problems = self.problems(json.dumps(suites), VALID_JUNIT)
        self.assertTrue(
            any("alpha_renamed" in p and "missing from JUnit" in p for p in problems)
        )
        self.assertTrue(
            any("storage::safety::tests::alpha" in p and "not selected" in p for p in problems)
        )

    def test_selected_ignored_reports_problem(self):
        suites = json.loads(manifest_json(dict(VALID_SUITES)))
        suites["rust-suites"]["demo"]["testcases"]["storage::safety::tests::alpha"] = selected(
            ignored=True
        )
        problems = self.problems(json.dumps(suites), VALID_JUNIT)
        self.assertTrue(any("ignored (JUnit would omit it)" in p for p in problems))

    def test_selected_ignored_and_omitted_by_junit_fails(self):
        # Realistic pairing from `nextest list --run-ignored all` plus the
        # report a real run writes: the run skips the ignored test, so JUnit
        # omits it entirely while every executed test stays green.
        suites = json.loads(manifest_json(dict(VALID_SUITES)))
        suites["rust-suites"]["demo"]["testcases"]["storage::safety::tests::alpha"] = selected(
            ignored=True
        )
        junit = junit_report(
            [
                ("demo", [("utils::scan::scan_utils_tests::beta", "")]),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
            ]
        )
        problems = self.problems(json.dumps(suites), junit)
        self.assertTrue(
            any(
                "selected test is ignored" in p
                and "demo::storage::safety::tests::alpha" in p
                for p in problems
            )
        )
        self.assertTrue(
            any(
                "storage::safety::tests::alpha" in p and "missing from JUnit" in p
                for p in problems
            )
        )

    def test_same_name_in_two_binaries_cannot_alias(self):
        # The same test name is selected in the lib binary and in an
        # integration binary; a JUnit record for one classname must never
        # satisfy the other binary's same-name selection.
        suites = json.loads(manifest_json(dict(VALID_SUITES)))
        suites["rust-suites"]["demo"]["testcases"]["shared_name"] = selected()
        suites["rust-suites"]["demo::demo_selection_test"]["testcases"]["shared_name"] = selected()
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", ""),
                        ("utils::scan::scan_utils_tests::beta", ""),
                        ("shared_name", ""),
                    ],
                ),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
            ]
        )
        problems = self.problems(json.dumps(suites), junit)
        self.assertTrue(
            any(
                "demo::demo_selection_test::shared_name" in p
                and "missing from JUnit" in p
                for p in problems
            )
        )

    def test_same_name_drift_to_other_binary_rejected(self):
        # Name drift across binaries: the manifest selects the test under the
        # integration binary, but the report carries the same name under a
        # classname nothing selected. Both directions must be flagged.
        suites = json.loads(manifest_json(dict(VALID_SUITES)))
        suites["rust-suites"]["demo::demo_selection_test"]["testcases"] = {"moved_case": selected()}
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", ""),
                        ("utils::scan::scan_utils_tests::beta", ""),
                    ],
                ),
                ("demo::demo_renamed_test", [("moved_case", "")]),
            ]
        )
        problems = self.problems(json.dumps(suites), junit)
        self.assertTrue(
            any(
                "demo::demo_selection_test::moved_case" in p
                and "missing from JUnit" in p
                for p in problems
            )
        )
        self.assertTrue(
            any(
                "demo::demo_renamed_test::moved_case" in p
                and "not selected" in p
                for p in problems
            )
        )

    def test_missing_selected_testcase(self):
        junit = junit_report([("demo", [("utils::scan::scan_utils_tests::beta", "")])])
        problems = self.problems(manifest_json(dict(VALID_SUITES)), junit)
        self.assertTrue(
            any(
                "storage::safety::tests::alpha" in p and "missing from JUnit" in p
                for p in problems
            )
        )

    def test_stale_extra_junit_record(self):
        junit = junit_report(
            [
                ("demo", [("storage::safety::tests::alpha", ""), ("old_name", "")]),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
            ]
        )
        problems = self.problems(manifest_json(dict(VALID_SUITES)), junit)
        self.assertTrue(any("old_name" in p and "not selected by manifest" in p for p in problems))

    def test_duplicate_junit_record(self):
        junit = junit_report(
            [
                ("demo", [("storage::safety::tests::alpha", ""), ("storage::safety::tests::alpha", "")]),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
            ]
        )
        problems = self.problems(manifest_json(dict(VALID_SUITES)), junit)
        self.assertTrue(any("duplicate JUnit record" in p for p in problems))

    def test_empty_report(self):
        junit = '<?xml version="1.0"?><testsuites name="nextest-run"></testsuites>'
        problems = self.problems(manifest_json(dict(VALID_SUITES)), junit)
        self.assertTrue(any("0 test cases" in p for p in problems))

    def test_empty_selection(self):
        data = json.loads(manifest_json(dict(VALID_SUITES)))
        for entry in data["rust-suites"].values():
            entry["testcases"] = {"other::gamma": mismatch()}
        problems = self.problems(json.dumps(data), VALID_JUNIT)
        self.assertTrue(any("selected 0 tests" in p for p in problems))

    def test_failed_junit_record(self):
        junit = junit_report(
            [
                (
                    "demo",
                    [
                        ("storage::safety::tests::alpha", "<failure type=\"assert\">boom</failure>"),
                        ("utils::scan::scan_utils_tests::beta", ""),
                    ],
                ),
                ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
            ]
        )
        problems = self.problems(manifest_json(dict(VALID_SUITES)), junit)
        self.assertTrue(
            any("JUnit test failed" in p and "storage::safety::tests::alpha" in p for p in problems)
        )


class EndToEnd(unittest.TestCase):
    def test_missing_files_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(
                ctc.run_check(str(Path(tmp) / "missing.json"), str(Path(tmp) / "missing.xml"), "x"),
                1,
            )
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(manifest_json(dict(VALID_SUITES)), encoding="utf-8")
            self.assertEqual(ctc.run_check(str(manifest), str(Path(tmp) / "missing.xml"), "x"), 1)

    def test_bom_manifest_accepted(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_bytes(b"\xef\xbb\xbf" + manifest_json(dict(VALID_SUITES)).encode())
            junit = Path(tmp) / "junit.xml"
            junit.write_text(VALID_JUNIT, encoding="utf-8")
            self.assertEqual(ctc.run_check(str(manifest), str(junit), "x"), 0)

    def test_success_exit_zero(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(manifest_json(dict(VALID_SUITES)), encoding="utf-8")
            junit = Path(tmp) / "junit.xml"
            junit.write_text(VALID_JUNIT, encoding="utf-8")
            self.assertEqual(ctc.run_check(str(manifest), str(junit), "x"), 0)

    def test_malformed_manifest_exit_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            cases = {
                "storage::safety::tests::alpha": selected(),
                "beta": {"ignored": False, "filter-match": {"status": "maybe"}},
            }
            manifest.write_text(manifest_json({"demo": suite("demo", binary="demo", cases=cases)}), encoding="utf-8")
            junit = Path(tmp) / "junit.xml"
            junit.write_text(VALID_JUNIT, encoding="utf-8")
            self.assertEqual(ctc.run_check(str(manifest), str(junit), "x"), 1)


    def test_bin_kind_pairing_exit_zero(self):
        # End-to-end form of the CI-blocker regression: the durable bin-kind
        # fixture pairs at the CLI level (exit 0). The pre-fix checker exits
        # 1 on the identical pair (see
        # target/test-fix-handoff/continuation-oct1/coverage-checker-fix/).
        with tempfile.TemporaryDirectory() as tmp:
            suites = copy.deepcopy(VALID_SUITES)
            suites[BIN_KEY] = bin_suite()
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(manifest_json(suites), encoding="utf-8")
            junit = Path(tmp) / "junit.xml"
            junit.write_text(
                junit_report(
                    [
                        (
                            "demo",
                            [
                                ("storage::safety::tests::alpha", ""),
                                ("utils::scan::scan_utils_tests::beta", ""),
                            ],
                        ),
                        ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
                        (BIN_KEY, [(BIN_CASE, "")]),
                    ]
                ),
                encoding="utf-8",
            )
            self.assertEqual(ctc.run_check(str(manifest), str(junit), "x"), 0)

    def test_realistic_ignored_pairing_exit_one(self):
        # End-to-end form of the ignored gap: the manifest (as listed with
        # --run-ignored all) selects an ignored test, the run's JUnit omits
        # it, and the check must fail despite all executed tests passing.
        with tempfile.TemporaryDirectory() as tmp:
            suites = json.loads(manifest_json(dict(VALID_SUITES)))
            suites["rust-suites"]["demo"]["testcases"]["storage::safety::tests::alpha"] = selected(
                ignored=True
            )
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(json.dumps(suites), encoding="utf-8")
            junit = Path(tmp) / "junit.xml"
            junit.write_text(
                junit_report(
                    [
                        ("demo", [("utils::scan::scan_utils_tests::beta", "")]),
                        ("demo::demo_selection_test", [("delta_runs_and_passes", "")]),
                    ]
                ),
                encoding="utf-8",
            )
            self.assertEqual(ctc.run_check(str(manifest), str(junit), "x"), 1)


if __name__ == "__main__":
    unittest.main()
