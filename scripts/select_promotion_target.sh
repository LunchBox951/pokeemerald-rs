#!/usr/bin/env bash
# select_promotion_target.sh — print the lowest rung of the channel ladder
# (unstable -> stable -> main) that does not yet contain the given release/*
# branch's tip, or nothing (exit 0) if every rung already contains it.
#
# Used by .github/workflows/promote.yml. Routing by "lowest uncleared rung"
# handles both cases with one rule (RELEASE.md: a branch that moves after
# clearing a rung re-clears it):
#   * a tip just merged into a rung is contained there but not above ->
#     the next rung up (a normal next-rung promotion);
#   * a tip that advanced past what a rung took is not contained even
#     there -> that same rung again (a re-clear), never a higher rung the
#     new commits have not earned.
#
# Requires origin/unstable, origin/stable, origin/main remote-tracking refs
# (actions/checkout with fetch-depth: 0, matched by promote.yml).
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <release-branch-or-commit>" >&2
  exit 2
fi

# Accept a branch name (routed by its origin tip) or a raw commit OID —
# promote.yml re-routes a created PR by the exact head OID it captured.
tip="$(git rev-parse --verify -q "origin/$1" \
  || git rev-parse --verify -q "$1^{commit}")" || {
  echo "unknown branch or commit: $1" >&2
  exit 2
}

for rung in unstable stable main; do
  if ! git rev-parse --verify -q "origin/${rung}" >/dev/null; then
    echo "missing channel ref origin/${rung}; cannot route promotion." >&2
    exit 2
  fi
  if ! git merge-base --is-ancestor "${tip}" "origin/${rung}"; then
    printf '%s\n' "${rung}"
    exit 0
  fi
done
