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


def run_cargo_metadata(root: Path, *, locked: bool, no_deps: bool = True) -> None:
    """Parse or verify Cargo metadata; never writes the lockfile.

    ``--no-deps`` (the default) skips full dependency resolution, so it only
    proves the manifest parses -- it does NOT prove the lock satisfies the
    manifest, because Cargo never inspects workspace-member version pins
    under ``--no-deps``. Pass ``no_deps=False`` for a real ``--locked``
    verification that the *lock*, not just the manifest, is in sync; that
    call still never writes because ``--locked`` forbids it, refusing
    instead. Refreshing the lock is ``refresh_cargo_lock``'s job.
    """
    if not no_deps and not locked:
        raise SyncError(
            "full-resolution metadata without --locked would write Cargo.lock; "
            "refreshing the lock is refresh_cargo_lock's job"
        )
    base = ["cargo", "metadata", "--format-version", "1"]
    if no_deps:
        base.append("--no-deps")
    if locked:
        base.append("--locked")
    # Full resolution needs the registry index, which a fresh checkout's
    # empty Cargo home lacks, so on offline failure retry once with
    # registry access allowed; ``--locked`` still forbids lockfile writes.
    # ``--no-deps`` never resolves, so it keeps the single offline attempt.
    attempts = [base[:4] + ["--offline"] + base[4:]]
    if not no_deps:
        attempts.append(base)
    last_detail = ""
    for command in attempts:
        try:
            result = subprocess.run(
                command,
                cwd=root,
                capture_output=True,
                text=True,
            )
        except (FileNotFoundError, OSError) as exc:
            raise SyncError(f"cannot run Cargo: {exc}") from exc
        if result.returncode == 0:
            return
        last_detail = result.stderr.strip() or result.stdout.strip()
    raise SyncError(f"{' '.join(base)} failed: {last_detail}")


def refresh_cargo_lock(root: Path) -> None:
    """Rewrite ``Cargo.lock`` so it matches the just-written manifest.

    ``cargo update --workspace`` re-resolves only the workspace members' own
    version entries in the lock against the manifest just written, without
    touching any third-party dependency version. The first attempt passes
    ``--offline`` so a populated Cargo home never touches the network; a
    fresh checkout with an empty Cargo home lacks the registry-index
    metadata offline resolution needs, so on failure the command retries
    once with registry access allowed.
    """
    base = ["cargo", "update", "--workspace"]
    last_detail = ""
    for command in (base + ["--offline"], base):
        try:
            result = subprocess.run(
                command,
                cwd=root,
                capture_output=True,
                text=True,
            )
        except (FileNotFoundError, OSError) as exc:
            raise SyncError(f"cannot run Cargo: {exc}") from exc
        if result.returncode == 0:
            return
        last_detail = result.stderr.strip() or result.stdout.strip()
    raise SyncError(f"{' '.join(base)} failed: {last_detail}")


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
        # no_deps=False: a stale Cargo.lock (workspace member versions still
        # pinned to the old release) passes plain ``--no-deps --locked``
        # metadata unnoticed -- Cargo never inspects member version pins
        # without full resolution. This full-resolution ``--locked`` call is
        # what actually rejects a stale lock, and it stays read-only:
        # ``--locked`` makes Cargo refuse to write rather than update.
        run_cargo_metadata(root, locked=True, no_deps=False)
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
        # Rewrite Cargo.lock's workspace-member entries to match the version
        # just written -- ``--no-deps`` metadata calls never do this (that
        # was the bug: a bumped VERSION left the lock stale, and the next
        # `--locked` CI job failed because it, unlike this helper, resolves
        # dependencies fully).
        refresh_cargo_lock(root)
        # Full-resolution verification (no_deps=False) that the refreshed
        # lock genuinely satisfies --locked, not just that the manifest
        # parses.
        run_cargo_metadata(root, locked=True, no_deps=False)
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
