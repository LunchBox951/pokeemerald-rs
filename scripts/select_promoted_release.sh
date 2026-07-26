#!/usr/bin/env bash
# select_promoted_release.sh — identify the exact release/* branch promoted
# by a given push to a channel branch (unstable/stable/main).
#
# Used by .github/workflows/promote.yml's "next rung" step. Extracted into
# its own script so the selection logic can be exercised directly by
# scripts/test_select_promoted_release.py (issue #135).
#
# Why this exists (issue #135): RELEASE.md's release/* model explicitly keeps
# old release/* branches around as long-lived maintenance lines, so more than
# one release/* branch can be an ancestor of a channel at once. Picking "the
# release/* branch with the newest committer date that is an ancestor of the
# channel" describes branch *activity*, not which branch the triggering push
# actually promoted -- a stale maintenance line with a newer tip can shadow
# the release branch that was really merged. Instead, this walks the
# triggering merge commit's own ancestry: promote.yml requires promotions to
# land as merge commits (see its header comment), so the second parent of the
# push's commit is the tip of whatever branch was merged. Binding selection
# to that ancestry -- not to commit dates or open/closed PR state on other
# lines -- makes the answer depend only on the push that triggered the run.
#
# Usage: select_promoted_release.sh <merge-commit-sha>
# Prints the release/* branch name promoted by that merge commit to stdout,
# or prints nothing (exit 0) if the commit is not a two-parent merge, or its
# second parent is not reachable from any known release/* branch.
#
# Requires: a git checkout with full history and `origin/release/*` remote-
# tracking refs present (as produced by `actions/checkout` with
# `fetch-depth: 0`, matched by promote.yml).
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <merge-commit-sha>" >&2
  exit 2
fi

merge_sha="$1"

# A promotion push is required (see promote.yml's ASSUMPTIONS) to be a merge
# commit whose second parent is the release/* branch's tip at merge time. A
# single-parent commit means this push did not come from a release/*
# promotion merge -- nothing to select.
second_parent="$(git rev-parse --verify -q "${merge_sha}^2" 2>/dev/null || true)"
if [ -z "${second_parent}" ]; then
  exit 0
fi

# Find the release/* branch this merge actually promoted: the one for which
# the merged commit is an ancestor of (or equal to) its current tip. This is
# anchored to the triggering push's own ancestry, so neither another release
# branch's newer tip commit date nor its open/closed PR state can change the
# answer.
for ref in $(git for-each-ref --format='%(refname:short)' 'refs/remotes/origin/release/*'); do
  name="${ref#origin/}"
  if git merge-base --is-ancestor "${second_parent}" "origin/${name}"; then
    printf '%s\n' "${name}"
    exit 0
  fi
done
