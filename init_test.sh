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

echo "=== test 8: a linked-worktree reference at the wrong commit is a hard error ==="

# .git is a FILE in a linked worktree; a `.git` directory test would
# misclassify it as an unverifiable tarball and accept it unchecked.
aux_clone="$workdir/aux-clone"
git_c clone --quiet "$fake_poke" "$aux_clone"
checkout8="$workdir/checkout8"
make_main_checkout "$checkout8"
git_c -C "$aux_clone" worktree add --quiet --detach "$checkout8/pokeemerald" "$sha_b_poke"

set +e
stderr_out="$(cd "$checkout8" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a worktree-linked reference at the wrong commit"
echo "$stderr_out" | grep -qF "$sha_b_poke" || fail "worktree-reference error didn't name the actual SHA; got: $stderr_out"

echo "ok: linked-worktree reference at the wrong commit was rejected"

echo "=== test 9: a sparse reference checkout is a hard error ==="

checkout9="$workdir/checkout9"
make_main_checkout "$checkout9"
(
    cd "$checkout9"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout9/pokeemerald" sparse-checkout init 2>/dev/null

set +e
stderr_out="$(cd "$checkout9" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 on a sparse reference checkout"
echo "$stderr_out" | grep -qi "sparse" || fail "sparse-checkout error missing; got: $stderr_out"

echo "ok: sparse reference checkout was rejected"

echo "=== test 10: a git-ignored planted file is still a hard error ==="

# The extraction pipeline reads the filesystem, not the index, so a file
# hidden by an ignore rule (.git/info/exclude, host excludes) must still
# read as dirt.
checkout10="$workdir/checkout10"
make_main_checkout "$checkout10"
(
    cd "$checkout10"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
echo "planted.png" >"$checkout10/pokeemerald/.git/info/exclude"
echo "fake" >"$checkout10/pokeemerald/planted.png"

set +e
stderr_out="$(cd "$checkout10" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a git-ignored planted file in the reference tree"
echo "$stderr_out" | grep -qF "planted.png" || fail "ignored-file error didn't name the planted file; got: $stderr_out"

echo "ok: git-ignored planted file was rejected"

echo "=== test 11: a reference tree without git metadata is a hard error ==="

checkout11="$workdir/checkout11"
make_main_checkout "$checkout11"
mkdir -p "$checkout11/pokeemerald"
echo "opaque" >"$checkout11/pokeemerald/VERSION"

set +e
stderr_out="$(cd "$checkout11" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 on a reference tree with no git metadata"
echo "$stderr_out" | grep -qi "no git metadata" || fail "no-metadata error missing; got: $stderr_out"

echo "ok: unverifiable reference tree was rejected (fail closed)"

echo "=== test 12: status.showUntrackedFiles=no cannot hide an injected file ==="

checkout12="$workdir/checkout12"
make_main_checkout "$checkout12"
(
    cd "$checkout12"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout12/pokeemerald" config status.showUntrackedFiles no
echo "fake" >"$checkout12/pokeemerald/injected_asset.png"

set +e
stderr_out="$(cd "$checkout12" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with an injected file hidden by showUntrackedFiles=no"
echo "$stderr_out" | grep -qF "injected_asset.png" || fail "injected-file error didn't name the file; got: $stderr_out"

echo "ok: showUntrackedFiles=no could not hide the injected file"

echo "=== test 13: assume-unchanged edits cannot evade the clean check ==="

checkout13="$workdir/checkout13"
make_main_checkout "$checkout13"
(
    cd "$checkout13"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout13/pokeemerald" update-index --assume-unchanged VERSION
echo "TAMPERED" >>"$checkout13/pokeemerald/VERSION"

set +e
stderr_out="$(cd "$checkout13" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with an assume-unchanged tampered file"
echo "$stderr_out" | grep -qi "index-hidden" || fail "index-hidden error missing; got: $stderr_out"

echo "ok: assume-unchanged tampering was rejected"

echo "=== test 14: replacement refs are rejected ==="

checkout14="$workdir/checkout14"
make_main_checkout "$checkout14"
(
    cd "$checkout14"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout14/pokeemerald" fetch --quiet origin
git_c -C "$checkout14/pokeemerald" replace -f "$sha_a_poke" "$sha_b_poke"

set +e
stderr_out="$(cd "$checkout14" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a replacement ref shadowing the pin"
echo "$stderr_out" | grep -qi "replace" || fail "replacement-ref error missing; got: $stderr_out"

echo "ok: replacement refs were rejected"

echo "=== test 15: local content-filter config is rejected ==="

checkout15="$workdir/checkout15"
make_main_checkout "$checkout15"
(
    cd "$checkout15"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout15/pokeemerald" config filter.launder.clean cat

set +e
stderr_out="$(cd "$checkout15" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a local content filter configured"
echo "$stderr_out" | grep -qi "filter" || fail "filter-config error missing; got: $stderr_out"

echo "ok: local content-filter config was rejected"

echo "=== test 16: a tamper is caught even with core.fsmonitor configured ==="

checkout16="$workdir/checkout16"
make_main_checkout "$checkout16"
(
    cd "$checkout16"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout16/pokeemerald" config core.fsmonitor /bin/true
echo "TAMPERED" >>"$checkout16/pokeemerald/VERSION"

set +e
stderr_out="$(cd "$checkout16" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a tampered file under core.fsmonitor"
echo "$stderr_out" | grep -qi "not clean" || fail "fsmonitor-bypass tamper not caught; got: $stderr_out"

echo "ok: tamper caught despite core.fsmonitor config"

echo "=== test 17: local info/attributes overrides are rejected ==="

checkout17="$workdir/checkout17"
make_main_checkout "$checkout17"
(
    cd "$checkout17"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
echo "VERSION filter=launder" >"$checkout17/pokeemerald/.git/info/attributes"

set +e
stderr_out="$(cd "$checkout17" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a local info/attributes override"
echo "$stderr_out" | grep -qi "attribute" || fail "info/attributes error missing; got: $stderr_out"

echo "ok: local info/attributes override was rejected"

echo "=== test 18: a host-global clean filter cannot launder tampered bytes ==="

# Global attributesFile binds filter=launder to VERSION; the global filter's
# clean command maps any bytes back to the committed content. Verification
# neutralizes the global attributes file, so the tamper must still surface.
fakehome18="$workdir/fakehome18"
mkdir -p "$fakehome18"
HOME="$fakehome18" git config --global user.name "init_test"
HOME="$fakehome18" git config --global user.email "init_test@example.invalid"
echo "VERSION filter=launder" >"$fakehome18/attributes"
HOME="$fakehome18" git config --global core.attributesFile "$fakehome18/attributes"
HOME="$fakehome18" git config --global filter.launder.clean "printf 'commit A\n'"

checkout18="$workdir/checkout18"
make_main_checkout "$checkout18"
(
    cd "$checkout18"
    HOME="$fakehome18" POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
echo "TAMPERED" >>"$checkout18/pokeemerald/VERSION"

set +e
stderr_out="$(cd "$checkout18" && HOME="$fakehome18" \
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a tamper laundered by a global clean filter"
echo "$stderr_out" | grep -qi "not clean" || fail "global-filter tamper not caught; got: $stderr_out"

echo "ok: host-global clean filter could not launder the tamper"

echo "=== test 19: core.checkStat=minimal cannot hide a same-size tamper ==="

checkout19="$workdir/checkout19"
make_main_checkout "$checkout19"
(
    cd "$checkout19"
    POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
git -C "$checkout19/pokeemerald" config core.checkStat minimal
# Same-size content tamper with the original mtime restored: the stat
# cache sees nothing, only a byte comparison can catch it.
stamp="$workdir/stamp19"
touch -r "$checkout19/pokeemerald/VERSION" "$stamp"
printf 'commit B\n' >"$checkout19/pokeemerald/VERSION"
touch -r "$stamp" "$checkout19/pokeemerald/VERSION"

set +e
stderr_out="$(cd "$checkout19" && POKEEMERALD_REMOTE="$fake_poke" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha_a_poke" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 on a same-size tamper under core.checkStat=minimal"
echo "$stderr_out" | grep -qF "VERSION" || fail "checkStat tamper error didn't name the file; got: $stderr_out"

echo "ok: stat-cache evasion could not hide the tamper"

echo "=== test 20: a committed filter binding cannot launder tampered bytes ==="

# The filter attribute lives in the TRACKED .gitattributes; the filter
# command comes from host-global config. Status would read the laundered
# bytes as clean; the byte comparison must not.
fake_poke20="$workdir/remote-pokeemerald-20"
sha20="$(make_fake_remote "$fake_poke20")"
echo "VERSION filter=launder" >"$fake_poke20/.gitattributes"
git_c -C "$fake_poke20" add .gitattributes
git_c -C "$fake_poke20" commit --quiet -m "attributes"
sha20="$(git -C "$fake_poke20" rev-parse HEAD)"

fakehome20="$workdir/fakehome20"
mkdir -p "$fakehome20"
HOME="$fakehome20" git config --global user.name "init_test"
HOME="$fakehome20" git config --global user.email "init_test@example.invalid"
HOME="$fakehome20" git config --global filter.launder.clean "printf 'commit A\n'"

checkout20="$workdir/checkout20"
make_main_checkout "$checkout20"
(
    cd "$checkout20"
    HOME="$fakehome20" POKEEMERALD_REMOTE="$fake_poke20" MGBA_REMOTE="$fake_mgba" \
        POKEEMERALD_REF="$sha20" MGBA_REF="$sha_a_mgba" \
        ./init.sh >/dev/null
)
printf 'commit B\n' >"$checkout20/pokeemerald/VERSION"

set +e
stderr_out="$(cd "$checkout20" && HOME="$fakehome20" \
    POKEEMERALD_REMOTE="$fake_poke20" MGBA_REMOTE="$fake_mgba" \
    POKEEMERALD_REF="$sha20" MGBA_REF="$sha_a_mgba" \
    ./init.sh 2>&1 >/dev/null)"
status=$?
set -e

[[ "$status" -ne 0 ]] || fail "init.sh exited 0 with a tamper laundered by a committed filter binding"
echo "$stderr_out" | grep -qF "VERSION" || fail "committed-filter tamper error didn't name the file; got: $stderr_out"

echo "ok: committed filter binding could not launder the tamper"

echo "all tests passed"
