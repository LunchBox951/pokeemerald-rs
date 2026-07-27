#!/usr/bin/env bash
# init.sh — bootstrap the pokeemerald-rewrite reference checkouts.
#
# Clones pret/pokeemerald and mgba-emu/mgba into ./pokeemerald and ./mgba so
# the Rust rewrite has the upstream C source available for reference. Both
# directories are gitignored and are never committed back to this repo.
#
# Both references are pinned to a fixed commit SHA (below), not to the
# remotes' moving default branches: a fresh bootstrap always checks out
# exactly that revision, so two independent clean checkouts of the same
# pokeemerald-rs commit see byte-identical upstream trees (issue #136). To
# deliberately move a pin (e.g. to pick up new upstream behaviour), bump the
# matching *_REF constant in a dedicated commit/PR so the change is reviewed
# and recorded; existing checkouts then need `rm -rf pokeemerald mgba &&
# ./init.sh` (see the mismatch error below).
#
# Behaviour:
#   * In the main checkout: clones from GitHub and checks out the pinned SHA;
#                            an existing checkout is verified against the pin
#                            and the script fails loudly on a mismatch rather
#                            than silently reusing a stale tree.
#   * In a git worktree:    creates symlinks to the main checkout's pokeemerald/
#                           and mgba/ directories (no extra disk, no clone).
#
# POKEEMERALD_REMOTE/MGBA_REMOTE/POKEEMERALD_REF/MGBA_REF may be overridden via
# the environment; this is intended for this script's own regression test
# (init_test.sh), which points them at local synthetic remotes/refs. Ordinary
# use should never need to set them.

set -euo pipefail

POKEEMERALD_REMOTE="${POKEEMERALD_REMOTE:-https://github.com/pret/pokeemerald.git}"
MGBA_REMOTE="${MGBA_REMOTE:-https://github.com/mgba-emu/mgba.git}"

# Pinned upstream revisions (audited 2026-07-26, issue #136). Update
# deliberately, in their own commit, when the project's upstream basis moves.
POKEEMERALD_REF="${POKEEMERALD_REF:-83df84e40623b79281f2397faa611cbf044170bd}"
MGBA_REF="${MGBA_REF:-c034660f007c543233f1cadeb0ca13c71afd8f41}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

if ! command -v git >/dev/null 2>&1; then
    echo "error: git is required but not installed" >&2
    exit 1
fi

git_dir_abs="$(cd "$(git rev-parse --git-dir)" && pwd)"
git_common_dir_abs="$(cd "$(git rev-parse --git-common-dir)" && pwd)"

# verify_pinned <dir> <ref> — assert an existing reference checkout really is
# the pinned tree: HEAD at the pin AND a clean worktree/index (HEAD equality
# alone says nothing about edited, staged, or untracked files feeding the
# extraction pipeline). A tarball-extracted tree (no .git) cannot be
# verified; warn loudly rather than guessing.
verify_pinned() {
    local dir="$1"
    local ref="$2"

    if [[ ! -d "$dir/.git" ]]; then
        echo "warning: $dir has no git metadata; cannot verify it matches pinned revision $ref" >&2
        echo "  (byte-identical bootstraps are only guaranteed for git checkouts)" >&2
        return 0
    fi
    local actual
    actual="$(git -C "$dir" rev-parse HEAD)"
    if [[ "$actual" != "$ref" ]]; then
        echo "error: $dir is at $actual, but init.sh pins it to $ref" >&2
        echo "  to update: rm -rf $dir && ./init.sh" >&2
        echo "  (if this pin should move, that is a deliberate change to the *_REF constant in init.sh, not a silent skip)" >&2
        exit 1
    fi
    local dirty
    dirty="$(git -C "$dir" status --porcelain)"
    if [[ -n "$dirty" ]]; then
        echo "error: $dir is at the pinned revision but its tree is not clean:" >&2
        printf '%s\n' "$dirty" | head -20 >&2
        echo "  to restore the pinned tree: rm -rf $dir && ./init.sh" >&2
        exit 1
    fi
}

if [[ "$git_dir_abs" != "$git_common_dir_abs" ]]; then
    # Worktree: symlink the main checkout's reference directories rather than
    # re-cloning them. This works with both git-cloned and tarball-extracted
    # sources and avoids duplicating gigabytes of read-only data per worktree.
    # The main checkout's trees are verified against the pins first — an
    # unvalidated symlink would silently reintroduce the drift the pins
    # exist to prevent.
    main_repo="$(dirname "$git_common_dir_abs")"
    for name in pokeemerald mgba; do
        src="$main_repo/$name"
        if [[ ! -d "$src" ]]; then
            echo "error: $src missing; run init.sh in the main checkout ($main_repo) first." >&2
            exit 1
        fi
        case "$name" in
            pokeemerald) verify_pinned "$src" "$POKEEMERALD_REF" ;;
            mgba)        verify_pinned "$src" "$MGBA_REF" ;;
        esac
        if [[ -L "$name" || -d "$name" ]]; then
            echo "skip: $name already present"
        else
            ln -s "$src" "$name"
            echo "linked $name -> $src"
        fi
    done
    echo "done."
    exit 0
fi

# clone_and_pin clones $source into $target and checks it out at the pinned
# $ref, or — if $target already holds a checkout — verifies it is already at
# $ref. A pre-existing checkout at any other revision is a hard error: silently
# reusing it would let independent bootstraps of the same pokeemerald-rs
# commit drift onto different upstream trees (issue #136).
clone_and_pin() {
    local source="$1"
    local target="$2"
    local ref="$3"

    if [[ -d "$target/.git" ]]; then
        verify_pinned "$target" "$ref"
        echo "skip: $target already at pinned revision $ref"
        return
    fi

    echo "cloning $source -> $target"
    git clone --quiet "$source" "$target"
    echo "checking out pinned revision $ref in $target"
    if ! git -C "$target" -c advice.detachedHead=false checkout --quiet "$ref"; then
        echo "error: failed to check out pinned revision $ref in $target" >&2
        echo "  the upstream repository may have rewritten history; update the *_REF constant in init.sh" >&2
        exit 1
    fi
}

echo "main checkout detected; cloning from GitHub"
clone_and_pin "$POKEEMERALD_REMOTE" "pokeemerald" "$POKEEMERALD_REF"
clone_and_pin "$MGBA_REMOTE" "mgba" "$MGBA_REF"

echo "done."
