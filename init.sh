#!/usr/bin/env bash
# init.sh — bootstrap the pokeemerald-rewrite reference checkouts.
#
# Clones pret/pokeemerald and mgba-emu/mgba into ./pokeemerald and ./mgba so
# the Rust rewrite has the upstream C source available for reference. Both
# directories are gitignored and are never committed back to this repo.
#
# Behaviour:
#   * In the main checkout: clones from GitHub.
#   * In a git worktree:    clones from the main checkout's local pokeemerald/
#                           and mgba/ directories (fast, no network, hardlinks
#                           via git's default --local behaviour).

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
    # Worktree: main repo's working tree is the parent of the common git dir.
    main_repo="$(dirname "$git_common_dir_abs")"
    pokeemerald_source="$main_repo/pokeemerald"
    mgba_source="$main_repo/mgba"
    for src in "$pokeemerald_source" "$mgba_source"; do
        if [[ ! -d "$src/.git" ]]; then
            echo "error: $src not found. Run init.sh in the main checkout ($main_repo) first." >&2
            exit 1
        fi
    done
    echo "worktree detected; cloning from $main_repo"
else
    pokeemerald_source="$POKEEMERALD_REMOTE"
    mgba_source="$MGBA_REMOTE"
    echo "main checkout detected; cloning from GitHub"
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

clone_if_missing "$pokeemerald_source" "pokeemerald"
clone_if_missing "$mgba_source" "mgba"

echo "done."
