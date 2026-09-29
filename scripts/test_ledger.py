#!/usr/bin/env python3
"""Unit tests for scripts/ledger.py — focused on sub-file artifacts.

Run with:  python3 -m unittest scripts/test_ledger.py
       or:  python3 scripts/test_ledger.py

Tests monkeypatch the ledger module's path globals at call time to a per-test
temp directory and build a tiny fake upstream (a `Makefile`, matching the
`meta.build` rule), so the real gitignored `pokeemerald/` tree is not required.
"""

import argparse
import contextlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

_LEDGER_PY = Path(__file__).resolve().parent / "ledger.py"
_spec = importlib.util.spec_from_file_location("ledger_under_test", _LEDGER_PY)
ledger = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ledger)

PROJECT = "pokeemerald"


def ns(**kw):
    return argparse.Namespace(**kw)


class LedgerTestBase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        # Redirect all path globals to the temp sandbox.
        self._saved = {
            "REPO_ROOT": ledger.REPO_ROOT,
            "LEDGER_DIR": ledger.LEDGER_DIR,
            "root": ledger.PROJECTS[PROJECT]["root"],
        }
        ledger.REPO_ROOT = self.root
        ledger.LEDGER_DIR = self.root / "ledger"
        ledger.PROJECTS[PROJECT]["root"] = self.root / PROJECT
        # Fake upstream: one file that matches the meta.build rule.
        (self.root / PROJECT).mkdir(parents=True)
        (self.root / PROJECT / "Makefile").write_text("all:\n", encoding="utf-8")

    def tearDown(self):
        ledger.REPO_ROOT = self._saved["REPO_ROOT"]
        ledger.LEDGER_DIR = self._saved["LEDGER_DIR"]
        ledger.PROJECTS[PROJECT]["root"] = self._saved["root"]
        self._tmp.cleanup()

    # -- helpers ---------------------------------------------------------------

    def write_ledger(self, files, schema_version=ledger.SCHEMA_VERSION):
        """Write a ledger JSON directly (bypasses save-time validation)."""
        ledger.LEDGER_DIR.mkdir(parents=True, exist_ok=True)
        data = {"schema_version": schema_version, "project": PROJECT, "files": files}
        ledger.ledger_path(PROJECT).write_text(
            json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def read_ledger(self):
        return json.loads(ledger.ledger_path(PROJECT).read_text(encoding="utf-8"))

    def touch_repo(self, rel):
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("// stub\n", encoding="utf-8")

    def capture(self, fn, *a, **k):
        # Redirect stderr too: several commands (e.g. `gaps`) print a summary
        # line to stderr, which would otherwise leak to the test console.
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
            fn(*a, **k)
        return buf.getvalue()


# ── 1. validation ────────────────────────────────────────────────────────────

class TestValidation(LedgerTestBase):
    def _parent(self, artifacts):
        return {"status": "rewritten", "category": "code.source", "kind": "file",
                "spec": "S-6", "reason": "rest of file",
                "rust_target": "crates/battle/src/main.rs", "artifacts": artifacts}

    def test_valid_terminal_artifact(self):
        e = self._parent({"gT": {"status": "ported", "spec": "S-6",
                                 "reason": "type chart",
                                 "rust_target": "crates/assets/src/tc.rs"}})
        self.assertIsNone(ledger.validate_entry(e))

    def test_valid_pending_artifact_on_pending_parent(self):
        e = {"status": "pending", "category": "code.source", "kind": "file",
             "artifacts": {"gT": {"status": "pending"}}}
        self.assertIsNone(ledger.validate_entry(e))

    def test_ported_artifact_missing_rust_target(self):
        e = self._parent({"gT": {"status": "ported", "spec": "S-6", "reason": "r"}})
        self.assertIn("rust_target", ledger.validate_entry(e))

    def test_vague_fold_target(self):
        e = self._parent({"gT": {"status": "folded", "spec": "S-6", "reason": "r",
                                 "fold_target": "various"}})
        self.assertIn("concrete", ledger.validate_entry(e))

    def test_hash_in_name(self):
        e = self._parent({"a#b": {"status": "pending"}})
        self.assertIn("#", ledger.validate_entry(e))

    def test_artifacts_not_a_dict(self):
        e = self._parent(["not", "a", "dict"])
        self.assertIn("must be an object", ledger.validate_entry(e))

    def test_nested_artifacts_rejected(self):
        e = self._parent({"gT": {"status": "pending", "artifacts": {}}})
        self.assertIn("recursion", ledger.validate_entry(e))

    def test_pending_artifact_extra_keys(self):
        e = self._parent({"gT": {"status": "pending", "spec": "S-6"}})
        self.assertIn("must not carry", ledger.validate_entry(e))

    # -- v1 acceptance-ID vocabulary (issue #424) ---------------------------

    def test_accepted_spec_id_on_parent(self):
        e = {"status": "rewritten", "category": "code.source", "kind": "file",
             "spec": "S-6", "reason": "r", "rust_target": "x.rs"}
        self.assertIsNone(ledger.validate_entry(e))

    def test_legacy_spec_id_rejected_on_parent(self):
        e = {"status": "rewritten", "category": "code.source", "kind": "file",
             "spec": "06-engine", "reason": "r", "rust_target": "x.rs"}
        err = ledger.validate_entry(e)
        self.assertIsNotNone(err)
        self.assertIn("06-engine", err)

    def test_nonexistent_spec_id_rejected_on_parent(self):
        # Well-formed (prefix-number) but not a real acceptance ID.
        e = {"status": "rewritten", "category": "code.source", "kind": "file",
             "spec": "C-99", "reason": "r", "rust_target": "x.rs"}
        err = ledger.validate_entry(e)
        self.assertIsNotNone(err)
        self.assertIn("C-99", err)

    def test_legacy_spec_id_rejected_on_artifact(self):
        e = self._parent({"gT": {"status": "ported", "spec": "05-assets",
                                 "reason": "r", "rust_target": "x.rs"}})
        err = ledger.validate_entry(e)
        self.assertIsNotNone(err)
        self.assertIn("05-assets", err)

    def test_valid_spec_owner_on_pending(self):
        e = {"status": "pending", "category": "code.source", "kind": "file",
             "spec_owner": "S-4"}
        self.assertIsNone(ledger.validate_entry(e))

    def test_legacy_spec_owner_rejected_on_pending(self):
        e = {"status": "pending", "category": "code.source", "kind": "file",
             "spec_owner": "05-assets"}
        err = ledger.validate_entry(e)
        self.assertIsNotNone(err)
        self.assertIn("spec_owner", err)
        self.assertIn("05-assets", err)


# ── 2. serialization / backward compat ───────────────────────────────────────

class TestSerialization(LedgerTestBase):
    def test_no_artifacts_byte_identical(self):
        ledger.save_ledger(PROJECT, {
            "schema_version": ledger.SCHEMA_VERSION, "project": PROJECT,
            "files": {"Makefile": {"category": "meta.build", "kind": "file",
                                   "status": "pending"}}})
        expected = (
            "{\n"
            '  "files": {\n'
            '    "Makefile": {\n'
            '      "category": "meta.build",\n'
            '      "kind": "file",\n'
            '      "status": "pending"\n'
            "    }\n"
            "  },\n"
            '  "project": "pokeemerald",\n'
            '  "schema_version": 3\n'
            "}\n"
        )
        self.assertEqual(
            ledger.ledger_path(PROJECT).read_text(encoding="utf-8"), expected)

    def test_two_artifacts_sorted(self):
        ledger.save_ledger(PROJECT, {
            "schema_version": ledger.SCHEMA_VERSION, "project": PROJECT,
            "files": {"src/battle_main.c": {
                "status": "rewritten", "category": "code.source", "kind": "file",
                "spec": "S-6", "reason": "main loop",
                "rust_target": "crates/battle/src/main_loop.rs",
                "artifacts": {
                    "gTypeEffectiveness": {
                        "status": "ported", "spec": "S-6",
                        "reason": "type chart",
                        "rust_target": "crates/assets/src/type_chart.rs"},
                    "gBattleMoves": {
                        "status": "ported", "spec": "S-6",
                        "reason": "move table",
                        "rust_target": "crates/assets/src/moves.rs"},
                }}}})
        expected = (
            "{\n"
            '  "files": {\n'
            '    "src/battle_main.c": {\n'
            '      "artifacts": {\n'
            '        "gBattleMoves": {\n'
            '          "reason": "move table",\n'
            '          "rust_target": "crates/assets/src/moves.rs",\n'
            '          "spec": "S-6",\n'
            '          "status": "ported"\n'
            "        },\n"
            '        "gTypeEffectiveness": {\n'
            '          "reason": "type chart",\n'
            '          "rust_target": "crates/assets/src/type_chart.rs",\n'
            '          "spec": "S-6",\n'
            '          "status": "ported"\n'
            "        }\n"
            "      },\n"
            '      "category": "code.source",\n'
            '      "kind": "file",\n'
            '      "reason": "main loop",\n'
            '      "rust_target": "crates/battle/src/main_loop.rs",\n'
            '      "spec": "S-6",\n'
            '      "status": "rewritten"\n'
            "    }\n"
            "  },\n"
            '  "project": "pokeemerald",\n'
            '  "schema_version": 3\n'
            "}\n"
        )
        self.assertEqual(
            ledger.ledger_path(PROJECT).read_text(encoding="utf-8"), expected)

    def test_v2_reads_as_v3(self):
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}}, schema_version=2)
        data = ledger.load_ledger(PROJECT)
        self.assertEqual(data["files"]["Makefile"]["status"], "pending")


# ── 3. register / _set_entry ─────────────────────────────────────────────────

class TestRegister(LedgerTestBase):
    def setUp(self):
        super().setUp()
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}})

    def test_port_creates_artifact(self):
        self.capture(ledger.cmd_port, ns(
            project=PROJECT, path="Makefile#tbl", target="crates/a/src/tbl.rs",
            spec="S-4", reason="carved out"))
        e = self.read_ledger()["files"]["Makefile"]
        self.assertEqual(e["status"], "pending")  # parent untouched
        self.assertEqual(e["artifacts"]["tbl"]["status"], "ported")
        self.assertEqual(e["artifacts"]["tbl"]["rust_target"], "crates/a/src/tbl.rs")

    def test_marking_parent_preserves_artifact(self):
        self.capture(ledger.cmd_port, ns(
            project=PROJECT, path="Makefile#tbl", target="crates/a/src/tbl.rs",
            spec="S-4", reason="carved out"))
        self.capture(ledger.cmd_mark, ns(
            project=PROJECT, path="Makefile", target="crates/a/src/rest.rs",
            spec="S-4", reason="the rest"))
        e = self.read_ledger()["files"]["Makefile"]
        self.assertEqual(e["status"], "rewritten")
        self.assertIn("tbl", e["artifacts"])

    def test_unmark_artifact_resets_to_pending(self):
        self.capture(ledger.cmd_port, ns(
            project=PROJECT, path="Makefile#tbl", target="crates/a/src/tbl.rs",
            spec="S-4", reason="carved out"))
        self.capture(ledger.cmd_unmark, ns(project=PROJECT, path="Makefile#tbl"))
        e = self.read_ledger()["files"]["Makefile"]
        self.assertEqual(e["artifacts"]["tbl"], {"status": "pending"})

    def test_unmark_artifact_create_if_missing(self):
        self.capture(ledger.cmd_unmark, ns(project=PROJECT, path="Makefile#new"))
        e = self.read_ledger()["files"]["Makefile"]
        self.assertEqual(e["artifacts"]["new"], {"status": "pending"})


# ── 4. status counting / effective_status ────────────────────────────────────

class TestStatusCounting(LedgerTestBase):
    def test_effective_status_rules(self):
        es = ledger.effective_status
        self.assertEqual(es({"status": "pending"}), "pending")
        self.assertEqual(es({"status": "rewritten"}), "rewritten")
        # own terminal, artifact pending -> counts pending
        self.assertEqual(es({"status": "rewritten",
                             "artifacts": {"a": {"status": "pending"}}}), "pending")
        # own terminal, all artifacts terminal -> accounted (own status)
        self.assertEqual(es({"status": "rewritten",
                             "artifacts": {"a": {"status": "ported"}}}), "rewritten")
        # own pending always pending regardless of artifacts
        self.assertEqual(es({"status": "pending",
                             "artifacts": {"a": {"status": "ported"}}}), "pending")

    def test_status_counts_sum_to_total(self):
        self.write_ledger({
            "Makefile": {"category": "meta.build", "kind": "file",
                         "status": "rewritten", "spec": "s", "reason": "r",
                         "rust_target": "x.rs",
                         "artifacts": {"a": {"status": "pending"}}},
            "src/b.c": {"category": "code.source", "kind": "file",
                        "status": "pending"},
        })
        out = self.capture(ledger.cmd_status, ns(projects=[PROJECT]))
        # Makefile counts pending (pending artifact); src/b.c pending.
        self.assertIn("total=2", out)
        self.assertIn("pending=2", out)
        self.assertIn("rewritten=0", out)
        self.assertIn("artifacts: total=1 accounted=0 pending=1", out)

    def test_status_no_artifact_line_when_none(self):
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}})
        out = self.capture(ledger.cmd_status, ns(projects=[PROJECT]))
        self.assertNotIn("artifacts:", out)

    def test_report_uses_effective_status(self):
        # Own terminal + pending sub-artifact must NOT be reported accounted-for.
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "rewritten",
            "spec": "s", "reason": "r", "rust_target": "x.rs",
            "artifacts": {"a": {"status": "pending"}}}})
        out = self.capture(ledger.cmd_report, ns(project=PROJECT))
        # Headline must agree with status (0 accounted, 0.0%), not over-claim.
        self.assertIn("**0/1 accounted for** (0.0%)", out)
        # By-category coverage must also be 0.0%, not 100.0%.
        self.assertIn("| `meta.build` | 1 | 1 | 0 | 0.0% |", out)
        # By-spec must not count the still-pending parent.
        self.assertNotIn("## Accounted for, by spec", out)

    def test_report_accounts_when_all_terminal(self):
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "rewritten",
            "spec": "s", "reason": "r", "rust_target": "x.rs",
            "artifacts": {"a": {"status": "ported"}}}})
        out = self.capture(ledger.cmd_report, ns(project=PROJECT))
        self.assertIn("**1/1 accounted for** (100.0%)", out)
        self.assertIn("| `meta.build` | 1 | 0 | 1 | 100.0% |", out)
        self.assertIn("## Accounted for, by spec", out)
        self.assertIn("| `s` | 1 |", out)

    def test_categories_uses_effective_status(self):
        # Own terminal + pending sub-artifact must count as pending in the
        # per-category breakdown too, agreeing with status/report/gaps.
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "rewritten",
            "spec": "s", "reason": "r", "rust_target": "x.rs",
            "artifacts": {"a": {"status": "pending"}}}})
        out = self.capture(ledger.cmd_categories, ns(project=PROJECT))
        row = next(l for l in out.splitlines() if "meta.build" in l)
        _, total, pending, terminal = row.split()
        self.assertEqual((total, pending, terminal), ("1", "1", "0"))

    def test_categories_accounts_when_all_terminal(self):
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "rewritten",
            "spec": "s", "reason": "r", "rust_target": "x.rs",
            "artifacts": {"a": {"status": "ported"}}}})
        out = self.capture(ledger.cmd_categories, ns(project=PROJECT))
        row = next(l for l in out.splitlines() if "meta.build" in l)
        _, total, pending, terminal = row.split()
        self.assertEqual((total, pending, terminal), ("1", "0", "1"))


# ── 5. scan preservation ─────────────────────────────────────────────────────

class TestScanPreservation(LedgerTestBase):
    def test_scan_preserves_artifact(self):
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "pending",
            "artifacts": {"tbl": {"status": "ported", "spec": "S-1", "reason": "r",
                                  "rust_target": "x.rs"}}}})
        out = self.capture(ledger.cmd_scan, ns(project=PROJECT, prune=False))
        e = self.read_ledger()["files"]["Makefile"]
        self.assertIn("tbl", e.get("artifacts", {}))
        self.assertEqual(e["artifacts"]["tbl"]["status"], "ported")
        self.assertIn("0 added", out)

    def test_prune_keeps_artifact_bearing_entry(self):
        # ghost.c is not on disk; pending own-status but carries an artifact.
        self.write_ledger({
            "Makefile": {"category": "meta.build", "kind": "file",
                         "status": "pending"},
            "src/ghost.c": {"category": "code.source", "kind": "file",
                            "status": "pending",
                            "artifacts": {"g": {"status": "ported", "spec": "S-1",
                                                "reason": "r",
                                                "rust_target": "x.rs"}}}})
        self.capture(ledger.cmd_scan, ns(project=PROJECT, prune=True))
        files = self.read_ledger()["files"]
        self.assertIn("src/ghost.c", files)  # artifact-bearing entry survives


# ── 6. verify ────────────────────────────────────────────────────────────────

class TestVerify(LedgerTestBase):
    def _ledger_with_artifact(self, target):
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "pending",
            "artifacts": {"tbl": {"status": "ported", "spec": "S-1", "reason": "r",
                                  "rust_target": target}}}})

    def test_missing_artifact_target_fails(self):
        self._ledger_with_artifact("crates/a/src/missing.rs")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            with self.assertRaises(SystemExit) as cm:
                ledger.cmd_verify(ns(project=PROJECT))
        self.assertEqual(cm.exception.code, 1)
        self.assertIn("Makefile#tbl", buf.getvalue())

    def test_present_artifact_target_clean(self):
        self.touch_repo("crates/a/src/present.rs")
        self._ledger_with_artifact("crates/a/src/present.rs")
        out = self.capture(ledger.cmd_verify, ns(project=PROJECT))
        self.assertIn("resolve", out)

    def test_terminal_entry_missing_rust_target_fails(self):
        # A hand-edited or merge-mangled ledger with a terminal entry that
        # skipped `rust_target` must not slip past `verify` — it would
        # otherwise count as done for L-1 coverage despite validate_entry
        # rejecting the exact same object.
        self.write_ledger({"src/battle_main.c": {
            "category": "code.source", "kind": "file", "status": "rewritten",
            "spec": "S-1", "reason": "r"}})
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            with self.assertRaises(SystemExit) as cm:
                ledger.cmd_verify(ns(project=PROJECT))
        self.assertNotEqual(cm.exception.code, 0)
        self.assertIn("src/battle_main.c", str(cm.exception))
        self.assertIn("rust_target", str(cm.exception))

    def test_terminal_artifact_missing_rust_target_fails(self):
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "pending",
            "artifacts": {"tbl": {"status": "ported", "spec": "S-1",
                                  "reason": "r"}}}})
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_verify(ns(project=PROJECT))
        self.assertNotEqual(cm.exception.code, 0)
        self.assertIn("Makefile", str(cm.exception))
        self.assertIn("rust_target", str(cm.exception))

    def test_entry_missing_status_fails(self):
        # An entry that lost its `status` key entirely must fail verify, not
        # silently default to pending.
        self.write_ledger({"src/battle_main.c": {
            "category": "code.source", "kind": "file"}})
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_verify(ns(project=PROJECT))
        self.assertNotEqual(cm.exception.code, 0)
        self.assertIn("src/battle_main.c", str(cm.exception))
        self.assertIn("status", str(cm.exception))

    def test_artifact_missing_status_fails(self):
        self.write_ledger({"Makefile": {
            "category": "meta.build", "kind": "file", "status": "pending",
            "artifacts": {"tbl": {}}}})
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_verify(ns(project=PROJECT))
        self.assertNotEqual(cm.exception.code, 0)
        self.assertIn("Makefile", str(cm.exception))
        self.assertIn("status", str(cm.exception))

    def test_valid_ledger_still_passes(self):
        self.touch_repo("crates/a/src/present.rs")
        self.write_ledger({"src/battle_main.c": {
            "category": "code.source", "kind": "file", "status": "rewritten",
            "spec": "S-1", "reason": "r",
            "rust_target": "crates/a/src/present.rs"}})
        out = self.capture(ledger.cmd_verify, ns(project=PROJECT))
        self.assertIn("resolve", out)


# ── 7. gaps / inspect ────────────────────────────────────────────────────────

class TestGapsInspect(LedgerTestBase):
    def setUp(self):
        super().setUp()
        self.write_ledger({"src/battle_main.c": {
            "category": "code.source", "kind": "file", "status": "rewritten",
            "spec": "s", "reason": "r", "rust_target": "crates/b/src/main.rs",
            "artifacts": {
                "gDone": {"status": "ported", "spec": "s", "reason": "r",
                          "rust_target": "crates/a/src/done.rs"},
                "gTodo": {"status": "pending"}}}})

    def test_gaps_lists_pending_artifact(self):
        out = self.capture(ledger.cmd_gaps, ns(
            project=PROJECT, prefix="", category="", limit=0))
        self.assertIn("src/battle_main.c#gTodo", out)
        # The parent itself is terminal, so it must not appear as a plain gap.
        lines = [ln for ln in out.splitlines() if ln == "src/battle_main.c"]
        self.assertEqual(lines, [])

    def test_gaps_pending_parent_not_double_listed(self):
        # A pending parent with a pending artifact must surface once (the
        # parent path), not also as parent#artifact — avoids double-counting.
        self.write_ledger({"src/foo.c": {
            "category": "code.source", "kind": "file", "status": "pending",
            "artifacts": {"gTbl": {"status": "pending"}}}})
        out = self.capture(ledger.cmd_gaps, ns(
            project=PROJECT, prefix="", category="", limit=0))
        self.assertIn("src/foo.c", out.splitlines())
        self.assertNotIn("src/foo.c#gTbl", out)

    def test_inspect_file_lists_artifacts(self):
        out = self.capture(ledger.cmd_inspect, ns(
            project=PROJECT, path="src/battle_main.c"))
        self.assertIn("gDone", out)
        self.assertIn("gTodo", out)

    def test_inspect_artifact_shows_fields(self):
        out = self.capture(ledger.cmd_inspect, ns(
            project=PROJECT, path="src/battle_main.c#gDone"))
        self.assertIn("ported", out)
        self.assertIn("crates/a/src/done.rs", out)


# ── 8. help ───────────────────────────────────────────────────────────────

class TestHelp(unittest.TestCase):
    COMMANDS = (
        "scan", "status", "categories", "report", "inspect", "gaps",
        "mark", "port", "stub", "fold", "drop", "unmark", "verify",
        "audit", "migrate", "init",
    )

    def capture_help(self, *args):
        buf = io.StringIO()
        # argparse wraps to the terminal width; pin it so the byte budgets
        # below measure the same rendering everywhere.
        with mock.patch.dict(os.environ, {"COLUMNS": "80"}):
            with mock.patch.object(sys, "argv", ["ledger.py", *args]):
                with contextlib.redirect_stdout(buf):
                    with self.assertRaises(SystemExit) as cm:
                        ledger.main()
        self.assertEqual(cm.exception.code, 0)
        return buf.getvalue()

    def test_top_level_help_is_a_compact_command_index(self):
        out = self.capture_help("-h")
        self.assertLessEqual(len(out.splitlines()), 32)
        self.assertLessEqual(len(out.encode("utf-8")), 1_500)
        self.assertIn("ledger.py COMMAND -h", out)
        for command in self.COMMANDS:
            self.assertIn(command, out)
        self.assertNotIn("ENTRY SHAPE", out)
        self.assertNotIn("SUB-ARTIFACTS", out)

    def test_command_help_stays_focused(self):
        for command in self.COMMANDS:
            with self.subTest(command=command):
                out = self.capture_help(command, "-h")
                self.assertLessEqual(len(out.splitlines()), 40)
                self.assertLessEqual(len(out), 4_000)

    def assert_help_contains(self, command, text):
        self.assertIn(text, " ".join(self.capture_help(command, "-h").split()))

    def test_relevant_commands_explain_ledger_rules(self):
        self.assert_help_contains(
            "status",
            "own status and every sub-artifact status are terminal",
        )
        self.assert_help_contains(
            "gaps",
            "pending parent appears once",
        )
        self.assert_help_contains(
            "mark",
            "concrete Rust target, spec ID, and reason",
        )
        self.assert_help_contains(
            "unmark",
            "registers that artifact",
        )
        self.assert_help_contains("verify", "rust_target")
        self.assert_help_contains(
            "audit",
            "Suggestions do not modify the ledger",
        )


# ── 9. migrate ───────────────────────────────────────────────────────────────

class TestMigrate(LedgerTestBase):
    def test_v2_to_v3_pure_bump(self):
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}}, schema_version=2)
        self.capture(ledger.cmd_migrate, ns())
        data = self.read_ledger()
        self.assertEqual(data["schema_version"], 3)
        self.assertEqual(data["files"]["Makefile"]["status"], "pending")

    # -- legacy spec_owner remap (issue #424) -------------------------------

    def test_migrate_remaps_legacy_spec_owner_on_current_schema(self):
        self.write_ledger({
            "graphics/fonts/a.png": {"category": "graphics.font", "kind": "file",
                                     "status": "pending",
                                     "spec_owner": "02-rendering"},
            "data/maps/x": {"category": "data.map", "kind": "dir",
                            "status": "pending", "spec_owner": "05-assets"},
            "src/b.c": {"category": "code.source", "kind": "file",
                        "status": "pending"},
        })
        out = self.capture(ledger.cmd_migrate, ns())
        self.assertIn("remapped 2", out)
        files = self.read_ledger()["files"]
        self.assertEqual(files["graphics/fonts/a.png"]["spec_owner"], "S-2")
        self.assertEqual(files["data/maps/x"]["spec_owner"], "S-4")
        self.assertNotIn("spec_owner", files["src/b.c"])

    def test_migrate_no_op_when_already_current_and_no_legacy_owners(self):
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}})
        out = self.capture(ledger.cmd_migrate, ns())
        self.assertIn("already at v3", out)


# ── 10. audit spec validation ────────────────────────────────────────────────

class TestAuditSpecValidation(LedgerTestBase):
    def setUp(self):
        super().setUp()
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}})

    def test_audit_rejects_nonexistent_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_audit(ns(project=PROJECT, spec_id="C-99",
                                base="HEAD", rev="HEAD"))
        self.assertIn("C-99", str(cm.exception))

    def test_audit_rejects_legacy_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_audit(ns(project=PROJECT, spec_id="06-engine",
                                base="HEAD", rev="HEAD"))
        self.assertIn("06-engine", str(cm.exception))

    def test_audit_accepts_valid_spec(self):
        # git() is stubbed so this stays independent of whether the sandbox
        # temp dir is itself a git repo -- only spec validation is under test.
        with mock.patch.object(ledger, "git", return_value=""):
            out = self.capture(ledger.cmd_audit, ns(
                project=PROJECT, spec_id="S-6", base="HEAD", rev="HEAD"))
        self.assertIn("Audit", out)
        self.assertIn("S-6", out)


# ── 11. marking commands validate spec (issue #424) ──────────────────────────

class TestMarkingCommandsSpecValidation(LedgerTestBase):
    def setUp(self):
        super().setUp()
        self.write_ledger({"Makefile": {"category": "meta.build", "kind": "file",
                                        "status": "pending"}})

    def test_mark_rejects_nonexistent_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_mark(ns(project=PROJECT, path="Makefile",
                               target="crates/a/src/x.rs", spec="C-99",
                               reason="r"))
        self.assertIn("C-99", str(cm.exception))
        # Rejected before any write -- entry stays pending.
        self.assertEqual(self.read_ledger()["files"]["Makefile"]["status"],
                          "pending")

    def test_port_rejects_legacy_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_port(ns(project=PROJECT, path="Makefile",
                               target="crates/a/src/x.rs", spec="05-assets",
                               reason="r"))
        self.assertIn("05-assets", str(cm.exception))

    def test_stub_rejects_invalid_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_stub(ns(project=PROJECT, path="Makefile",
                               target="crates/a/src/x.rs", spec="bogus",
                               reason="r"))
        self.assertIn("bogus", str(cm.exception))

    def test_fold_rejects_invalid_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_fold(ns(project=PROJECT, path="Makefile",
                               into="crates/a::mod", spec="bogus",
                               reason="r"))
        self.assertIn("bogus", str(cm.exception))

    def test_drop_rejects_invalid_spec(self):
        with self.assertRaises(SystemExit) as cm:
            ledger.cmd_drop(ns(project=PROJECT, path="Makefile",
                               spec="bogus", reason="r"))
        self.assertIn("bogus", str(cm.exception))

    def test_mark_accepts_valid_spec(self):
        self.capture(ledger.cmd_mark, ns(
            project=PROJECT, path="Makefile", target="crates/a/src/x.rs",
            spec="S-1", reason="r"))
        e = self.read_ledger()["files"]["Makefile"]
        self.assertEqual(e["status"], "rewritten")
        self.assertEqual(e["spec"], "S-1")


# ── 12. acceptance-ID vocabulary (issue #424) ────────────────────────────────

class TestAcceptanceIdVocabulary(unittest.TestCase):
    def test_exactly_48_ids_parsed_from_v1_doc(self):
        self.assertEqual(len(ledger.ACCEPTANCE_IDS), 48)

    def test_legacy_hints_are_not_valid_ids(self):
        for legacy in ledger.LEGACY_SPEC_OWNER_MAP:
            self.assertNotIn(legacy, ledger.ACCEPTANCE_IDS)

    def test_invalid_spec_id_accepts_real_id(self):
        self.assertIsNone(ledger.invalid_spec_id("S-1"))

    def test_invalid_spec_id_rejects_legacy(self):
        self.assertIsNotNone(ledger.invalid_spec_id("06-engine"))

    def test_invalid_spec_id_rejects_nonexistent(self):
        self.assertIsNotNone(ledger.invalid_spec_id("C-99"))

    def test_invalid_spec_id_rejects_unhashable_value_without_crashing(self):
        # A hand-mangled or merge-mangled ledger could carry a list/dict
        # where a string spec id belongs; membership testing on an
        # unhashable value must not raise (issue #424 review).
        err = ledger.invalid_spec_id(["S-1"])
        self.assertIsNotNone(err)
        self.assertIn("S-1", err)
        err = ledger.invalid_spec_id({"a": 1})
        self.assertIsNotNone(err)

    def test_invalid_spec_id_rejects_none_and_empty_string(self):
        self.assertIsNotNone(ledger.invalid_spec_id(None))
        self.assertIsNotNone(ledger.invalid_spec_id(""))

    def test_row_parser_ignores_id_shaped_text_outside_status_rows(self):
        # A line that merely starts with `| <ID> |` but is not a genuine
        # `| ID | criterion | marker |` row (e.g. a stray 2-column line, or
        # prose citing an ID) must not be mistaken for a criterion row.
        fake_doc = (
            "| C-99 | not a real criterion row (no status column) |\n"
            "some prose mentioning | C-98 | in passing, not a table row\n"
            "| F-1 | a real-shaped row | ☑ |\n"
        )
        with tempfile.NamedTemporaryFile(
                mode="w", suffix=".md", delete=False, encoding="utf-8") as f:
            f.write(fake_doc)
            path = Path(f.name)
        try:
            ids = ledger._parse_acceptance_ids(path)
        finally:
            path.unlink()
        self.assertEqual(ids, {"F-1"})

    def test_all_rule_spec_owners_are_valid_ids(self):
        # Every POKEEMERALD_RULES hint must itself be a real v1 ID -- this is
        # the regression guard against a rule reintroducing a legacy or
        # made-up owner string.
        for rule in ledger.POKEEMERALD_RULES:
            if rule.spec_owner is not None:
                self.assertIsNone(
                    ledger.invalid_spec_id(rule.spec_owner),
                    f"{rule.category}: {rule.spec_owner!r}")


if __name__ == "__main__":
    unittest.main()
