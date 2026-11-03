#!/usr/bin/env python3
"""Unit tests for scripts/release_tag.py (see its module docstring, F-5).

Run with:  python3 -m unittest scripts/test_release_tag.py
       or:  python3 scripts/test_release_tag.py

Drives release_tag.py against real, throwaway local git repositories -- a
bare "remote" plus a workspace clone -- so pushes, fetches, and tag peeling
exercise real git, not a mock.
"""

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

_RELEASE_TAG_PY = Path(__file__).resolve().parent / "release_tag.py"
_spec = importlib.util.spec_from_file_location(
    "release_tag_under_test", _RELEASE_TAG_PY
)
release_tag = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(release_tag)


class _BareRemote:
    """A throwaway bare git repo standing in for the GitHub remote."""

    def __init__(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.path = Path(self._tmp.name)
        subprocess.run(
            ["git", "init", "-q", "--bare", "-b", "main", str(self.path)],
            check=True,
            capture_output=True,
        )

    def cleanup(self):
        self._tmp.cleanup()


class _Workspace:
    """A clone-like local repo used to drive release_tag.main() end to end."""

    def __init__(self, remote: _BareRemote):
        self._tmp = tempfile.TemporaryDirectory()
        self.path = Path(self._tmp.name)
        self.remote = remote
        self._run("init", "-q", "-b", "main")
        self._run("config", "user.email", "test@example.invalid")
        self._run("config", "user.name", "Test")
        self._run("remote", "add", "origin", str(remote.path))

    def cleanup(self):
        self._tmp.cleanup()

    def _run(self, *args, check=True):
        return subprocess.run(
            ["git", *args],
            cwd=self.path,
            check=check,
            capture_output=True,
            text=True,
        )

    def commit(self, message: str = "commit") -> str:
        (self.path / "FILE").write_text(message, encoding="utf-8")
        self._run("add", "-A")
        self._run("commit", "-q", "-m", message, "--allow-empty")
        return self._run("rev-parse", "HEAD").stdout.strip()

    def push_main(self):
        self._run("push", "-q", "origin", "HEAD:refs/heads/main")

    def push_lightweight_tag(self, tag: str, sha: str):
        self._run("push", "-q", "origin", f"{sha}:refs/tags/{tag}")

    def push_annotated_tag(self, tag: str, sha: str, message: str = "ann"):
        self._run("tag", "-a", tag, sha, "-m", message)
        self._run("push", "-q", "origin", f"refs/tags/{tag}")
        self._run("tag", "-d", tag)

    def run_release_tag(self, tag: str, target: str, remote: str = "origin") -> int:
        """Invoke release_tag.main() with cwd set to this workspace (like CI)."""
        old_cwd = os.getcwd()
        os.chdir(self.path)
        try:
            return release_tag.main(
                ["--remote", remote, "--tag", tag, "--target", target]
            )
        finally:
            os.chdir(old_cwd)


class TestResolveLocalCommit(unittest.TestCase):
    def setUp(self):
        self.remote = _BareRemote()
        self.ws = _Workspace(self.remote)

    def tearDown(self):
        self.ws.cleanup()
        self.remote.cleanup()

    def test_resolves_head(self):
        sha = self.ws.commit()
        self.assertEqual(
            release_tag.resolve_local_commit("HEAD", cwd=str(self.ws.path)), sha
        )

    def test_unknown_ref_raises(self):
        with self.assertRaises(release_tag.ProvenanceError):
            release_tag.resolve_local_commit(
                "not-a-ref", cwd=str(self.ws.path)
            )


class TestRemoteTagCommitDiagnostics(unittest.TestCase):
    """A genuinely absent ref must never be conflated with an unrelated git
    failure (network, auth, bad remote, ...) -- the latter has to raise
    instead of silently reporting the tag as absent."""

    def setUp(self):
        self.remote = _BareRemote()
        self.ws = _Workspace(self.remote)

    def tearDown(self):
        self.ws.cleanup()
        self.remote.cleanup()

    def test_missing_ref_returns_none(self):
        self.assertIsNone(
            release_tag.remote_tag_commit(
                "origin", "v9.9.9.9", cwd=str(self.ws.path)
            )
        )

    def test_other_git_failure_raises_instead_of_reporting_absent(self):
        bogus_remote = str(self.ws.path / "does-not-exist")
        with self.assertRaises(release_tag.ProvenanceError):
            release_tag.remote_tag_commit(
                bogus_remote, "v0.1.0.0", cwd=str(self.ws.path)
            )


class TestMissingTag(unittest.TestCase):
    def setUp(self):
        self.remote = _BareRemote()
        self.ws = _Workspace(self.remote)
        self.sha = self.ws.commit()
        self.ws.push_main()

    def tearDown(self):
        self.ws.cleanup()
        self.remote.cleanup()

    def test_creates_the_tag_at_the_target_commit(self):
        self.assertEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)
        self.assertEqual(
            release_tag.remote_tag_commit(
                "origin", "v0.1.0.0", cwd=str(self.ws.path)
            ),
            self.sha,
        )


class TestMatchingExistingTag(unittest.TestCase):
    def setUp(self):
        self.remote = _BareRemote()
        self.ws = _Workspace(self.remote)
        self.sha = self.ws.commit()
        self.ws.push_main()

    def tearDown(self):
        self.ws.cleanup()
        self.remote.cleanup()

    def test_lightweight_tag_already_at_target_succeeds(self):
        self.ws.push_lightweight_tag("v0.1.0.0", self.sha)
        self.assertEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)

    def test_annotated_tag_is_peeled_before_comparison(self):
        # An annotated tag's own object SHA never equals a commit SHA; only
        # comparing the peeled *target* commit can accept this tag.
        self.ws.push_annotated_tag("v0.1.0.0", self.sha)
        self.assertEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)


class TestConflictingExistingTag(unittest.TestCase):
    def setUp(self):
        self.remote = _BareRemote()
        self.ws = _Workspace(self.remote)
        self.sha = self.ws.commit("first")
        self.ws.push_main()
        self.other_sha = self.ws.commit("second")

    def tearDown(self):
        self.ws.cleanup()
        self.remote.cleanup()

    def test_lightweight_tag_at_a_different_commit_fails_closed(self):
        self.ws.push_lightweight_tag("v0.1.0.0", self.other_sha)
        self.assertNotEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)
        # The wrong tag must survive untouched -- no force-push recovery.
        self.assertEqual(
            release_tag.remote_tag_commit(
                "origin", "v0.1.0.0", cwd=str(self.ws.path)
            ),
            self.other_sha,
        )

    def test_annotated_tag_peeled_to_a_different_commit_fails_closed(self):
        self.ws.push_annotated_tag("v0.1.0.0", self.other_sha)
        self.assertNotEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)
        # The wrong tag must survive untouched -- no force-push recovery.
        self.assertEqual(
            release_tag.remote_tag_commit(
                "origin", "v0.1.0.0", cwd=str(self.ws.path)
            ),
            self.other_sha,
        )


class TestCreationRace(unittest.TestCase):
    """Simulates a second actor winning the tag race during this run's own
    creation attempt, by monkeypatching create_remote_tag so the racing push
    lands immediately before this run's real push is attempted."""

    def setUp(self):
        self.remote = _BareRemote()
        self.ws = _Workspace(self.remote)
        self.sha = self.ws.commit("first")
        self.ws.push_main()

    def tearDown(self):
        self.ws.cleanup()
        self.remote.cleanup()

    def _simulate_concurrent_creation(self, winner_sha: str):
        original = release_tag.create_remote_tag

        def racing_create(remote, tag, target_sha, cwd=None):
            self.ws.push_lightweight_tag(tag, winner_sha)
            return original(remote, tag, target_sha, cwd=cwd)

        release_tag.create_remote_tag = racing_create
        self.addCleanup(setattr, release_tag, "create_remote_tag", original)

    def test_concurrent_matching_tag_is_accepted(self):
        self._simulate_concurrent_creation(self.sha)
        self.assertEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)

    def test_concurrent_conflicting_tag_fails_closed(self):
        other_sha = self.ws.commit("second")
        self._simulate_concurrent_creation(other_sha)
        self.assertNotEqual(self.ws.run_release_tag("v0.1.0.0", self.sha), 0)
        self.assertEqual(
            release_tag.remote_tag_commit(
                "origin", "v0.1.0.0", cwd=str(self.ws.path)
            ),
            other_sha,
        )


if __name__ == "__main__":
    unittest.main()
