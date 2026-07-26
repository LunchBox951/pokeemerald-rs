#!/usr/bin/env python3
"""Unit tests for scripts/version_check.py.

Run with:  python3 -m unittest scripts/test_version_check.py
       or:  python3 scripts/test_version_check.py

Tests build a scratch git repository per test (so the real project history is
never touched) and drive ``version_check.main`` against it, mirroring the
harness style of ``scripts/test_ledger.py``.
"""

import contextlib
import importlib.util
import io
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


def _git(*args, cwd):
    subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    )


class VersionCheckTestBase(unittest.TestCase):
    """Builds a scratch git repo per test and chdirs into it.

    ``version_check.py`` resolves the repo root and reads base refs via
    ``git`` subprocess calls relative to the current working directory, so
    tests need a real repo on disk rather than a monkeypatched module global.
    """

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        _git("init", "-q", cwd=self.root)
        _git("config", "user.email", "test@example.com", cwd=self.root)
        _git("config", "user.name", "Test", cwd=self.root)
        self._prev_cwd = os.getcwd()
        os.chdir(self.root)

    def tearDown(self):
        os.chdir(self._prev_cwd)
        self._tmp.cleanup()

    def write_version(self, text):
        (self.root / "VERSION").write_text(text, encoding="utf-8")

    def commit_version(self, text, branch=None):
        """Write VERSION, commit it, and optionally point ``branch`` at it.

        ``branch`` may be a slash-qualified name such as ``origin/dev`` --
        git allows creating a local ref by that literal name, which resolves
        exactly like the remote-tracking ref version_check.py expects, without
        needing a real remote.
        """
        self.write_version(text)
        _git("add", "VERSION", cwd=self.root)
        _git("commit", "-q", "-m", "version", cwd=self.root)
        if branch is not None:
            _git("branch", "-f", branch, "HEAD", cwd=self.root)

    def run_check(self, base, head="HEAD"):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = version_check.main(["--base", base, "--head", head])
        return code, out.getvalue(), err.getvalue()

    def run_check_with_base_ref(self, base_ref, head="HEAD"):
        """Run main() with no --base, resolving the base from GITHUB_BASE_REF.

        Mirrors the real CI invocation, where the workflow passes no --base and
        the runner exports GITHUB_BASE_REF for pull_request events.
        """
        saved = os.environ.get("GITHUB_BASE_REF")
        os.environ["GITHUB_BASE_REF"] = base_ref
        out, err = io.StringIO(), io.StringIO()
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(
                err
            ):
                code = version_check.main(["--head", head])
        finally:
            if saved is None:
                os.environ.pop("GITHUB_BASE_REF", None)
            else:
                os.environ["GITHUB_BASE_REF"] = saved
        return code, out.getvalue(), err.getvalue()


class DivergentTargetRegressionTests(VersionCheckTestBase):
    """Pins the exact scenario from issue #128."""

    def test_lower_than_target_but_higher_than_main_is_rejected(self):
        # main = 0.0.0.0
        self.commit_version("0.0.0.0", branch="origin/main")
        # dev has already advanced ahead of main.
        self.commit_version("0.0.2.0", branch="origin/dev")
        # Proposed PR head resets to 0.0.1.0: above main, but a regression
        # against the actual merge target, dev.
        self.write_version("0.0.1.0")

        # Comparing only against origin/main -- the pre-fix CI invocation --
        # cannot see the regression and reports success.
        code, out, _ = self.run_check("origin/main")
        self.assertEqual(code, 0)
        self.assertIn("version_check: OK", out)

        # Comparing against the actual target branch must reject it.
        code, _, err = self.run_check("origin/dev")
        self.assertEqual(code, 1)
        self.assertIn("version regression", err)
        self.assertIn("0.0.1.0", err)
        self.assertIn("0.0.2.0", err)

    def test_base_ref_env_drives_rejection_without_explicit_base(self):
        # End-to-end down the real CI path: no --base flag, target selected
        # purely from GITHUB_BASE_REF=dev. The regression against dev must be
        # caught even though HEAD (0.0.1.0) is still above main (0.0.0.0).
        self.commit_version("0.0.0.0", branch="origin/main")
        self.commit_version("0.0.2.0", branch="origin/dev")
        self.write_version("0.0.1.0")

        code, _, err = self.run_check_with_base_ref("dev")
        self.assertEqual(code, 1)
        self.assertIn("version regression", err)
        self.assertIn("0.0.1.0", err)
        self.assertIn("0.0.2.0", err)

    def test_target_ahead_of_main_with_valid_forward_move_still_passes(self):
        self.commit_version("0.0.0.0", branch="origin/main")
        self.commit_version("0.0.2.0", branch="origin/dev")
        # A genuine forward move relative to the target must still pass.
        self.write_version("0.0.3.0")

        code, out, _ = self.run_check("origin/dev")
        self.assertEqual(code, 0)
        self.assertIn("0.0.2.0 -> 0.0.3.0", out)


class ResolveBaseTests(unittest.TestCase):
    """Base-ref selection logic.

    Issue #128's real fix moves the "which base ref does a PR compare against?"
    decision out of inline CI YAML (where it was untestable) into
    ``version_check.resolve_base``. These tests pin that decision directly: a PR
    targeting a branch ahead of main must resolve to that branch, not main.
    """

    def setUp(self):
        self._saved = os.environ.get("GITHUB_BASE_REF")
        os.environ.pop("GITHUB_BASE_REF", None)

    def tearDown(self):
        if self._saved is None:
            os.environ.pop("GITHUB_BASE_REF", None)
        else:
            os.environ["GITHUB_BASE_REF"] = self._saved

    def test_explicit_base_always_wins_over_env(self):
        os.environ["GITHUB_BASE_REF"] = "dev"
        self.assertEqual(
            version_check.resolve_base("origin/stable"), "origin/stable"
        )

    def test_pull_request_base_ref_selects_target_branch(self):
        # pull_request event targeting dev -> compare against origin/dev,
        # NOT origin/main. This is the #128 selection.
        os.environ["GITHUB_BASE_REF"] = "dev"
        self.assertEqual(version_check.resolve_base(None), "origin/dev")

    def test_release_base_ref_selects_target_branch(self):
        os.environ["GITHUB_BASE_REF"] = "release/1.2"
        self.assertEqual(
            version_check.resolve_base(None), "origin/release/1.2"
        )

    def test_push_event_without_base_ref_falls_back_to_main(self):
        # GITHUB_BASE_REF unset (push event) -> origin/main.
        self.assertEqual(version_check.resolve_base(None), "origin/main")

    def test_blank_base_ref_falls_back_to_main(self):
        os.environ["GITHUB_BASE_REF"] = ""
        self.assertEqual(version_check.resolve_base(None), "origin/main")


class WorkflowInvocationTests(unittest.TestCase):
    """Guards the CI workflow itself against re-introducing #128.

    The pre-fix workflow hard-coded ``--base origin/main``. If someone reverts
    the fix, the version step would once again compare every PR against main and
    the regression this slice closes would silently return. This test fails in
    exactly that case, pinning the fix at the workflow level -- not only at the
    ``version_check.py`` level.
    """

    _CI_YML = (
        Path(__file__).resolve().parent.parent
        / ".github"
        / "workflows"
        / "ci.yml"
    )

    def test_version_step_does_not_hardcode_main_as_base(self):
        ci = self._CI_YML.read_text(encoding="utf-8")
        self.assertNotIn("--base origin/main", ci)
        self.assertNotIn('--base "origin/main"', ci)

    def test_version_step_invokes_version_check(self):
        # The gate must still actually run; a resolver with no caller is no gate.
        ci = self._CI_YML.read_text(encoding="utf-8")
        self.assertIn("scripts/version_check.py", ci)


class UnchangedAndForwardTransitionTests(VersionCheckTestBase):
    """Sanity-checks that ordinary transitions are unaffected by the fix."""

    def test_unchanged_version_passes(self):
        self.commit_version("0.1.2.5", branch="origin/dev")
        self.write_version("0.1.2.5")

        code, out, _ = self.run_check("origin/dev")
        self.assertEqual(code, 0)
        self.assertIn("unchanged at 0.1.2.5", out)

    def test_patch_bump_passes(self):
        self.commit_version("0.1.2.5", branch="origin/dev")
        self.write_version("0.1.2.6")

        code, out, _ = self.run_check("origin/dev")
        self.assertEqual(code, 0)
        self.assertIn("0.1.2.5 -> 0.1.2.6", out)

    def test_minor_bump_without_reset_is_rejected(self):
        self.commit_version("0.1.2.5", branch="origin/dev")
        self.write_version("0.1.3.6")

        code, _, err = self.run_check("origin/dev")
        self.assertEqual(code, 1)
        self.assertIn("must reset", err)


if __name__ == "__main__":
    unittest.main()
