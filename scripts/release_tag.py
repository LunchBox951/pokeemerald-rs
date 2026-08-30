#!/usr/bin/env python3
"""Bind a release tag to its exact triggering commit before publication.

Invoked by the ``release`` job in ``.github/workflows/release.yml``,
immediately before ``gh release create --verify-tag`` (issue #445, F-5 --
see ``docs/acceptance/v1.md`` and the issue thread for the full contract).
Given ``--remote``, ``--tag``, and ``--target``, proves ``refs/tags/<tag>``
resolves -- after peeling any annotated tag -- to ``<target>``'s exact
commit: creates the tag if absent, accepts it if it (or a
concurrently-created copy) already matches, and fails closed, leaving the
remote tag untouched, if it resolves elsewhere.

Usage::

    python3 scripts/release_tag.py --remote origin --tag v0.1.0.0 \\
        --target "$GITHUB_SHA"

Exit status: ``0`` once ``refs/tags/<tag>`` on ``<remote>`` is proven to
resolve to ``<target>``'s commit; non-zero with a diagnostic message
otherwise.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from typing import Optional

# Disposable local ref used to inspect a remote tag without ever touching the
# caller's own branches or tags. Fetched into with `+` (force) so a leftover
# ref from an earlier inspection in the same working copy never blocks reuse.
CHECK_REF = "refs/release-tag-check/target"

# git's exact message for "the requested ref is not on the remote" -- used to
# tell a genuinely absent tag apart from a network/auth/other git failure, so
# the latter is never silently reported as a missing tag.
_MISSING_REF_MARKER = "couldn't find remote ref"

# Bounds every git subprocess call (network ones especially) so a stalled
# fetch/push fails the release job instead of hanging it indefinitely.
_GIT_TIMEOUT_SECONDS = 120


class ProvenanceError(Exception):
    """Raised when a tag cannot be proven to point at the target commit."""


def _run(cmd: list, cwd: Optional[str] = None) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=_GIT_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as exc:
        return subprocess.CompletedProcess(
            cmd,
            returncode=124,
            stdout=exc.stdout or "",
            stderr=f"timed out after {_GIT_TIMEOUT_SECONDS}s: {' '.join(cmd)}",
        )


def resolve_local_commit(ref: str, cwd: Optional[str] = None) -> str:
    """Resolve ``ref`` to a commit SHA in the local repository.

    ``^{commit}`` peels through an annotated tag object (or a chain of them)
    to the commit it ultimately targets, so this also serves as the peeling
    step for any locally-fetched tag ref.
    """
    out = _run(["git", "rev-parse", "--verify", f"{ref}^{{commit}}"], cwd)
    if out.returncode != 0:
        raise ProvenanceError(
            f"{ref!r} does not resolve to a commit: {out.stderr.strip()}"
        )
    return out.stdout.strip()


def remote_tag_commit(
    remote: str, tag: str, cwd: Optional[str] = None
) -> Optional[str]:
    """Return the commit ``refs/tags/<tag>`` resolves to on ``remote``.

    Peels annotated tags to their target commit. Returns ``None`` only when
    git reports the ref itself does not exist. Any other git failure (auth,
    network, malformed remote, ...) raises ``ProvenanceError`` with the
    underlying stderr -- it must never be silently reported as "tag absent".

    Fetches into a private, disposable local ref (see ``CHECK_REF``) and
    always deletes it afterwards, so the caller's checkout, branches, and
    real tags are never touched.
    """
    fetch = _run(
        [
            "git",
            "fetch",
            "--no-tags",
            "--force",
            remote,
            f"+refs/tags/{tag}:{CHECK_REF}",
        ],
        cwd,
    )
    if fetch.returncode != 0:
        if _MISSING_REF_MARKER in fetch.stderr:
            return None
        raise ProvenanceError(
            f"could not inspect refs/tags/{tag} on remote {remote!r}: "
            f"{fetch.stderr.strip() or fetch.stdout.strip()}"
        )
    try:
        return resolve_local_commit(CHECK_REF, cwd)
    finally:
        cleanup = _run(["git", "update-ref", "-d", CHECK_REF], cwd)
        if cleanup.returncode != 0:
            print(
                f"release_tag: WARNING: could not remove disposable ref "
                f"{CHECK_REF}: {cleanup.stderr.strip()}",
                file=sys.stderr,
            )


def create_remote_tag(
    remote: str, tag: str, target_sha: str, cwd: Optional[str] = None
) -> bool:
    """Best-effort creation of a lightweight tag at ``target_sha``.

    Returns ``True`` only if the push itself reports success. A ``False``
    return does NOT mean the tag is still absent -- a concurrent creation
    racing this push (or any other transient failure) also returns ``False``.
    Callers must always re-read the remote tag afterwards rather than trust
    this return value alone (see ``ensure_tag_provenance``).

    Never forces the push: git already rejects a non-fast-forward update to
    an existing tag ref, so a conflicting tag can never be clobbered here.
    """
    push = _run(["git", "push", remote, f"{target_sha}:refs/tags/{tag}"], cwd)
    if push.returncode != 0:
        detail = (push.stderr or push.stdout).strip()
        print(
            f"release_tag: creation push for refs/tags/{tag} did not "
            f"succeed (expected on a creation race; the remote is re-read "
            f"next): {detail}",
            file=sys.stderr,
        )
    return push.returncode == 0


def ensure_tag_provenance(
    remote: str, tag: str, target_ref: str, cwd: Optional[str] = None
) -> str:
    """Bind ``tag`` on ``remote`` to ``target_ref``'s exact commit, or fail.

    Handles the missing/matching/conflicting states plus the creation race
    (see module docstring). Returns the confirmed commit SHA on success.
    Raises ``ProvenanceError`` otherwise -- the caller must not proceed to
    publish.
    """
    target_sha = resolve_local_commit(target_ref, cwd)

    existing = remote_tag_commit(remote, tag, cwd)
    if existing is None:
        # The tag looks absent. Attempt to create it, but do not trust the
        # push's own exit status either way -- see create_remote_tag. The
        # only reliable source of truth is re-reading the remote afterwards.
        create_remote_tag(remote, tag, target_sha, cwd)
        existing = remote_tag_commit(remote, tag, cwd)

    if existing is None:
        raise ProvenanceError(
            f"refs/tags/{tag} does not exist on remote {remote!r} after "
            f"attempting to create it at {target_sha}; refusing to publish"
        )
    if existing != target_sha:
        raise ProvenanceError(
            f"refs/tags/{tag} on remote {remote!r} resolves to {existing}, "
            f"not the triggering commit {target_sha}; refusing to publish a "
            f"release under a tag that points elsewhere"
        )
    return target_sha


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="release_tag.py",
        description=(
            "Bind a release tag to its exact triggering commit before "
            "publication, failing closed on any mismatch or unresolved race."
        ),
    )
    parser.add_argument(
        "--remote",
        default="origin",
        help="Git remote to inspect and (if needed) push the tag to (default: origin).",
    )
    parser.add_argument(
        "--tag",
        required=True,
        help="Tag name to bind, e.g. v0.1.0.0.",
    )
    parser.add_argument(
        "--target",
        required=True,
        help="Ref or SHA the tag must resolve to, e.g. $GITHUB_SHA.",
    )
    return parser


def main(argv: Optional[list] = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        sha = ensure_tag_provenance(args.remote, args.tag, args.target)
    except ProvenanceError as exc:
        print(f"release_tag: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"release_tag: OK ({args.tag} -> {sha})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
