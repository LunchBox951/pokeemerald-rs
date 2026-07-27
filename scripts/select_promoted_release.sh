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
# second parent is not reachable from any known release/* branch. Exits 4
# (name still printed) if the identified branch has advanced past the merged
# commit — the caller must re-clear the current rung rather than promote the
# unvetted tip. Exits 3 (message on stderr) if the topology is genuinely
# ambiguous — more than one release/* branch equally claims the merged commit.
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

# Once refs move, current ref state cannot recover which branch a past merge
# named: a consolidated line that *contains* the commit and the true branch
# that *advanced past* it look identical from topology alone. So identity
# comes from what the merge itself recorded — its commit subject (GitHub PR
# merges and plain `git merge` both name the merged branch) — and topology is
# only a fallback for a rewritten or unparseable subject.
#
# Exit contract:
#   0 + name   branch identified, tip still at the merged commit — promote.
#   0 + empty  nothing promoted (not a merge / no release/* involved).
#   4 + name   branch identified but it ADVANCED past the merged commit; its
#              new commits have not cleared the current rung — the caller
#              must re-clear that rung, not advance the unvetted tip.
#   3          genuinely ambiguous — refuse to guess.

emit() { # <name> — classify by tip position and emit with the right exit code
  local name="$1" tip
  tip="$(git rev-parse --verify -q "origin/${name}")"
  if [ "${tip}" = "${second_parent}" ]; then
    printf '%s\n' "${name}"
    exit 0
  fi
  echo "note: ${name} advanced past the promoted commit ${second_parent}; its tip must re-clear the current rung." >&2
  printf '%s\n' "${name}"
  exit 4
}

# --- Primary: the head ref of the PR associated with the merge commit. ----
# GitHub records the association server-side, so it survives edited or
# PR-title-style merge subjects. Requires `gh` + a token; unavailable (e.g.
# in the offline test suite) or empty results fall through to the subject.
if command -v gh >/dev/null 2>&1 && [ -n "${GH_TOKEN:-}${GITHUB_TOKEN:-}" ]; then
  pr_head="$(gh api "repos/{owner}/{repo}/commits/${merge_sha}/pulls" \
    --jq '[.[] | select(.merge_commit_sha == "'"${merge_sha}"'") | .head.ref] | first // empty' \
    2>/dev/null || true)"
  if [[ "${pr_head}" == release/* ]] \
    && git rev-parse --verify -q "origin/${pr_head}" >/dev/null \
    && git merge-base --is-ancestor "${second_parent}" "origin/${pr_head}"; then
    emit "${pr_head}"
  fi
fi

# --- Secondary: the branch name the merge commit's subject recorded. -------
subject="$(git log -1 --format=%s "${merge_sha}")"
candidate=""
case "${subject}" in
  "Merge pull request #"*" from "*)
    rest="${subject##* from }"
    if [[ "${rest}" == release/* ]]; then
      candidate="${rest}"
    else
      candidate="${rest#*/}" # strip the head-repo owner segment
    fi
    ;;
  "Merge branch '"*)
    rest="${subject#Merge branch \'}"
    candidate="${rest%%\'*}"
    ;;
  "Merge remote-tracking branch '"*)
    rest="${subject#Merge remote-tracking branch \'}"
    candidate="${rest%%\'*}"
    candidate="${candidate#origin/}"
    ;;
esac

if [[ "${candidate}" == release/* ]] \
  && git rev-parse --verify -q "origin/${candidate}" >/dev/null \
  && git merge-base --is-ancestor "${second_parent}" "origin/${candidate}"; then
  emit "${candidate}"
fi
# A recorded branch that no longer exists or no longer contains the merged
# commit (rewritten history) falls through to the topology fallback.

# --- Fallback: topology. Only trustworthy when it is unambiguous. ----------
exact=()
containing=()
for ref in $(git for-each-ref --format='%(refname:short)' 'refs/remotes/origin/release/*'); do
  name="${ref#origin/}"
  tip="$(git rev-parse --verify -q "origin/${name}")"
  if [ "${tip}" = "${second_parent}" ]; then
    exact+=("${name}")
  elif git merge-base --is-ancestor "${second_parent}" "origin/${name}"; then
    containing+=("${name}")
  fi
done

if [ "$(( ${#exact[@]} + ${#containing[@]} ))" -eq 0 ]; then
  exit 0
fi

# A lone candidate — stationary or advanced — is unambiguous. Anything else
# (two refs on the commit, or a ref AT the commit plus a ref PAST it, which
# is indistinguishable from a decoy at the old tip of an advanced branch)
# is not decidable from topology: refuse to guess.
if [ "${#exact[@]}" -eq 1 ] && [ "${#containing[@]}" -eq 0 ]; then
  emit "${exact[0]}"
fi
if [ "${#exact[@]}" -eq 0 ] && [ "${#containing[@]}" -eq 1 ]; then
  emit "${containing[0]}"
fi

echo "ambiguous promotion: ${merge_sha}^2 (${second_parent}) matches multiple release/* branches: ${exact[*]:-} ${containing[*]:-}" >&2
echo "refusing to guess -- resolve the release/* topology, then re-run." >&2
exit 3
