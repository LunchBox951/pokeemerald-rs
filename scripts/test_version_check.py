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

    def test_target_ahead_of_main_with_valid_forward_move_still_passes(self):
        self.commit_version("0.0.0.0", branch="origin/main")
        self.commit_version("0.0.2.0", branch="origin/dev")
        # A genuine forward move relative to the target must still pass.
        self.write_version("0.0.3.0")

        code, out, _ = self.run_check("origin/dev")
        self.assertEqual(code, 0)
        self.assertIn("0.0.2.0 -> 0.0.3.0", out)


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
