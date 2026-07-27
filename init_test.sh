#!/usr/bin/env bash
# init_test.sh — regression test for init.sh's pinned-revision bootstrap
# (issue #136).
#
# Builds synthetic local "upstream" remotes with an advancing default branch
# and drives init.sh against them (via its POKEEMERALD_REMOTE/MGBA_REMOTE/
# POKEEMERALD_REF/MGBA_REF environment overrides — see init.sh's header) to
# prove, without touching the real network or the real pokeemerald/mgba
# checkouts:
#
#   1. Two clean bootstraps of the same pin resolve to the identical upstream
#      SHA even though the remote's default branch advanced in between.
#   2. An existing checkout that doesn't match the pin is a hard error with
#      recovery instructions, not a silent skip.
#   3. Worktrees still symlink the main checkout's validated references.
#
# Run with: ./init_test.sh  (exits non-zero and prints the failing assertion
# on any failure; prints "all tests passed" on success).

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
init_sh="$script_dir/init.sh"

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

git_c() {
    # A commit with a fixed, test-local identity so this doesn't depend on
    # the host's git config.
    git -c user.name="init_test" -c user.email="init_test@example.invalid" "$@"
}

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

# make_fake_remote <dir> — creates a tiny non-bare git repo with one commit
# and echoes its HEAD SHA. Callers advance it further with commit_more.
make_fake_remote() {
    local dir="$1"
    mkdir -p "$dir"
    git_c -C "$dir" init --quiet -b main
    echo "commit A" >"$dir/VERSION"
    git_c -C "$dir" add VERSION
    git_c -C "$dir" commit --quiet -m "commit A"
    git -C "$dir" rev-parse HEAD
}

# commit_more <dir> <message> — advances a fake remote's default branch and
# echoes the new HEAD SHA.
commit_more() {
    local dir="$1" msg="$2"
    echo "$msg" >>"$dir/VERSION"
    git_c -C "$dir" add VERSION
    git_c -C "$dir" commit --quiet -m "$msg"
    git -C "$dir" rev-parse HEAD
}

# make_main_checkout <dir> — a git repo containing init.sh, playing the role
# of a main checkout (git-dir == git-common-dir) for init.sh's worktree
# detection.
make_main_checkout() {
    local dir="$1"
    mkdir -p "$dir"
    cp "$init_sh" "$dir/init.sh"
    chmod +x "$dir/init.sh"
    git_c -C "$dir" init --quiet -b main
    git_c -C "$dir" add init.sh
    git_c -C "$dir" commit --quiet -m "init"
}

echo "=== test 1: pinned reproducibility across an advancing remote ==="

fake_poke="$workdir/remote-pokeemerald"
fake_mgba="$workdir/remote-mgba"
sha_a_poke="$(make_fake_remote "$fake_poke")"
sha_a_mgba="$(make_fake_remote "$fake_mgba")"

checkout1="$workdir/checkout1"
make_main_checkout "$checkout1"
(
    cd "$checkout1"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
head1="$(git -C "$checkout1/pokeemerald" rev-parse HEAD)"
[[ "$head1" == "$sha_a_poke" ]] || fail "checkout1 resolved to $head1, expected pinned $sha_a_poke"

sha_b_poke="$(commit_more "$fake_poke" "commit B (advanced default branch)")"
[[ "$sha_b_poke" != "$sha_a_poke" ]] || fail "fake remote did not advance"

checkout2="$workdir/checkout2"
make_main_checkout "$checkout2"
(
    cd "$checkout2"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
head2="$(git -C "$checkout2/pokeemerald" rev-parse HEAD)"
[[ "$head2" == "$sha_a_poke" ]] || fail "checkout2 resolved to $head2, expected pinned $sha_a_poke (remote had moved to $sha_b_poke)"
[[ "$head1" == "$head2" ]] || fail "checkout1 ($head1) and checkout2 ($head2) diverged despite an identical pin"

echo "ok: both clean bootstraps pinned to $sha_a_poke, unaffected by the remote's advance to $sha_b_poke"

echo "=== test 2: a checkout that doesn't match the pin is a hard error ==="

checkout3="$workdir/checkout3"
make_main_checkout "$checkout3"
git_c -C "$checkout3" clone --quiet "$fake_poke" "$checkout3/pokeemerald"
git -C "$checkout3/pokeemerald" -c advice.detachedHead=false checkout --quiet "$sha_b_poke"

set +e
stderr_out="$(cd "$checkout3" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 on a pin mismatch; expected a hard failure"
echo "$stderr_out" | grep -qF "$sha_b_poke" || fail "mismatch error didn't name the actual SHA ($sha_b_poke); got: $stderr_out"
echo "$stderr_out" | grep -qF "$sha_a_poke" || fail "mismatch error didn't name the pinned SHA ($sha_a_poke); got: $stderr_out"
echo "$stderr_out" | grep -qi "rm -rf" || fail "mismatch error didn't give recovery instructions; got: $stderr_out"

still_head="$(git -C "$checkout3/pokeemerald" rev-parse HEAD)"
[[ "$still_head" == "$sha_b_poke" ]] || fail "init.sh mutated the mismatched checkout instead of just failing"

echo "ok: mismatched checkout was rejected with recovery instructions, and left untouched"

echo "=== test 3: worktrees still symlink the main checkout's references ==="

worktree1="$workdir/worktree1"
git_c -C "$checkout1" worktree add --quiet "$worktree1" -b wt-branch >/dev/null
(
    cd "$worktree1"
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" ./init.sh >/dev/null
)
[[ -L "$worktree1/pokeemerald" ]] || fail "worktree pokeemerald is not a symlink"
[[ -L "$worktree1/mgba" ]] || fail "worktree mgba is not a symlink"
linked_head="$(git -C "$worktree1/pokeemerald" rev-parse HEAD)"
[[ "$linked_head" == "$sha_a_poke" ]] || fail "worktree symlink resolved to $linked_head, expected the main checkout's pinned $sha_a_poke"

echo "ok: worktree symlinked the main checkout's validated pokeemerald/mgba"

echo "=== test 4: a dirty-but-pinned checkout is a hard error ==="

checkout4="$workdir/checkout4"
make_main_checkout "$checkout4"
(
    cd "$checkout4"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
# HEAD stays at the pin, but the tree is tampered with: a tracked edit and
# an untracked file. HEAD equality alone must not launder this.
echo "MALICIOUS EDIT" >>"$checkout4/pokeemerald/VERSION"
echo "planted" >"$checkout4/pokeemerald/EXTRA_ASSET"

set +e
stderr_out="$(cd "$checkout4" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 on a dirty pinned checkout; expected a hard failure"
echo "$stderr_out" | grep -qi "not clean" || fail "dirty-tree error missing; got: $stderr_out"
echo "$stderr_out" | grep -qi "rm -rf" || fail "dirty-tree error didn't give recovery instructions; got: $stderr_out"

echo "ok: dirty-but-pinned checkout was rejected"

echo "=== test 5: a worktree over a mismatched main checkout is a hard error ==="

# checkout3's pokeemerald sits at sha_b (not the pin); a worktree over it
# must refuse to symlink the stale tree.
worktree3="$workdir/worktree3"
git_c -C "$checkout3" worktree add --quiet "$worktree3" -b wt-branch3 >/dev/null

set +e
stderr_out="$(cd "$worktree3" && POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "worktree init.sh exited 0 over a mismatched main checkout; expected a hard failure"
echo "$stderr_out" | grep -qF "$sha_b_poke" || fail "worktree mismatch error didn't name the actual SHA; got: $stderr_out"
[[ ! -L "$worktree3/pokeemerald" ]] || fail "worktree symlinked a mismatched reference anyway"

echo "ok: worktree refused to symlink a mismatched main checkout"

echo "=== test 6: a pre-existing rogue symlink in a worktree is a hard error ==="

worktree6="$workdir/worktree6"
git_c -C "$checkout1" worktree add --quiet "$worktree6" -b wt-branch6 >/dev/null
mkdir -p "$workdir/rogue-tree"
ln -s "$workdir/rogue-tree" "$worktree6/pokeemerald"

set +e
stderr_out="$(cd "$worktree6" && POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "worktree init.sh exited 0 with a rogue pokeemerald symlink; expected a hard failure"
echo "$stderr_out" | grep -qi "symlink" || fail "rogue-symlink error missing; got: $stderr_out"
[[ "$(readlink "$worktree6/pokeemerald")" == "$workdir/rogue-tree" ]] || fail "init.sh mutated the rogue symlink instead of just failing"

echo "ok: rogue pre-existing symlink was rejected"

echo "=== test 7: an autocrlf host config cannot produce a CRLF checkout ==="

fakehome="$workdir/fakehome"
mkdir -p "$fakehome"
HOME="$fakehome" git config --global core.autocrlf true
HOME="$fakehome" git config --global user.name "init_test"
HOME="$fakehome" git config --global user.email "init_test@example.invalid"

checkout7="$workdir/checkout7"
make_main_checkout "$checkout7"
(
    cd "$checkout7"
    HOME="$fakehome" POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
if grep -q $'\r' "$checkout7/pokeemerald/VERSION"; then
    fail "bootstrap under core.autocrlf=true produced CRLF bytes; trees are not byte-identical across platforms"
fi
# And the pinned checkout must still verify clean on a re-run.
(
    cd "$checkout7"
    HOME="$fakehome" POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)

echo "ok: autocrlf host config was overridden; checkout is LF byte-for-byte"

echo "all tests passed"
