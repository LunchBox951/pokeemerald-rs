#!/usr/bin/env python3
"""Validate the canonical ``VERSION`` file across a proposed change.

This is a stdlib-only CI helper (no third-party dependencies) that enforces
the project's 4-part version discipline (see ``RELEASE.md``).

Version scheme:

    vFINAL.MAJOR.MINOR.PATCH

The root ``VERSION`` file stores the version *without* the ``v`` prefix, as
exactly four dot-separated non-negative integers, e.g.::

    0.1.0.0

Git tags / GitHub Releases add the ``v`` prefix; the ``VERSION`` file never
carries it.

What this script checks:

1. Read ``VERSION`` at HEAD (the current working tree).
2. Read ``VERSION`` at the base ref. When ``--base`` is not given, the base is
   derived from the ``GITHUB_BASE_REF`` environment variable: on a GitHub
   ``pull_request`` event the runner sets it to the PR's target branch, so the
   base becomes ``origin/<target>`` and the guarantee holds against the actual
   merge target rather than always ``origin/main`` (#128). For push events
   (direct commits, promotions) ``GITHUB_BASE_REF`` is empty and the base falls
   back to ``origin/main``. If the ref or the file is absent on the base --
   e.g. a brand-new repository with no remote yet -- the base is treated as
   ``0.0.0.0`` so the first commit is never rejected as a "regression".
3. Parse both sides as exactly four dot-separated non-negative integers.
   A leading ``v`` or any malformed value is rejected.
4. Reject a proposed version lower than the base (lexicographic compare over
   four unsigned ints: FINAL, then MAJOR, then MINOR, then PATCH).
5. Reject a MAJOR or MINOR bump that does not reset all lower components to 0.
6. Reject a FINAL change unless a maintainer-approved override marker file is
   present. ``FINAL`` is maintainer-only and must never be moved by
   automation or drive-by contributors (handle: gated-by-default).

Exit status: ``0`` on success (prints a clear OK line), non-zero with a clear
message on any rejection or read/parse error.

----------------------------------------------------------------------------
Self-test notes -- example version transitions.
(base -> head, with the expected verdict)

  Allowed:
    0.1.2.5 -> 0.1.2.6   # PATCH bump
    0.1.2.5 -> 0.1.3.0   # MINOR bump, lower components reset
    0.1.2.5 -> 0.2.0.0   # MAJOR bump, lower components reset
    0.9.9.9 -> 1.0.0.0   # FINAL bump -- ONLY with the approval marker present

  Rejected:
    0.1.2.5 -> 0.1.1.6   # regression (MINOR moved backward)
    0.1.2.5 -> 0.1.2     # malformed (only three components)
    0.1.2.5 -> v0.1.2.6  # malformed (VERSION must omit the tag prefix)
    0.1.2.5 -> 0.1.3.6   # MINOR bump did NOT reset PATCH to 0
    0.9.9.9 -> 1.0.0.0   # FINAL bump WITHOUT the approval marker -> rejected

You can exercise these by writing a VERSION file and running, e.g.::

    python3 scripts/version_check.py --base origin/main --head HEAD
----------------------------------------------------------------------------
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from typing import Optional, Tuple

# Number of components in the version scheme: FINAL.MAJOR.MINOR.PATCH.
VERSION_PARTS = 4

# Human-readable names for the four components, most- to least-significant.
COMPONENT_NAMES = ("FINAL", "MAJOR", "MINOR", "PATCH")

# Path (relative to the repository root) to the maintainer-approved override
# marker that authorizes a FINAL bump (see RELEASE.md). Overridable via
# --final-gate-marker.
DEFAULT_FINAL_GATE_MARKER = "docs/release/final-gate-approved.md"

# The base version assumed when no prior VERSION exists (new repo / no remote).
ABSENT_BASE_VERSION = (0, 0, 0, 0)

# Fallback base ref when no --base is given and no PR target is in scope
# (push events: direct commits, channel promotions).
DEFAULT_BASE_REF = "origin/main"

Version = Tuple[int, int, int, int]


class VersionError(Exception):
    """Raised for any malformed version or disallowed transition."""


def repo_root() -> str:
    """Return the repository root, falling back to the script's parent dir.

    Resolving the root keeps the marker lookup stable regardless of the
    working directory CI happens to invoke us from.
    """
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        )
        root = out.stdout.strip()
        if root:
            return root
    except (subprocess.CalledProcessError, FileNotFoundError, OSError):
        pass
    # Fallback: parent of the scripts/ directory holding this file.
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def parse_version(raw: str, source: str) -> Version:
    """Parse ``raw`` as exactly four dot-separated non-negative integers.

    Rejects a leading ``v`` prefix, extra/missing components, signs, blanks,
    and non-digit characters. ``source`` is used only for error messages.
    """
    text = raw.strip()
    if not text:
        raise VersionError(f"{source}: VERSION is empty")
    if text[:1] in ("v", "V"):
        raise VersionError(
            f"{source}: VERSION must omit the tag prefix; got {text!r} "
            f"(use '{text[1:]}', not '{text}')"
        )

    parts = text.split(".")
    if len(parts) != VERSION_PARTS:
        raise VersionError(
            f"{source}: VERSION must have exactly {VERSION_PARTS} "
            f"dot-separated components (FINAL.MAJOR.MINOR.PATCH); "
            f"got {len(parts)} in {text!r}"
        )

    values = []
    for name, part in zip(COMPONENT_NAMES, parts):
        # Reject anything that is not a run of ASCII digits. This excludes
        # signs ('+'/'-'), whitespace, underscores, and unicode digits that
        # int() would otherwise accept.
        if not part.isascii() or not part.isdigit():
            raise VersionError(
                f"{source}: {name} component {part!r} in {text!r} is not a "
                f"non-negative integer"
            )
        values.append(int(part))

    return (values[0], values[1], values[2], values[3])


def read_head_version(head: str, root: str) -> Version:
    """Read VERSION at HEAD.

    For the default 'HEAD' we read the working-tree file so an uncommitted
    bump is validated. For any other ref we read that ref's committed VERSION.
    """
    if head == "HEAD":
        path = os.path.join(root, "VERSION")
        try:
            with open(path, "r", encoding="utf-8") as fh:
                raw = fh.read()
        except FileNotFoundError:
            raise VersionError(
                f"head ({head}): VERSION file not found at {path}"
            )
        except OSError as exc:
            raise VersionError(f"head ({head}): cannot read VERSION: {exc}")
        return parse_version(raw, f"head ({head})")

    raw = git_show_version(head)
    if raw is None:
        raise VersionError(
            f"head ({head}): VERSION not found at ref {head!r}"
        )
    return parse_version(raw, f"head ({head})")


def git_show_version(ref: str) -> Optional[str]:
    """Return the contents of ``<ref>:VERSION`` via ``git show``.

    Returns ``None`` if the ref is unknown or the file is absent on that ref
    (e.g. a fresh repo with no ``origin/main`` yet). Any git failure is
    treated as "no base version" rather than a hard error so the very first
    commit is not rejected.
    """
    try:
        out = subprocess.run(
            ["git", "show", f"{ref}:VERSION"],
            capture_output=True,
            text=True,
        )
    except (FileNotFoundError, OSError):
        # git not installed / not runnable -> behave as if no base exists.
        return None
    if out.returncode != 0:
        return None
    return out.stdout


def resolve_base(explicit_base: Optional[str]) -> str:
    """Resolve the base ref to compare HEAD against.

    An explicit ``--base`` always wins. Otherwise the base is derived from the
    ``GITHUB_BASE_REF`` environment variable, which GitHub Actions sets to the
    target branch on ``pull_request`` events (and leaves empty on push events).
    Reading it here -- rather than interpolating ``${{ github.base_ref }}`` into
    a workflow shell -- keeps this selection logic testable and avoids feeding an
    untrusted ref name through the shell (script-injection hardening). A PR
    targeting a channel/release branch ahead of ``main`` is therefore validated
    against that branch, not ``main`` (#128); push events fall back to
    ``origin/main``.
    """
    if explicit_base is not None:
        return explicit_base
    base_ref = os.environ.get("GITHUB_BASE_REF", "").strip()
    if base_ref:
        return f"origin/{base_ref}"
    return DEFAULT_BASE_REF


def read_base_version(base: str) -> Version:
    """Read VERSION at the base ref, defaulting to 0.0.0.0 when absent."""
    raw = git_show_version(base)
    if raw is None:
        return ABSENT_BASE_VERSION
    return parse_version(raw, f"base ({base})")


def marker_present(root: str, marker_rel: str) -> bool:
    """True if the maintainer FINAL-gate approval marker file exists."""
    return os.path.isfile(os.path.join(root, marker_rel))


def check_transition(
    base: Version,
    head: Version,
    *,
    marker_ok: bool,
    marker_rel: str,
) -> None:
    """Validate base -> head; raise VersionError on any disallowed move."""
    # 4. No regressions: proposed must be >= base lexicographically.
    if head < base:
        raise VersionError(
            f"version regression: {fmt(head)} is lower than base "
            f"{fmt(base)} (versions only move forward)"
        )

    # 6. FINAL changes require the maintainer-approved override marker.
    #    (Any change to FINAL -- not just an increase -- is gated.)
    if head[0] != base[0] and not marker_ok:
        raise VersionError(
            f"FINAL changed {base[0]} -> {head[0]} without the maintainer "
            f"approval marker. FINAL is maintainer-only; commit "
            f"'{marker_rel}' to authorize this release."
        )

    # 5. A MAJOR or MINOR bump must reset all lower components to 0.
    #    (FINAL itself is index 0; a FINAL bump is governed by the gate above,
    #    but per spec a complete release should reset to a.0.0.0 too. We apply
    #    the reset rule to FINAL/MAJOR/MINOR bumps uniformly.)
    #    Walk from most-significant; the first component that increased must
    #    have every lower component equal to 0.
    for i in range(VERSION_PARTS - 1):
        if head[i] > base[i]:
            lower = head[i + 1:]
            if any(c != 0 for c in lower):
                bumped = COMPONENT_NAMES[i]
                lower_names = ", ".join(COMPONENT_NAMES[i + 1:])
                raise VersionError(
                    f"{bumped} bump {fmt(base)} -> {fmt(head)} must reset "
                    f"lower components ({lower_names}) to 0; "
                    f"expected {fmt(_reset_lower(head, i))}"
                )
            # First increased component found and validated; the move is a
            # clean higher-tier bump. Nothing below it matters.
            break
        if head[i] < base[i]:
            # Caught by the regression check above; defensive guard.
            raise VersionError(
                f"version regression at {COMPONENT_NAMES[i]}: "
                f"{fmt(head)} < {fmt(base)}"
            )
        # head[i] == base[i]: tier unchanged, inspect the next tier down.


def _reset_lower(version: Version, idx: int) -> Version:
    """Return ``version`` with every component after ``idx`` set to 0."""
    kept = list(version[: idx + 1])
    kept.extend([0] * (VERSION_PARTS - idx - 1))
    return (kept[0], kept[1], kept[2], kept[3])


def fmt(version: Version) -> str:
    """Format a parsed version tuple back to dotted string form."""
    return ".".join(str(c) for c in version)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="version_check.py",
        description=(
            "Validate the root VERSION file against a base ref: rejects "
            "malformed versions, regressions, un-reset milestone bumps, and "
            "ungated FINAL changes."
        ),
    )
    parser.add_argument(
        "--base",
        default=None,
        help=(
            "Base git ref to compare against. When omitted, it is derived from "
            "the GITHUB_BASE_REF env var (origin/<PR-target> on pull_request "
            "events), falling back to origin/main. If the ref or its VERSION is "
            "absent, base is treated as 0.0.0.0."
        ),
    )
    parser.add_argument(
        "--head",
        default="HEAD",
        help=(
            "Head to validate (default: HEAD). 'HEAD' reads the working-tree "
            "VERSION; any other ref reads that ref's committed VERSION."
        ),
    )
    parser.add_argument(
        "--final-gate-marker",
        default=DEFAULT_FINAL_GATE_MARKER,
        help=(
            "Repo-relative path to the maintainer FINAL-gate approval marker "
            f"(default: {DEFAULT_FINAL_GATE_MARKER})."
        ),
    )
    return parser


def main(argv: Optional[list] = None) -> int:
    args = build_parser().parse_args(argv)
    root = repo_root()
    base = resolve_base(args.base)

    try:
        head_version = read_head_version(args.head, root)
        base_version = read_base_version(base)
        marker_ok = marker_present(root, args.final_gate_marker)
        check_transition(
            base_version,
            head_version,
            marker_ok=marker_ok,
            marker_rel=args.final_gate_marker,
        )
    except VersionError as exc:
        print(f"version_check: FAIL: {exc}", file=sys.stderr)
        return 1

    base_str = fmt(base_version)
    head_str = fmt(head_version)
    if head_version == base_version:
        detail = f"unchanged at {head_str}"
    else:
        detail = f"{base_str} -> {head_str}"
    print(f"version_check: OK ({detail})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
