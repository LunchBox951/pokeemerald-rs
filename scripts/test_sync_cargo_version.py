#!/usr/bin/env python3
"""Unit tests for scripts/sync_cargo_version.py."""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))

import sync_cargo_version  # noqa: E402
import version_check  # noqa: E402

class TestSyncCargoVersion(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        (self.root / "VERSION").write_text("0.0.22.7\n", encoding="utf-8")
        (self.root / "Cargo.toml").write_text(
            "[workspace.package]\n"
            'version = "0.0.0"\n'
            'rust-version = "1.97.1"\n',
            encoding="utf-8",
        )
        (self.root / "Cargo.lock").write_bytes(b"original lock\n")

    def tearDown(self):
        self._tmp.cleanup()

    @mock.patch.object(sync_cargo_version, "refresh_cargo_lock")
    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_updates_manifest_and_verifies_lockfile(self, metadata, refresh_lock):
        cargo_version = sync_cargo_version.sync(self.root)

        self.assertEqual(cargo_version, "0.0.22+gamepatch.7")
        self.assertIn(
            'version = "0.0.22+gamepatch.7"',
            (self.root / "Cargo.toml").read_text(encoding="utf-8"),
        )
        # The final verification must use full dependency resolution
        # (no_deps=False): plain --no-deps metadata never inspects
        # workspace-member version pins, so it would pass even against a
        # stale lock -- exactly the bug this fix closes.
        self.assertEqual(
            metadata.call_args_list,
            [
                mock.call(self.root, locked=False),
                mock.call(self.root, locked=True, no_deps=False),
            ],
        )
        refresh_lock.assert_called_once_with(self.root)

    @mock.patch.object(sync_cargo_version, "refresh_cargo_lock")
    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_repeated_sync_is_idempotent(self, metadata, refresh_lock):
        sync_cargo_version.sync(self.root)
        first = (self.root / "Cargo.toml").read_bytes()
        sync_cargo_version.sync(self.root)

        self.assertEqual((self.root / "Cargo.toml").read_bytes(), first)
        self.assertEqual(metadata.call_count, 4)
        self.assertEqual(refresh_lock.call_count, 2)

    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_check_rejects_manifest_drift_without_running_cargo(self, metadata):
        with self.assertRaises(version_check.VersionError):
            sync_cargo_version.sync(self.root, check_only=True)

        metadata.assert_not_called()

    @mock.patch.object(sync_cargo_version, "refresh_cargo_lock")
    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_failed_refresh_restores_original_files(self, metadata, refresh_lock):
        original_manifest = (self.root / "Cargo.toml").read_bytes()
        original_lock = (self.root / "Cargo.lock").read_bytes()

        def fake_refresh(root):
            (root / "Cargo.lock").write_bytes(b"changed lock\n")

        refresh_lock.side_effect = fake_refresh

        def fail_final_verification(root, *, locked, no_deps=True):
            if locked and not no_deps:
                raise sync_cargo_version.SyncError("metadata failed")

        metadata.side_effect = fail_final_verification

        with self.assertRaises(sync_cargo_version.SyncError):
            sync_cargo_version.sync(self.root)

        self.assertEqual((self.root / "Cargo.toml").read_bytes(), original_manifest)
        self.assertEqual((self.root / "Cargo.lock").read_bytes(), original_lock)

    @mock.patch.object(sync_cargo_version, "subprocess")
    def test_cargo_invocations_pin_no_deps_and_offline_flags(self, subprocess_mod):
        """Pin the literal argv Cargo is invoked with.

        The earlier tests mock ``run_cargo_metadata``/``refresh_cargo_lock``
        wholesale, which is exactly why the original ``--no-deps`` bug
        escaped: nothing ever inspected what was actually passed to Cargo.
        This test inspects the real argv each helper builds.
        """
        completed = mock.Mock(returncode=0, stdout="", stderr="")
        subprocess_mod.run.return_value = completed

        sync_cargo_version.run_cargo_metadata(self.root, locked=False)
        sync_cargo_version.run_cargo_metadata(self.root, locked=True, no_deps=False)
        sync_cargo_version.refresh_cargo_lock(self.root)

        calls = subprocess_mod.run.call_args_list
        self.assertEqual(len(calls), 3)

        manifest_check_argv = calls[0].args[0]
        locked_full_argv = calls[1].args[0]
        refresh_argv = calls[2].args[0]

        self.assertEqual(
            manifest_check_argv,
            ["cargo", "metadata", "--format-version", "1", "--offline", "--no-deps"],
        )
        # The real lock-staleness check must NOT carry --no-deps: that flag
        # is what let a stale lock slip past --locked undetected.
        self.assertEqual(
            locked_full_argv,
            ["cargo", "metadata", "--format-version", "1", "--offline", "--locked"],
        )
        self.assertNotIn("--no-deps", locked_full_argv)
        self.assertEqual(
            refresh_argv, ["cargo", "update", "--workspace", "--offline"]
        )

    @mock.patch.object(sync_cargo_version, "subprocess")
    def test_refresh_retries_online_when_offline_resolution_fails(
        self, subprocess_mod
    ):
        """A fresh checkout's empty Cargo home lacks registry-index metadata.

        Offline resolution then fails before touching the lock, so the
        refresh must retry once with registry access allowed instead of
        failing the whole sync.
        """
        offline_failure = mock.Mock(
            returncode=101,
            stdout="",
            stderr="no matching package named `softbuffer` found",
        )
        online_success = mock.Mock(returncode=0, stdout="", stderr="")
        subprocess_mod.run.side_effect = [offline_failure, online_success]

        sync_cargo_version.refresh_cargo_lock(self.root)

        calls = subprocess_mod.run.call_args_list
        self.assertEqual(len(calls), 2)
        self.assertEqual(
            calls[0].args[0], ["cargo", "update", "--workspace", "--offline"]
        )
        self.assertEqual(calls[1].args[0], ["cargo", "update", "--workspace"])

    @mock.patch.object(sync_cargo_version, "subprocess")
    def test_full_resolution_metadata_retries_online_but_no_deps_does_not(
        self, subprocess_mod
    ):
        """Check mode's full-resolution --locked call also needs the retry.

        A cold Cargo home cannot resolve the registry offline even when the
        committed lock is synchronized, so the --locked verification retries
        once with registry access (still read-only: --locked forbids
        writes). The --no-deps manifest parse never resolves, so it stays a
        single offline attempt.
        """
        offline_failure = mock.Mock(
            returncode=101,
            stdout="",
            stderr="no matching package named `softbuffer` found",
        )
        online_success = mock.Mock(returncode=0, stdout="", stderr="")
        subprocess_mod.run.side_effect = [offline_failure, online_success]

        sync_cargo_version.run_cargo_metadata(self.root, locked=True, no_deps=False)

        calls = subprocess_mod.run.call_args_list
        self.assertEqual(len(calls), 2)
        self.assertEqual(
            calls[0].args[0],
            ["cargo", "metadata", "--format-version", "1", "--offline", "--locked"],
        )
        self.assertEqual(
            calls[1].args[0],
            ["cargo", "metadata", "--format-version", "1", "--locked"],
        )

        subprocess_mod.run.reset_mock()
        subprocess_mod.run.side_effect = [offline_failure]
        with self.assertRaises(sync_cargo_version.SyncError):
            sync_cargo_version.run_cargo_metadata(self.root, locked=False)
        self.assertEqual(subprocess_mod.run.call_count, 1)


class TestSyncCargoVersionWithRealCargo(unittest.TestCase):
    """End-to-end regression test using the real ``cargo`` binary.

    Reproduces the exact failure from #332: a lockfile pinned to an old
    workspace version, followed by a VERSION bump and a sync. Before the
    fix, ``sync()`` left the lock stale and a subsequent
    ``cargo metadata --locked`` (full resolution, as real CI jobs run it)
    failed. No network access is required -- the workspace has no
    third-party dependencies.
    """

    def setUp(self):
        # Mandatory, not skipped: without Cargo the stale-lock regression and
        # the read-only check-mode contract would lose their only end-to-end
        # coverage (test-ratchet).
        if shutil.which("cargo") is None:
            self.fail("cargo binary is required for these regression tests")
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        (self.root / "member" / "src").mkdir(parents=True)
        (self.root / "member" / "src" / "lib.rs").write_text(
            "// empty\n", encoding="utf-8"
        )
        (self.root / "member" / "Cargo.toml").write_text(
            "[package]\n"
            'name = "member"\n'
            "version.workspace = true\n"
            "edition.workspace = true\n",
            encoding="utf-8",
        )
        (self.root / "Cargo.toml").write_text(
            "[workspace]\n"
            'members = ["member"]\n'
            'resolver = "2"\n'
            "\n"
            "[workspace.package]\n"
            'version = "0.0.0"\n'
            'edition = "2021"\n',
            encoding="utf-8",
        )
        (self.root / "VERSION").write_text("0.0.21.0\n", encoding="utf-8")

    def tearDown(self):
        self._tmp.cleanup()

    def _cargo_metadata_locked_offline(self) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"],
            cwd=self.root,
            capture_output=True,
            text=True,
        )

    def test_sync_from_stale_lock_leaves_locked_metadata_passing(self):
        # Establish an initial, correctly-synced lock at 0.0.21.0.
        sync_cargo_version.sync(self.root)
        self.assertEqual(
            self._cargo_metadata_locked_offline().returncode,
            0,
            "sanity: freshly synced workspace must already satisfy --locked",
        )

        # Bump VERSION the way a real PR does, then sync again: this is the
        # exact stale-lock scenario from #332.
        (self.root / "VERSION").write_text("0.0.22.5\n", encoding="utf-8")
        sync_cargo_version.sync(self.root)

        lock_text = (self.root / "Cargo.lock").read_text(encoding="utf-8")
        self.assertIn("0.0.22+gamepatch.5", lock_text)

        result = self._cargo_metadata_locked_offline()
        self.assertEqual(
            result.returncode,
            0,
            f"cargo metadata --locked failed after sync: {result.stderr}",
        )

    def test_check_mode_rejects_stale_lock(self):
        sync_cargo_version.sync(self.root)

        # Simulate the old buggy behavior: bump the manifest version but
        # leave Cargo.lock untouched (what --no-deps used to allow).
        manifest = self.root / "Cargo.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace(
                "0.0.21+gamepatch.0", "0.0.22+gamepatch.5"
            ),
            encoding="utf-8",
        )
        (self.root / "VERSION").write_text("0.0.22.5\n", encoding="utf-8")

        with self.assertRaises(sync_cargo_version.SyncError):
            sync_cargo_version.sync(self.root, check_only=True)

        # check mode must not have written anything.
        self.assertNotIn(
            "0.0.22+gamepatch.5",
            (self.root / "Cargo.lock").read_text(encoding="utf-8"),
        )


if __name__ == "__main__":
    unittest.main()
