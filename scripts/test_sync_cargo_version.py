#!/usr/bin/env python3
"""Unit tests for scripts/sync_cargo_version.py."""

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

    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_updates_manifest_and_verifies_lockfile(self, metadata):
        cargo_version = sync_cargo_version.sync(self.root)

        self.assertEqual(cargo_version, "0.0.22+gamepatch.7")
        self.assertIn(
            'version = "0.0.22+gamepatch.7"',
            (self.root / "Cargo.toml").read_text(encoding="utf-8"),
        )
        self.assertEqual(
            metadata.call_args_list,
            [mock.call(self.root, locked=False), mock.call(self.root, locked=True)],
        )

    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_repeated_sync_is_idempotent(self, metadata):
        sync_cargo_version.sync(self.root)
        first = (self.root / "Cargo.toml").read_bytes()
        sync_cargo_version.sync(self.root)

        self.assertEqual((self.root / "Cargo.toml").read_bytes(), first)
        self.assertEqual(metadata.call_count, 4)

    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_check_rejects_manifest_drift_without_running_cargo(self, metadata):
        with self.assertRaises(version_check.VersionError):
            sync_cargo_version.sync(self.root, check_only=True)

        metadata.assert_not_called()

    @mock.patch.object(sync_cargo_version, "run_cargo_metadata")
    def test_failed_refresh_restores_original_files(self, metadata):
        original_manifest = (self.root / "Cargo.toml").read_bytes()
        original_lock = (self.root / "Cargo.lock").read_bytes()

        def fail_after_refresh(root, *, locked):
            if not locked:
                (root / "Cargo.lock").write_bytes(b"changed lock\n")
                return
            raise sync_cargo_version.SyncError("metadata failed")

        metadata.side_effect = fail_after_refresh

        with self.assertRaises(sync_cargo_version.SyncError):
            sync_cargo_version.sync(self.root)

        self.assertEqual((self.root / "Cargo.toml").read_bytes(), original_manifest)
        self.assertEqual((self.root / "Cargo.lock").read_bytes(), original_lock)


if __name__ == "__main__":
    unittest.main()
