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
                marker_unchanged=False,
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
            marker_unchanged=False,
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
                marker_unchanged=True,
                marker_rel=MARKER,
            )
        self.assertIn("unchanged from the", str(ctx.exception))

    def test_final_bump_with_wrong_version_marker_fails(self):
        with self.assertRaises(version_check.VersionError) as ctx:
            version_check.check_transition(
                (1, 0, 0, 0),
                (2, 0, 0, 0),
                marker_head=marker("1.0.0.0"),
                marker_unchanged=True,
                marker_rel=MARKER,
            )
        self.assertIn("approves", str(ctx.exception))

    def test_final_bump_with_malformed_marker_fails(self):
        with self.assertRaises(version_check.VersionError):
            version_check.check_transition(
                (0, 9, 9, 9),
                (1, 0, 0, 0),
                marker_head="not a marker at all",
                marker_unchanged=False,
                marker_rel=MARKER,
            )

    def test_non_final_change_ignores_marker_entirely(self):
        # A MINOR bump must not require (or even look at) the FINAL marker.
        version_check.check_transition(
            (0, 1, 2, 5),
            (0, 1, 3, 0),
            marker_head=None,
            marker_unchanged=False,
            marker_rel=MARKER,
        )

    def test_regression_still_rejected(self):
        with self.assertRaises(version_check.VersionError):
            version_check.check_transition(
                (0, 1, 2, 5),
                (0, 1, 1, 6),
                marker_head=None,
                marker_unchanged=False,
                marker_rel=MARKER,
            )

    def test_required_bump_rejects_unchanged_version(self):
        with self.assertRaises(version_check.VersionError) as ctx:
            version_check.check_transition(
                (0, 1, 2, 5),
                (0, 1, 2, 5),
                marker_head=None,
                marker_unchanged=False,
                marker_rel=MARKER,
                require_bump=True,
            )
        self.assertIn("must advance VERSION", str(ctx.exception))

    def test_unchanged_version_remains_valid_without_required_bump(self):
        version_check.check_transition(
            (0, 1, 2, 5),
            (0, 1, 2, 5),
            marker_head=None,
            marker_unchanged=False,
            marker_rel=MARKER,
        )


class TestCargoVersion(unittest.TestCase):
    def test_maps_every_game_component(self):
        self.assertEqual(
            version_check.cargo_version((0, 0, 22, 7)),
            "0.0.22+gamepatch.7",
        )
        self.assertEqual(
            version_check.cargo_version((1, 0, 0, 0)),
            "1.0.0+gamepatch.0",
        )

    def test_replaces_only_workspace_package_version(self):
        manifest = (
            '[package]\nversion = "9.9.9"\n\n'
            '[workspace.package]\nedition = "2021"\nversion = "0.0.0"\n'
        )
        updated = version_check.replace_workspace_package_version(
            manifest, (0, 1, 2, 3), "Cargo.toml"
        )
        self.assertIn('[package]\nversion = "9.9.9"', updated)
        self.assertIn('version = "0.1.2+gamepatch.3"', updated)

    def test_rejects_missing_or_duplicate_workspace_versions(self):
        for manifest in (
            "[workspace.package]\nedition = \"2021\"\n",
            (
                "[workspace.package]\n"
                'version = "0.0.0"\nversion = "0.0.1"\n'
            ),
        ):
            with self.subTest(manifest=manifest), self.assertRaises(
                version_check.VersionError
            ):
                version_check.workspace_package_version(manifest, "Cargo.toml")


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

    def sync_cargo_manifest(self):
        version = version_check.parse_version(
            (self.path / "VERSION").read_text(encoding="utf-8"), "VERSION"
        )
        self.write(
            "Cargo.toml",
            "[workspace.package]\n"
            f'version = "{version_check.cargo_version(version)}"\n',
        )

    def commit(self, tag: str, message: str = "commit"):
        try:
            self.sync_cargo_manifest()
        except version_check.VersionError:
            pass
        self._run("add", "-A")
        self._run("commit", "-q", "-m", message)
        self._run("tag", tag)

    def run_version_check(
        self,
        base: str,
        head: str = "HEAD",
        *,
        mode: str = "transition",
        require_bump: bool = False,
    ):
        """Invoke version_check.main() with cwd set to the repo (like CI)."""
        old_cwd = os.getcwd()
        os.chdir(self.path)
        try:
            args = ["--mode", mode, "--base", base, "--head", head]
            if require_bump:
                args.append("--require-bump")
            return version_check.main(args)
        finally:
            os.chdir(old_cwd)

    def run_version_check_from_outside(self, base: str):
        """Invoke main() from a non-repo cwd with root pinned to this repo.

        Reproduces the fail-open scenario: if the base-side ``git show``
        runs in the caller's cwd instead of the resolved root, the base
        silently reads as absent and the gate passes when it must not.
        """
        outside = tempfile.TemporaryDirectory()
        old_cwd = os.getcwd()
        orig_repo_root = version_check.repo_root
        os.chdir(outside.name)
        version_check.repo_root = lambda: str(self.path)
        try:
            return version_check.main(["--base", base, "--head", "HEAD"])
        finally:
            version_check.repo_root = orig_repo_root
            os.chdir(old_cwd)
            outside.cleanup()


class TestFinalGateIntegration(unittest.TestCase):
    """Reproduces issue #129's exact scratch-repo scenario against real git."""

    def setUp(self):
        self.repo = _TempGitRepo()

    def tearDown(self):
        self.repo.cleanup()

    def test_workspace_package_version_must_match_version(self):
        self.repo.write("VERSION", "0.1.2.5\n")
        self.repo.commit("baseline")

        self.repo.write("VERSION", "0.1.2.6\n")
        self.assertNotEqual(
            self.repo.run_version_check(
                base="baseline", head="HEAD", require_bump=True
            ),
            0,
        )

        self.repo.sync_cargo_manifest()
        self.assertEqual(
            self.repo.run_version_check(
                base="baseline", head="HEAD", require_bump=True
            ),
            0,
        )

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

    def test_base_head_reads_the_commit_not_the_working_tree(self):
        # `--base HEAD` must compare against the committed VERSION. If both
        # sides read the working tree, an uncommitted regression (or FINAL
        # bump) validates against itself and always passes.
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("committed")

        self.repo.write("VERSION", "0.9.9.8\n")  # uncommitted regression

        self.assertNotEqual(
            self.repo.run_version_check(base="HEAD", head="HEAD"), 0
        )

    def test_pull_request_mode_requires_a_strict_version_increase(self):
        self.repo.write("VERSION", "0.1.2.5\n")
        self.repo.commit("baseline")

        self.assertEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )
        self.assertNotEqual(
            self.repo.run_version_check(
                base="baseline", head="HEAD", require_bump=True
            ),
            0,
        )

        self.repo.write("VERSION", "0.1.2.6\n")
        self.repo.sync_cargo_manifest()
        self.assertEqual(
            self.repo.run_version_check(
                base="baseline", head="HEAD", require_bump=True
            ),
            0,
        )

    def test_base_reads_follow_the_resolved_root_not_the_cwd(self):
        # Stale-marker scenario, invoked from a directory outside any
        # worktree: the base-side reads must still resolve against the
        # repository root, or the stale marker reads as newly-added and
        # the gate fails open.
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")
        self.repo.write("VERSION", "1.0.0.0\n")
        self.repo.write(MARKER, marker("1.0.0.0"))
        self.repo.commit("approved-final")
        self.repo.write("VERSION", "2.0.0.0\n")
        self.repo.commit("unauthorized-bump")

        self.assertNotEqual(
            self.repo.run_version_check_from_outside(base="approved-final"), 0
        )

    def test_symlinked_ancestor_directory_is_treated_as_absent(self):
        # A symlinked ANCESTOR of the marker path (docs/release -> stash)
        # lets open() read a pre-staged approval that `git show` cannot
        # see, desynchronizing the two sides. Any symlinked component must
        # fail closed.
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.write("stash/final-gate-approved.md", marker("1.0.0.0"))
        self.repo.commit("baseline-with-prestaged-blob")

        self.repo.write("VERSION", "1.0.0.0\n")
        release_dir = self.repo.path / "docs/release"
        release_dir.parent.mkdir(parents=True, exist_ok=True)
        release_dir.symlink_to(self.repo.path / "stash")
        self.repo.commit("symlinked-ancestor")

        self.assertNotEqual(
            self.repo.run_version_check(
                base="baseline-with-prestaged-blob", head="HEAD"
            ),
            0,
        )

    def test_ident_filter_does_not_launder_a_stale_marker(self):
        # With the `ident` attribute, checkout expands $Id$ so the working
        # tree bytes differ from the committed blob even though the marker
        # was never touched. Change detection must be filter-aware (git
        # diff), or the stale marker reads as fresh and fails the gate open.
        self.repo.write(".gitattributes", f"{MARKER} ident\n")
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.write(MARKER, marker("1.0.0.0") + "$Id$\n")
        self.repo.commit("baseline-with-ident-marker")

        # Simulate the smudged checkout state: $Id$ expanded in the
        # working tree while the committed blob still holds the bare $Id$.
        self.repo.write(
            MARKER,
            marker("1.0.0.0") + "$Id: 0123456789abcdef0123456789abcdef01234567 $\n",
        )
        self.repo.write("VERSION", "1.0.0.0\n")

        self.assertNotEqual(
            self.repo.run_version_check(
                base="baseline-with-ident-marker", head="HEAD"
            ),
            0,
        )

    def test_mode_only_flip_does_not_launder_a_stale_marker(self):
        # `chmod +x` flips git diff's verdict without touching a byte; the
        # staleness check must compare content, not tree metadata.
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.write(MARKER, marker("1.0.0.0"))
        self.repo.commit("baseline-with-stale-marker")

        self.repo.write("VERSION", "1.0.0.0\n")
        os.chmod(self.repo.path / MARKER, 0o755)
        self.repo.commit("chmod-only-final-bump")

        self.assertNotEqual(
            self.repo.run_version_check(
                base="baseline-with-stale-marker", head="HEAD"
            ),
            0,
        )

    def test_version_with_surrounding_whitespace_fails(self):
        # ' 1.0.0.0 ' parses identically after strip() but release tooling
        # tags the raw text -- the exact spelling must be committed.
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")

        self.repo.write("VERSION", " 1.0.0.0 \n")
        self.repo.write(MARKER, marker("1.0.0.0"))
        self.repo.commit("whitespace-version")

        self.assertNotEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )

    def test_symlinked_marker_is_treated_as_absent(self):
        # open() follows a symlink while `git show` returns its target path
        # text, so a symlinked marker reads "changed" without its bytes ever
        # changing. It must be treated as absent (fail closed).
        self.repo.write("VERSION", "0.9.9.9\n")
        self.repo.commit("baseline")

        self.repo.write("VERSION", "1.0.0.0\n")
        self.repo.write("docs/release/notes.md", marker("1.0.0.0"))
        marker_path = self.repo.path / MARKER
        marker_path.parent.mkdir(parents=True, exist_ok=True)
        marker_path.symlink_to(self.repo.path / "docs/release/notes.md")
        self.repo.commit("symlinked-marker")

        self.assertNotEqual(
            self.repo.run_version_check(base="baseline", head="HEAD"), 0
        )


class TestStrictTransitionPolicy(unittest.TestCase):
    """Pin the ordinary-PR policy independently of cumulative validation."""

    def test_valid_patch_and_reset_bumps(self):
        for base, head in (
            ((0, 1, 2, 5), (0, 1, 2, 6)),
            ((0, 1, 2, 5), (0, 1, 3, 0)),
            ((0, 1, 2, 5), (0, 2, 0, 0)),
        ):
            with self.subTest(base=base, head=head):
                version_check.check_transition(
                    base,
                    head,
                    marker_head=None,
                    marker_unchanged=False,
                    marker_rel=MARKER,
                    require_bump=True,
                )

    def test_invalid_minor_and_major_resets_fail(self):
        for head in ((0, 1, 3, 6), (0, 2, 1, 0), (0, 2, 0, 1)):
            with self.subTest(head=head), self.assertRaises(
                version_check.VersionError
            ):
                version_check.check_transition(
                    (0, 1, 2, 5),
                    head,
                    marker_head=None,
                    marker_unchanged=False,
                    marker_rel=MARKER,
                    require_bump=True,
                )

    def test_unchanged_and_regression_fail_when_bump_required(self):
        for head in ((0, 1, 2, 5), (0, 1, 2, 4)):
            with self.subTest(head=head), self.assertRaises(
                version_check.VersionError
            ):
                version_check.check_transition(
                    (0, 1, 2, 5),
                    head,
                    marker_head=None,
                    marker_unchanged=False,
                    marker_rel=MARKER,
                    require_bump=True,
                )

    def test_malformed_versions_remain_rejected(self):
        for raw in ("0.1.2", "v0.1.2.6", "0.01.2.6", "0.1.-2.6"):
            with self.subTest(raw=raw), self.assertRaises(
                version_check.VersionError
            ):
                version_check.parse_version(raw, "test")


class TestCumulativePolicy(unittest.TestCase):
    def setUp(self):
        self.repo = _TempGitRepo()

    def tearDown(self):
        self.repo.cleanup()

    def test_accepts_distant_accumulated_endpoints(self):
        for base, head in (
            ("0.0.0.0", "0.0.13.9"),
            ("0.0.13.8", "0.1.36.2"),
            ("1.0.0.0", "1.4.27.6"),
            ("0.9.9.9", "1.4.27.6"),
        ):
            with self.subTest(base=base, head=head):
                repo = _TempGitRepo()
                try:
                    repo.write("VERSION", f"{base}\n")
                    repo.commit("base")
                    repo.write("VERSION", f"{head}\n")
                    repo.commit("head")
                    self.assertEqual(
                        repo.run_version_check(
                            base="base", head="head", mode="cumulative"
                        ),
                        0,
                    )
                finally:
                    repo.cleanup()

    def test_equality_allowed_for_health_but_rejected_for_promotion(self):
        self.repo.write("VERSION", "0.1.36.2\n")
        self.repo.commit("same")
        self.assertEqual(
            self.repo.run_version_check(
                base="same", head="same", mode="cumulative"
            ),
            0,
        )
        self.assertNotEqual(
            self.repo.run_version_check(
                base="same",
                head="same",
                mode="cumulative",
                require_bump=True,
            ),
            0,
        )

    def test_regression_is_rejected(self):
        self.repo.write("VERSION", "0.1.36.2\n")
        self.repo.commit("base")
        self.repo.write("VERSION", "0.0.13.9\n")
        self.repo.commit("head")
        self.assertNotEqual(
            self.repo.run_version_check(
                base="base", head="head", mode="cumulative"
            ),
            0,
        )

    def test_malformed_endpoints_are_rejected(self):
        for base, head in (
            ("0.0.13.9", "0.1.36"),
            ("0.0.13", "0.1.36.2"),
        ):
            with self.subTest(base=base, head=head):
                repo = _TempGitRepo()
                try:
                    repo.write("VERSION", f"{base}\n")
                    repo.commit("base")
                    repo.write("VERSION", f"{head}\n")
                    repo.commit("head")
                    self.assertNotEqual(
                        repo.run_version_check(
                            base="base", head="head", mode="cumulative"
                        ),
                        0,
                    )
                finally:
                    repo.cleanup()

    def test_missing_base_ref_is_rejected(self):
        self.repo.write("VERSION", "0.1.36.2\n")
        self.repo.commit("head")
        self.assertNotEqual(
            self.repo.run_version_check(
                base="missing", head="head", mode="cumulative"
            ),
            0,
        )


class TestMarkerStrictness(unittest.TestCase):
    """Field shape rules: one line per field, strict date form."""

    def test_field_split_across_lines_is_rejected(self):
        raw = "Approved version:\n1.0.0.0\nDate: 2026-07-25\n"
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(raw, MARKER)

    def test_key_split_across_lines_is_rejected(self):
        raw = "Approved version\n: 1.0.0.0\nDate: 2026-07-25\n"
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(raw, MARKER)

    def test_compact_iso_date_is_rejected(self):
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(marker("1.0.0.0", "20260725"), MARKER)

    def test_week_date_is_rejected(self):
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(marker("1.0.0.0", "2026-W30-6"), MARKER)

    def test_duplicate_approved_version_lines_rejected(self):
        raw = (
            "Approved version: 2.0.0.0\n"
            "Approved version: 1.0.0.0\n"
            "Date: 2026-07-25\n"
        )
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(raw, MARKER)

    def test_duplicate_date_lines_rejected(self):
        raw = (
            "Approved version: 1.0.0.0\n"
            "Date: 2026-07-25\n"
            "Date: 2026-07-26\n"
        )
        with self.assertRaises(version_check.VersionError):
            version_check.parse_marker(raw, MARKER)

    def test_plain_date_still_accepted(self):
        approved, when = version_check.parse_marker(
            marker("1.0.0.0", "2026-07-25"), MARKER
        )
        self.assertEqual(approved, (1, 0, 0, 0))
        self.assertEqual(when, "2026-07-25")

    def test_crlf_marker_still_parses(self):
        # A marker committed with CRLF line endings is a legitimate
        # approval; \r must not break the line anchors.
        raw = "Approved version: 1.0.0.0\r\nDate: 2026-07-25\r\n"
        approved, when = version_check.parse_marker(raw, MARKER)
        self.assertEqual(approved, (1, 0, 0, 0))
        self.assertEqual(when, "2026-07-25")

    def test_leading_zero_version_component_rejected(self):
        # '01.0.0.0' would compare equal to the approved '1.0.0.0' as
        # integers while release tooling tags the raw text -- the spelling
        # itself must be canonical.
        with self.assertRaises(version_check.VersionError):
            version_check.parse_version("01.0.0.0", "test")
        with self.assertRaises(version_check.VersionError):
            version_check.parse_version("1.0.00.0", "test")
        # A lone zero component is fine.
        self.assertEqual(
            version_check.parse_version("0.1.0.0", "test"), (0, 1, 0, 0)
        )


if __name__ == "__main__":
    unittest.main()
