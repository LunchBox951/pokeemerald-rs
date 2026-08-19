#!/usr/bin/env python3
"""Synchronize Cargo package metadata with the canonical game VERSION."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from typing import Optional

import version_check


class SyncError(Exception):
    """Raised when Cargo metadata cannot be synchronized safely."""


def _restore(path: Path, original: Optional[bytes]) -> None:
    """Restore one file to its exact pre-sync state."""
    if original is None:
        path.unlink(missing_ok=True)
    else:
        path.write_bytes(original)


def run_cargo_metadata(root: Path, *, locked: bool) -> None:
    """Refresh or verify Cargo metadata."""
    command = ["cargo", "metadata", "--format-version", "1", "--no-deps"]
    if locked:
        command.append("--locked")
    try:
        result = subprocess.run(
            command,
            cwd=root,
            capture_output=True,
            text=True,
        )
    except (FileNotFoundError, OSError) as exc:
        raise SyncError(f"cannot run Cargo: {exc}") from exc
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise SyncError(f"{' '.join(command)} failed: {detail}")


def sync(root: Path, *, check_only: bool = False) -> str:
    """Synchronize the workspace manifest and lockfile with ``VERSION``."""
    version_path = root / "VERSION"
    manifest_path = root / "Cargo.toml"
    lock_path = root / "Cargo.lock"

    try:
        version_raw = version_path.read_bytes().decode("utf-8")
        original_manifest = manifest_path.read_bytes()
        manifest_raw = original_manifest.decode("utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise SyncError(f"cannot read version metadata: {exc}") from exc

    game_version = version_check.parse_version(version_raw, str(version_path))
    cargo_version = version_check.cargo_version(game_version)

    if check_only:
        version_check.check_workspace_package_version(
            manifest_raw, game_version, str(manifest_path)
        )
        run_cargo_metadata(root, locked=True)
        return cargo_version

    original_lock = lock_path.read_bytes() if lock_path.exists() else None
    updated_manifest = version_check.replace_workspace_package_version(
        manifest_raw, game_version, str(manifest_path)
    )

    try:
        manifest_path.write_bytes(updated_manifest.encode("utf-8"))
        run_cargo_metadata(root, locked=False)
        version_check.check_workspace_package_version(
            manifest_path.read_bytes().decode("utf-8"),
            game_version,
            str(manifest_path),
        )
        run_cargo_metadata(root, locked=True)
    except (OSError, SyncError, version_check.VersionError) as exc:
        _restore(manifest_path, original_manifest)
        _restore(lock_path, original_lock)
        raise SyncError(
            f"synchronization failed; restored original files: {exc}"
        ) from exc

    return cargo_version


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Synchronize Cargo package metadata with VERSION."
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="reject stale Cargo metadata without writing files",
    )
    return parser


def main(argv: Optional[list] = None) -> int:
    args = build_parser().parse_args(argv)
    root = Path(version_check.repo_root())
    try:
        cargo_version = sync(root, check_only=args.check)
    except (SyncError, version_check.VersionError) as exc:
        print(f"sync_cargo_version: FAIL: {exc}", file=sys.stderr)
        return 1

    action = "verified" if args.check else "synchronized"
    print(f"sync_cargo_version: OK ({action} {cargo_version})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
