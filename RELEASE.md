# Release policy & checklist

The canonical release policy for `pokeemerald-rs`: the direct branch ladder,
per-rung gates, automation schedule, version scheme, and hotfix path. CI enforces
objective rules; owners gate the player-facing rungs `(gated-by-default)`.

## Direct branch ladder

| Branch | Channel | Audience | Only promotion source |
|---|---|---|---|
| `dev` | developer | integration | normal pull requests |
| `unstable` | nightly | freshest release-ready build | `dev` |
| `stable` | beta | maintainer-reviewed candidate | `unstable` |
| `main` | stable | official player release | `stable` |

Promotion is direct: `dev -> unstable -> stable -> main`. A channel accepts no
fork, staging branch, or rung-skipping source. Every promotion uses a merge
commit so channel ancestry remains auditable. Normal contributions target `dev`.

## Promotion schedule

`.github/workflows/promote.yml` runs in `America/Toronto`, away from the daytime
Birch automation window:

| Local time | Operation |
|---|---|
| 00:17 daily | open/reconcile `dev -> unstable` |
| 00:47 daily | open/reconcile `unstable -> stable` |
| 01:17 daily | open/reconcile `stable -> main` |
| 02:17 daily | attempt to merge `dev -> unstable` |

The opener uses a dedicated least-privilege GitHub App, so its pull requests run
normal CI without the approval hold applied to `GITHUB_TOKEN`-created PRs. An
existing direct-channel PR follows its source branch automatically: updating
`dev`, `unstable`, or `stable` updates the corresponding PR and invalidates stale
checks or approvals.

Only `unstable` has an automated merge path. The 02:17 job performs a fresh,
non-admin merge attempt only when the exact App-created PR, live refs, required
checks, readiness result, and review threads all satisfy the ruleset. If blocked,
it records the reason and waits for the next scheduled attempt; it never leaves
GitHub native auto-merge armed. `stable` and `main` are always merged manually.

## Official-ROM readiness

`unstable` remains dormant until the native release can extract its required data
from an owner-supplied, lawfully obtained official Pokémon Emerald GBA file. The
ROM, its path, and its contents never enter the repository, Actions, artifacts,
logs, or releases.

Birch validates the exact current `dev` SHA from a clean checkout during its
10:00-20:00 local operating window by running the canonical ROM load/extract
command. After a zero exit, Birch runs the local
`scripts/record_nightly_readiness.py <tested-dev-sha>` command. The recorder
requires the local `gh` credential to authenticate as the repository owner,
rechecks that the tested SHA is still the live `dev` tip and that its ordinary CI
and CodeQL checks passed, then records `release-readiness` on that SHA. The owner
credential must never be stored in Actions; ordinary workflows therefore cannot
impersonate the readiness publisher. If `dev` advances, the result does not
follow it: the new tip must be validated during a later Birch run, and the next
off-hours promotion waits.

The exact product command belongs to the extraction implementation tracked in
GitHub issue #122 `(constitution-vs-roadmap)`. Until it exists and Birch can run
it successfully, no `release-readiness` status is recorded and no nightly PR
opens.

## Per-rung gates

### `dev` — developer integration

- pull request required, with no approving review required;
- `merge-gate / dev`, dependency review, and all CodeQL languages pass;
- the branch is current with `dev` before merge;
- every review thread is resolved;
- merge or squash is allowed; direct/force pushes and deletion are blocked.

### `dev -> unstable` — nightly

- promotion-App-authored PR from exact same-repository source `dev`, enforced by
  `source-gate / unstable`;
- SHA-bound `release-readiness` exists for the current `dev` tip;
- `merge-gate / unstable`, dependency review, and all CodeQL languages pass;
- every review thread is resolved;
- no approval is required;
- only the scheduled App may auto-merge, using a merge commit without bypass.

### `unstable -> stable` — beta

- promotion-App-authored PR from exact same-repository source `unstable`, and
  that tip is the merge commit of the preceding App-created `dev -> unstable`
  PR, enforced by `source-gate / stable`;
- `merge-gate / stable`, dependency review, and all CodeQL languages pass;
- every review thread is resolved;
- one current CODEOWNER approval and a manual merge commit are required.

There is no hard clock. Maintainers may leave ordinary changes soaking as long as
needed or promote security-sensitive changes promptly once evidence and review
are sufficient.

### `stable -> main` — official release

- promotion-App-authored PR from exact same-repository source `stable`, and that
  tip is the merge commit of the preceding App-created `unstable -> stable` PR,
  enforced by `source-gate / main`;
- `merge-gate / main`, dependency review, and all CodeQL languages pass;
- every review thread is resolved;
- one current CODEOWNER approval and a manual merge commit are required;
- full/soak E2E, clean-machine artifact, release notes, active waivers, and owner
  playtest evidence required by V-2, V-3, R-1, R-2, H-1, and H-4 are attached.

No clock substitutes for validation. Maintainers may expedite a security release,
but never bypass its objective checks, source gate, review, or release evidence.

## Promotion PR shape

Promotion PRs are mechanical: source and target in the title; live candidate and
target SHAs; gate references; `release` label; and `needs-review` plus
`needs-operator` for stable/main. Feature work lands on `dev`, never inside a
promotion PR. A blocked nightly is fixed on `dev`, which updates the open PR.

## Repository controls

Repository rulesets are the enforcement source of truth:

| Target | Required checks | Reviews | Merge methods |
|---|---|---|---|
| `dev` | `merge-gate / dev`, `dependency-review`, `codeql (actions)`, `codeql (python)`, `codeql (rust)` | 0 | merge, squash |
| `unstable` | dev checks plus `source-gate / unstable`, `release-readiness` | 0 | merge |
| `stable` | dev checks plus `source-gate / stable` | 1 CODEOWNER | merge |
| `main` | dev checks plus `source-gate / main` | 1 CODEOWNER | merge |

Every ruleset also requires a pull request and resolved review threads, dismisses
stale approvals, blocks deletion and non-fast-forward updates, has no standing
bypass actor, and blocks critical CodeQL alerts. `dev` uses GitHub's strict
up-to-date mode. Direct channel heads cannot absorb their target's merge commits
without polluting the preceding rung, so channel rules use loose status mode plus
target-specific contexts, exact-source/provenance checks, live SHA rebinding, and
single-PR enforcement.

Repository-native auto-merge is disabled. Default Actions permissions are
read-only and Actions cannot approve pull requests. The promotion App is installed
only on this repository and requests only Checks read, Commit statuses read,
Contents write, Pull requests write, and Metadata read. Secret scanning and push
protection remain enabled. CodeQL advanced setup scans Rust, Python, and Actions;
critical alerts block every protected branch.

`PROMOTION_APP_LOGIN` is a non-secret repository variable containing the exact
App bot login. The required source gate uses it to reject user- or other-bot-
created channel PRs and to authenticate preceding-rung provenance.

The separate readiness trust boundary is the repository owner's local Birch
credential. It is not an Actions secret. Promotion automation accepts only the
latest successful `release-readiness` status created by that owner identity, so a
repository workflow cannot forge official-ROM evidence.

### Control-plane bootstrap

`pull_request` workflows execute from the base branch, so new channel workflows
must exist on `unstable`, `stable`, and `main` before their contexts become
required. A migration may temporarily grant only the owner a bypass, fast-forward
one workflow-only bootstrap commit onto one channel, verify its new push checks,
and remove the bypass immediately before moving to the next branch. Record the
bootstrap commit SHAs in the migration PR. This is a one-time audited exception;
normal rulesets retain no bypass actor.

## Version scheme — `vFINAL.MAJOR.MINOR.PATCH`

The canonical version lives in [`VERSION`](VERSION) without the `v` prefix; tags
and Releases add it. Versions compare lexicographically as four unsigned ints.

| Component | Bump when | Resets | Authority |
|---|---|---|---|
| `PATCH` | fixes, docs, CI, ledger, packaging — normal flow | — | normal PR flow |
| `MINOR` | completed milestone / user-visible capability | `PATCH -> 0` | maintainer / owner |
| `MAJOR` | large project phase or breaking repository contract | `MINOR`, `PATCH -> 0` | maintainer |
| `FINAL` | project agreed complete (`0 -> 1`) | `MAJOR`, `MINOR`, `PATCH -> 0` | maintainer only |

Every pull request must increase `VERSION`; an unchanged or lower version fails
the policy gate. A `MAJOR` or `MINOR` bump must reset lower components to zero.
Promotion does not create a new product change, so its source version need only be
strictly newer than the target channel's current version.

## The `FINAL` gate

`FINAL` is the project-completion epoch and is never bumped by automation. A
`FINAL` bump requires `docs/release/final-gate-approved.md` to be added or changed
in the same PR, naming the exact approved version and date:

```text
Approved version: 1.0.0.0
Date: 2026-07-25
```

`scripts/version_check.py` rejects a stale marker, malformed marker, or marker for
another version. v1 ships only when every row in `docs/acceptance/v1.md` is done
or has a recorded waiver and the H-1 operator playtest is signed.

## Hotfixes

Security and serious player-facing fixes still follow the direct ladder. Land the
minimal fix on `dev`, let Birch validate the candidate, and promote it through
unstable, stable, and main. Owners may approve stable/main immediately once the
required evidence is green; there is no time gate and no direct-to-main bypass.
