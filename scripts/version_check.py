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
2. Read ``VERSION`` at the base ref (default ``origin/main``). If the ref or
   the file is absent on the base -- e.g. a brand-new repository with no
   remote yet -- the base is treated as ``0.0.0.0`` so the first commit is
   never rejected as a "regression".
3. Parse both sides as exactly four dot-separated non-negative integers.
   A leading ``v`` or any malformed value is rejected.
4. Reject a proposed version lower than the base (lexicographic compare over
   four unsigned ints: FINAL, then MAJOR, then MINOR, then PATCH).
5. Reject a MAJOR or MINOR bump that does not reset all lower components to 0.
6. Reject a FINAL change unless the maintainer-approved override marker file
   (``docs/release/final-gate-approved.md`` by default) is present *at head*,
   parses as an explicit approval of the exact proposed VERSION, and differs
   from whatever that same path contained at the base ref. ``FINAL`` is
   maintainer-only and must never be moved by automation or drive-by
   contributors (handle: gated-by-default). A marker is only evidence for
   *this* transition if the diff actually touched it -- an untouched marker
   left over from an earlier approval authorizes nothing.

   The marker must contain, one per line (case-insensitive keys, order
   doesn't matter)::

       Approved version: <FINAL.MAJOR.MINOR.PATCH>
       Date: <YYYY-MM-DD>

   the ``Approved version`` must equal the proposed VERSION exactly.

Exit status: ``0`` on success (prints a clear OK line), non-zero with a clear
message on any rejection or read/parse error.

----------------------------------------------------------------------------
Self-test notes -- example version transitions.
(base -> head, with the expected verdict)

  Allowed:
    0.1.2.5 -> 0.1.2.6   # PATCH bump
    0.1.2.5 -> 0.1.3.0   # MINOR bump, lower components reset
    0.1.2.5 -> 0.2.0.0   # MAJOR bump, lower components reset
    0.9.9.9 -> 1.0.0.0   # FINAL bump -- ONLY with a freshly committed marker
                         # naming "Approved version: 1.0.0.0" plus a Date

  Rejected:
    0.1.2.5 -> 0.1.1.6   # regression (MINOR moved backward)
    0.1.2.5 -> 0.1.2     # malformed (only three components)
    0.1.2.5 -> v0.1.2.6  # malformed (VERSION must omit the tag prefix)
    0.1.2.5 -> 0.1.3.6   # MINOR bump did NOT reset PATCH to 0
    0.9.9.9 -> 1.0.0.0   # FINAL bump WITHOUT the approval marker -> rejected
    0.9.9.9 -> 1.0.0.0   # FINAL bump whose marker is unchanged from base,
                         # malformed, or names a version other than 1.0.0.0

You can exercise these by writing a VERSION file and running, e.g.::

    python3 scripts/version_check.py --base origin/main --head HEAD
----------------------------------------------------------------------------
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from datetime import date as _date
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

# Marker line patterns: "Approved version: 1.0.0.0" / "Date: 2026-07-25".
# Case-insensitive key, colon-separated, one non-blank token as the value.
# Horizontal whitespace only ([ \t]) -- \s matches newlines under (?m), which
# would let a field split across lines satisfy the documented one-line form.
# (\r? tolerates CRLF line endings: \r is neither \S nor [ \t], so without
# it a CRLF marker would never match and a legitimate approval would be
# rejected.)
MARKER_VERSION_RE = re.compile(
    r"(?im)^[ \t]*approved version[ \t]*:[ \t]*(\S+)[ \t]*\r?$"
)
MARKER_DATE_RE = re.compile(r"(?im)^[ \t]*date[ \t]*:[ \t]*(\S+)[ \t]*\r?$")

# Strict Date shape; date.fromisoformat alone also accepts e.g. '20260725'
# and week dates, which the documented format does not.
MARKER_DATE_SHAPE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

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
    # Fallback: parent of the scripts/ directory holding this file. Every
    # read -- including the base-side `git show` -- is then anchored to the
    # resolved root, so head and base always describe the same repository.
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def parse_version(raw: str, source: str) -> Version:
    """Parse ``raw`` as exactly four dot-separated non-negative integers.

    Rejects a leading ``v`` prefix, extra/missing components, signs, blanks,
    and non-digit characters. ``source`` is used only for error messages.
    """
    # Strip only trailing newlines: the tag is built from the raw file text
    # (`v$(cat VERSION)`), and command substitution strips trailing newlines
    # too -- but any OTHER surrounding whitespace would ship in the tag
    # name, so it is rejected rather than silently normalized away.
    text = raw.rstrip("\n")
    if text != text.strip():
        raise VersionError(
            f"{source}: VERSION has surrounding whitespace in {text!r}; "
            f"release tooling tags the raw text, so the spelling must be "
            f"exact"
        )
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
        # Reject leading zeroes: '01.0.0.0' would compare equal to the
        # approved '1.0.0.0' as integers, yet release tooling builds tags
        # from the raw text -- an approval must name the exact spelling.
        if len(part) > 1 and part[0] == "0":
            raise VersionError(
                f"{source}: {name} component {part!r} in {text!r} has a "
                f"leading zero; use the canonical spelling"
            )
        values.append(int(part))

    return (values[0], values[1], values[2], values[3])


def read_ref_file(ref: str, rel_path: str, root: str) -> Optional[str]:
    """Return the head-side contents of ``rel_path`` at ``ref``.

    ``'HEAD'`` reads the working-tree file (so an uncommitted change is
    validated); any other ref reads that ref's committed blob via
    ``git show``. A missing file or unknown ref is reported as ``None``
    rather than an error -- callers decide whether absence is fatal.

    A symlinked path is treated as absent: ``git show`` on the base side
    returns a symlink's target *path text*, while ``open()`` here would
    follow it -- an asymmetry that would let a symlinked marker read as
    "changed" while its bytes never changed. Failing closed keeps the two
    sides comparable.
    """
    if ref == "HEAD":
        if _any_component_is_symlink(root, rel_path):
            return None
        path = os.path.join(root, rel_path)
        try:
            with open(path, "r", encoding="utf-8") as fh:
                return fh.read()
        except OSError:
            return None
    return git_show_file(ref, rel_path, root)


def _any_component_is_symlink(root: str, rel_path: str) -> bool:
    """True if any path component of ``rel_path`` under ``root`` is a symlink.

    Checking only the leaf is not enough: a symlinked ancestor directory
    (e.g. ``docs/release`` -> elsewhere) makes ``open()`` read content that
    ``git show`` cannot see, silently desynchronizing the two sides.
    """
    current = root
    for part in rel_path.replace("\\", "/").split("/"):
        current = os.path.join(current, part)
        if os.path.islink(current):
            return True
    return False


def git_show_file(ref: str, rel_path: str, root: str) -> Optional[str]:
    """Return the contents of ``<ref>:<rel_path>`` via ``git show``.

    Runs git in ``root`` so the read is independent of the caller's working
    directory (matching the head-side reads); otherwise an invocation from
    outside the worktree would silently read the base as absent and fail
    the gate OPEN. Returns ``None`` if the ref is unknown or the file is
    absent on that ref (e.g. a fresh repo with no ``origin/main`` yet, or a
    marker that simply doesn't exist there) -- callers decide whether
    absence is fatal.
    """
    try:
        out = subprocess.run(
            ["git", "show", f"{ref}:{rel_path}"],
            capture_output=True,
            text=True,
            cwd=root,
        )
    except (FileNotFoundError, OSError):
        # git not installed / not runnable -> behave as if absent.
        return None
    if out.returncode != 0:
        return None
    return out.stdout


def read_head_version(head: str, root: str) -> Version:
    """Read VERSION at ``head`` (working tree for 'HEAD', else the ref)."""
    raw = read_ref_file(head, "VERSION", root)
    if raw is None:
        raise VersionError(f"head ({head}): VERSION not found")
    return parse_version(raw, f"head ({head})")


def read_base_version(base: str, root: str) -> Version:
    """Read VERSION at the base ref, defaulting to 0.0.0.0 when absent.

    The base side always reads the committed blob -- never the working
    tree -- so ``--base HEAD`` compares against the commit, not against
    the same uncommitted file the head side is validating.
    """
    raw = git_show_file(base, "VERSION", root)
    if raw is None:
        return ABSENT_BASE_VERSION
    return parse_version(raw, f"base ({base})")


def parse_marker(raw: str, source: str) -> Tuple[Version, str]:
    """Parse a FINAL-gate approval marker's ``Approved version`` and ``Date``.

    Raises ``VersionError`` if either field is missing or malformed. Returns
    the approved version tuple and the raw (validated) date string.
    """
    version_match = MARKER_VERSION_RE.search(raw)
    if not version_match:
        raise VersionError(
            f"{source}: FINAL-gate marker is missing an "
            f"'Approved version: FINAL.MAJOR.MINOR.PATCH' line"
        )
    date_match = MARKER_DATE_RE.search(raw)
    if not date_match:
        raise VersionError(
            f"{source}: FINAL-gate marker is missing a 'Date: YYYY-MM-DD' line"
        )

    approved_version = parse_version(
        version_match.group(1), f"{source} (Approved version)"
    )

    date_text = date_match.group(1)
    if not MARKER_DATE_SHAPE_RE.match(date_text):
        raise VersionError(
            f"{source}: FINAL-gate marker Date {date_text!r} is not in "
            f"YYYY-MM-DD form"
        )
    try:
        _date.fromisoformat(date_text)
    except ValueError:
        raise VersionError(
            f"{source}: FINAL-gate marker Date {date_text!r} is not a valid "
            f"YYYY-MM-DD date"
        )

    return approved_version, date_text


def check_transition(
    base: Version,
    head: Version,
    *,
    marker_head: Optional[str],
    marker_base: Optional[str],
    marker_rel: str,
) -> None:
    """Validate base -> head; raise VersionError on any disallowed move."""
    # 4. No regressions: proposed must be >= base lexicographically.
    if head < base:
        raise VersionError(
            f"version regression: {fmt(head)} is lower than base "
            f"{fmt(base)} (versions only move forward)"
        )

    # 6. FINAL changes require a maintainer-approved override marker that is
    #    (a) present at head, (b) a valid, parseable approval, (c) naming
    #    this exact proposed VERSION, and (d) actually introduced or edited
    #    by this change -- an untouched marker inherited from base is not
    #    evidence of review for *this* transition (any change to FINAL, not
    #    just an increase, is gated).
    if head[0] != base[0]:
        if marker_head is None:
            raise VersionError(
                f"FINAL changed {base[0]} -> {head[0]} without the "
                f"maintainer approval marker. FINAL is maintainer-only; "
                f"commit '{marker_rel}' naming 'Approved version: "
                f"{fmt(head)}' and a Date to authorize this release."
            )

        approved_version, _approved_date = parse_marker(marker_head, marker_rel)

        if approved_version != head:
            raise VersionError(
                f"FINAL-gate marker '{marker_rel}' approves "
                f"{fmt(approved_version)}, not the proposed {fmt(head)}; "
                f"update the marker to name the exact target version"
            )

        if marker_head == marker_base:
            raise VersionError(
                f"FINAL-gate marker '{marker_rel}' is unchanged from the "
                f"base revision; it must be added or updated to record "
                f"approval of this specific {fmt(base)} -> {fmt(head)} "
                f"transition, not carried over from an earlier approval"
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
        default="origin/main",
        help=(
            "Base git ref to compare against (default: origin/main). "
            "If the ref or its VERSION is absent, base is treated as 0.0.0.0."
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

    try:
        head_version = read_head_version(args.head, root)
        base_version = read_base_version(args.base, root)
        marker_head = read_ref_file(args.head, args.final_gate_marker, root)
        # Base side always reads the committed blob (see read_base_version).
        marker_base = git_show_file(args.base, args.final_gate_marker, root)
        check_transition(
            base_version,
            head_version,
            marker_head=marker_head,
            marker_base=marker_base,
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
