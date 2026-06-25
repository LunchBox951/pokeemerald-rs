#!/usr/bin/env bash
# init.sh — bootstrap the pokeemerald-rewrite reference checkouts.
#
# Clones pret/pokeemerald and mgba-emu/mgba into ./pokeemerald and ./mgba so
# the Rust rewrite has the upstream C source available for reference. Both
# directories are gitignored and are never committed back to this repo.
#
# Behaviour:
#   * In the main checkout: clones from GitHub.
#   * In a git worktree:    creates symlinks to the main checkout's pokeemerald/
#                           and mgba/ directories (no extra disk, no clone).

set -euo pipefail

POKEEMERALD_REMOTE="https://github.com/pret/pokeemerald.git"
MGBA_REMOTE="https://github.com/mgba-emu/mgba.git"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

if ! command -v git >/dev/null 2>&1; then
    echo "error: git is required but not installed" >&2
    exit 1
fi

git_dir_abs="$(cd "$(git rev-parse --git-dir)" && pwd)"
git_common_dir_abs="$(cd "$(git rev-parse --git-common-dir)" && pwd)"

if [[ "$git_dir_abs" != "$git_common_dir_abs" ]]; then
    # Worktree: symlink the main checkout's reference directories rather than
    # re-cloning them. This works with both git-cloned and tarball-extracted
    # sources and avoids duplicating gigabytes of read-only data per worktree.
    main_repo="$(dirname "$git_common_dir_abs")"
    for name in pokeemerald mgba; do
        src="$main_repo/$name"
        if [[ ! -d "$src" ]]; then
            echo "error: $src missing; run init.sh in the main checkout ($main_repo) first." >&2
            exit 1
        fi
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

clone_if_missing() {
    local source="$1"
    local target="$2"
    if [[ -d "$target/.git" ]]; then
        echo "skip: $target already present"
        return
    fi
    echo "cloning $source -> $target"
    git clone "$source" "$target"
}

echo "main checkout detected; cloning from GitHub"
clone_if_missing "$POKEEMERALD_REMOTE" "pokeemerald"
clone_if_missing "$MGBA_REMOTE" "mgba"

echo "done."
