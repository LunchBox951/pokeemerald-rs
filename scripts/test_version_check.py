#!/usr/bin/env python3
"""Unit tests for scripts/version_check.py — focused on the FINAL gate.

Run with:  python3 -m unittest scripts/test_version_check.py
       or:  python3 scripts/test_version_check.py

Two layers of coverage:

- ``TestCheckTransition`` / ``TestParseMarker`` exercise the pure validation
  functions directly with in-memory marker contents (fast, no git needed).
- ``TestFinalGateIntegration`` reproduces the issue's exact repro steps
  against a real throwaway git repository, driving ``main()`` end to end
  the same way CI invokes the script (``--base <ref> --head HEAD``).
"""

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

_VERSION_CHECK_PY = Path(__file__).resolve().parent / "version_check.py"
_spec = importlib.util.spec_from_file_location(
    "version_check_under_test", _VERSION_CHECK_PY
)
version_check = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(version_check)

MARKER = version_check.DEFAULT_FINAL_GATE_MARKER


def marker(version: str, when: str = "2026-07-25") -> str:
    """Build valid marker file contents approving ``version``."""
    return f"Approved version: {version}\nDate: {when}\n"


class TestParseMarker(unittest.TestCase):
    def test_valid_marker(self):
        approved, when = version_check.parse_marker(marker("1.0.0.0"), MARKER)
        self.assertEqual(approved, (1, 0, 0, 0))
        self.assertEqual(when, "2026-07-25")

    def test_case_insensitive_keys_and_extra_lines(self):
        raw = "Notes: signed off in #129\napproved VERSION: 2.0.0.0\ndate: 2026-01-01\n"
        approved, when = version_check.parse_marker(raw, MARKER)
        self.assertEqual(approved, (2, 0, 0, 0))
        self.assertEqual(when, "2026-01-01")

    def test_missing_approved_version_line(self):
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker("Date: 2026-07-25\n", MARKER)

    def test_missing_date_line(self):
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker("Approved version: 1.0.0.0\n", MARKER)

    def test_malformed_approved_version(self):
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(marker("not-a-version"), MARKER)

    def test_malformed_date(self):
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(marker("1.0.0.0", "07/25/2026"), MARKER)


class TestCheckTransition(unittest.TestCase):
    """Direct, git-free coverage of the FINAL gate's four failure modes."""

    def test_final_bump_without_marker_fails(self):
        with self.assertRaises(version_check.VersionError) as ctx:
            version_check.check_transition(
                (0, 9, 9, 9),
                (1, 0, 0, 0),
                marker_head=None,
                marker_base=None,
                marker_rel=MARKER,
            )
        self.assertIn("without the", str(ctx.exception))

    def test_final_bump_with_matching_new_marker_succeeds(self):
        # Marker introduced in this change (absent at base), naming the
        # exact proposed version -- must not raise.
        version_check.check_transition(
            (0, 9, 9, 9),
            (1, 0, 0, 0),
            marker_head=marker("1.0.0.0"),
            marker_base=None,
            marker_rel=MARKER,
        )

    def test_final_bump_with_unchanged_stale_marker_fails(self):
        # A marker approving 1.0.0.0 already existed at base (from an
        # earlier, separate approval) and head carries the byte-identical
        # marker -- even though it happens to name the version head is
        # bumping to, it was never touched by *this* diff, so it is stale
        # and must still be rejected (this is the core of issue #129).
        stale = marker("1.0.0.0")
        with self.assertRaises(version_check.VersionError) as ctx:
            version_check.check_transition(
                (0, 9, 9, 9),
                (1, 0, 0, 0),
                marker_head=stale,
                marker_base=stale,
                marker_rel=MARKER,
            )
        self.assertIn("unchanged from the", str(ctx.exception))

    def test_final_bump_with_wrong_version_marker_fails(self):
        with self.assertRaises(version_check.VersionError) as ctx:
            version_check.check_transition(
                (1, 0, 0, 0),
                (2, 0, 0, 0),
                marker_head=marker("1.0.0.0"),
                marker_base=marker("1.0.0.0"),
                marker_rel=MARKER,
            )
        self.assertIn("approves", str(ctx.exception))

    def test_final_bump_with_malformed_marker_fails(self):
        with self.assertRaises(version_check.VersionError):
            version_check.check_transition(
                (0, 9, 9, 9),
                (1, 0, 0, 0),
                marker_head="not a marker at all",
                marker_base=None,
                marker_rel=MARKER,
            )

    def test_non_final_change_ignores_marker_entirely(self):
        # A MINOR bump must not require (or even look at) the FINAL marker.
        version_check.check_transition(
            (0, 1, 2, 5),
            (0, 1, 3, 0),
            marker_head=None,
            marker_base=None,
            marker_rel=MARKER,
        )

    def test_regression_still_rejected(self):
        with self.assertRaises(version_check.VersionError):
            version_check.check_transition(
                (0, 1, 2, 5),
                (0, 1, 1, 6),
                marker_head=None,
                marker_base=None,
                marker_rel=MARKER,
            )


class _TempGitRepo:
    """A throwaway git repo for driving version_check.main() end to end."""

    def __init__(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.path = Path(self._tmp.name)
        self._run("init", "-q", "-b", "main")
        self._run("config", "user.email", "test@example.invalid")
        self._run("config", "user.name", "Test")

    def cleanup(self):
        self._tmp.cleanup()

    def _run(self, *args):
        subprocess.run(
            ["git", *args], cwd=self.path, check=True, capture_output=True
        )

    def write(self, rel_path: str, content: str):
        p = self.path / rel_path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")

    def commit(self, tag: str, message: str = "commit"):
        self._run("add", "-A")
        self._run("commit", "-q", "-m", message)
        self._run("tag", tag)

    def run_version_check(self, base: str, head: str = "HEAD"):
        """Invoke version_check.main() with cwd set to the repo (like CI)."""
        old_cwd = os.getcwd()
        os.chdir(self.path)
        try:
            return version_check.main(["--base", base, "--head", head])
        finally:
            os.chdir(old_cwd)


class TestFinalGateIntegration(unittest.TestCase):
    """Reproduces issue #129's exact scratch-repo scenario against real git."""

    def setUp(self):
        self.repo = _TempGitRepo()

    def tearDown(self):
        self.repo.cleanup()

    def test_stale_marker_no_longer_authorizes_later_final_bump(self):
        # Step 2: baseline at 0.9.9.9.
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")

        # Step 3: approved 1.0.0.0 bump with a fresh, matching marker.
        self.repo.write("VERSION", "1.0.0.0\n")
        self.repo.write(MARKER, marker("1.0.0.0"))
        self.repo.commit("approved-final")
        # This transition, from baseline, must succeed.
        self.assertEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )

        # Step 4: a later PR changes only VERSION to 2.0.0.0, leaving the
        # approval marker byte-identical (still naming 1.0.0.0).
        self.repo.write("VERSION", "2.0.0.0\n")
        self.repo.commit("unauthorized-bump")

        # Before the fix this returned 0 ("OK (1.0.0.0 -> 2.0.0.0)"); it must
        # now fail because the marker was never updated for this transition.
        self.assertNotEqual(
            self.repo.run_version_check(base="approved-final", head="HEAD"),
            0,
        )

    def test_matching_freshly_committed_marker_passes(self):
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")

        self.repo.write("VERSION", "1.0.0.0\n")
        self.repo.write(MARKER, marker("1.0.0.0"))
        self.repo.commit("approved-final")

        self.assertEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )

    def test_marker_naming_wrong_version_fails(self):
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")

        # Marker approves 1.0.0.0 but VERSION is bumped straight to 2.0.0.0.
        self.repo.write("VERSION", "2.0.0.0\n")
        self.repo.write(MARKER, marker("1.0.0.0"))
        self.repo.commit("mismatched-final")

        self.assertNotEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )

    def test_marker_unchanged_from_base_fails_even_if_version_matches(self):
        # A subtler case than the plain mismatch above: the marker at base
        # already names the *new* head version verbatim (e.g. carried over
        # from tooling, or pre-staged ahead of time) and head leaves it
        # byte-for-byte unchanged. Even though the version matches, it is
        # not evidence this specific transition was reviewed, so it must
        # still fail end to end through main().
        self.repo.write("VERSION", "1.0.0.0\n")
        self.repo.write(MARKER, marker("2.0.0.0"))
        self.repo.commit("baseline-with-preapproved-marker")

        self.repo.write("VERSION", "2.0.0.0\n")
        self.repo.commit("final-bump-marker-untouched")

        self.assertNotEqual(
            self.repo.run_version_check(
                base="baseline-with-preapproved-marker", head="HEAD"
            ),
            0,
        )

    def test_no_marker_at_all_fails(self):
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")

        self.repo.write("VERSION", "1.0.0.0\n")
        self.repo.commit("unapproved-final")

        self.assertNotEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )


if __name__ == "__main__":
    unittest.main()
