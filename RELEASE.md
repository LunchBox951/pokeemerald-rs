# Release policy

This file owns the public branch ladder, per-rung gates, version scheme, operator evidence, and hotfix path. Workflow files and live rulesets enforce objective mechanics; GitHub promotion and playtest records own changing evidence `(gated-by-default, lean-docs)`.

## Direct branch ladder

| Branch | Channel | Audience | Only promotion source |
|---|---|---|---|
| `dev` | developer | integration | normal pull requests |
| `unstable` | nightly | freshest release-ready build | `dev` |
| `stable` | beta | owner-reviewed candidate | `unstable` |
| `main` | stable | official player release | `stable` |

Promotion is direct: `dev → unstable → stable → main`. No channel accepts a fork, staging branch, or rung-skipping source. Every promotion uses a merge commit so ancestry remains auditable. Normal contributions target `dev`.

The scheduled promotion App opens or reconciles one exact next-rung pull request. Only `dev → unstable` may auto-merge. `stable` and `main` always require current CODEOWNER approval and manual merge.

## Official-ROM readiness

The nightly remains dormant until the native release can extract required data from an owner-supplied, lawfully obtained official Pokémon Emerald ROM. The ROM, its path, and its contents never enter the repository, Actions, artifacts, logs, or releases.

An owner-local verifier runs the canonical load and extraction path from a clean checkout at the current `dev` SHA. After ordinary CI and CodeQL succeed, it records the `release-readiness` status on that exact SHA. A later `dev` commit requires new evidence. Repository workflows cannot create this owner-bound status.

The product command and remaining implementation live in GitHub roadmap work `(constitution-vs-roadmap)`. Until that command exists and passes, no readiness status exists and no nightly promotion advances.

## Per-rung gates

### `dev`

- A pull request is required; no approving review is required.
- `merge-gate / dev`, dependency review, and all CodeQL languages pass.
- The branch is current with `dev`, and every review thread is resolved.
- Merge or squash is allowed. Direct pushes, force pushes, and deletion are blocked.

### `dev → unstable`

- The dedicated promotion App authors the exact same-repository `dev` pull request.
- `source-gate / unstable` proves the source and App identity.
- Current SHA-bound `release-readiness`, `merge-gate / unstable`, dependency review, and CodeQL pass.
- Every review thread is resolved. No approval is required.
- Only the scheduled App may auto-merge, using a merge commit without bypass.

### `unstable → stable`

- The promotion App authors the exact same-repository `unstable` pull request.
- `source-gate / stable` proves that `unstable` descends from the preceding App-created nightly promotion.
- `merge-gate / stable`, dependency review, and CodeQL pass; every review thread is resolved.
- A passing unstable playtest issue exists for the current `VERSION`.
- One current CODEOWNER approval and a manual merge commit are required.

### `stable → main`

- The promotion App authors the exact same-repository `stable` pull request.
- `source-gate / main` proves that `stable` descends from the preceding App-created beta promotion.
- `merge-gate / main`, dependency review, and CodeQL pass; every review thread is resolved.
- A passing stable playtest issue exists for the current `VERSION`.
- Full and soak E2E, clean-machine artifacts, release notes, active waivers, and the evidence required by V-2, V-3, R-1, R-2, H-1, and H-4 are present.
- One current CODEOWNER approval and a manual merge commit are required.

After promotion, validate the main build through its own playtest issue. A defect at any rung receives a bug issue for that channel. The fix lands on `dev`, reaches a new nightly, and repeats every later gate. No clock or urgency permits a direct upper-channel patch.

## Playtest records

Open one playtest issue per player-channel build; `dev` is developer integration and is not playtested. Require `VERSION`, channel, comparison scope, an explicit pass or fail verdict, and notes carrying either `none` or the defects the session filed. An already-published release tag may provide additional identification; a separate SHA field is unnecessary because each `dev` change advances `VERSION`.

Compare the port side by side with the real game where applicable. File every defect as a separate linked bug, then close the playtest issue after the session whether it passed or failed. A new build receives a new issue; do not rewrite a failed record into a passing one.

Playtest issues record player feedback. Deterministic snapshot generation and byte comparison remain implementation and review tools under [`docs/snapshots.md`](docs/snapshots.md); snapshot hashes are not player signoff.

## Promotion pull requests

Promotion pull requests are mechanical. The title names source and target; the body records candidate and target SHAs when the pull request is created, gate references, and required evidence. Current GitHub refs and checks, not the initial body text, govern merge safety.

Apply `release` to every promotion. Apply `needs-review` and `needs-operator` to stable and main. Feature work never lands inside a promotion pull request.

## Repository controls

Live rulesets are the enforcement authority:

| Target | Additional target-specific checks | Reviews | Merge methods |
|---|---|---|---|
| `dev` | `merge-gate / dev` | 0 | merge, squash |
| `unstable` | `source-gate / unstable`, `release-readiness`, `merge-gate / unstable` | 0 | merge |
| `stable` | `source-gate / stable`, `merge-gate / stable` | 1 CODEOWNER | merge |
| `main` | `source-gate / main`, `merge-gate / main` | 1 CODEOWNER | merge |

Every rung also requires dependency review, CodeQL, a pull request, and resolved review threads. Rules dismiss stale approvals, block deletion and non-fast-forward updates, and provide no standing bypass. Repository-native auto-merge remains disabled.

A tag ruleset makes `v*` release tags immutable once created: no updates, no deletion, no bypass. The release workflow still creates the tag for its exact commit before publishing. Never create a `v*` tag by hand: a mistaken tag blocks that release until an admin suspends the ruleset, deletes the tag, and re-enables enforcement.

## Platform support and artifacts

Source builds and native CI support Linux, macOS, and Windows. Published archives currently target Linux and Windows. macOS packaging and platform-specific operator playtesting are not v1 gates; CI evidence carries the same unresolved product status as the other platforms.

R-1 remains incomplete until a configured binary archive runs on a clean target machine without a Rust toolchain. GitHub Releases own detailed artifact history; [`CHANGELOG.md`](CHANGELOG.md) owns concise curated summaries.

## Version scheme: `vFINAL.MAJOR.MINOR.PATCH`

[`VERSION`](VERSION) stores the canonical four unsigned components without the `v` tag prefix. Cargo maps them to `FINAL.MAJOR.MINOR+gamepatch.PATCH`; run `python3 scripts/sync_cargo_version.py` after every bump.

| Component | Bump when | Resets | Authority |
|---|---|---|---|
| `PATCH` | maintenance or a narrow behaviour change | none | normal PR flow |
| `MINOR` | meaningful capability, substantial unfinished slice, or smaller completed criterion | `PATCH → 0` | normal PR flow |
| `MAJOR` | large completed playable step before v1; normal project phase after v1 | `MINOR`, `PATCH → 0` | normal PR flow |
| `FINAL` | the owner agrees the project is complete | `MAJOR`, `MINOR`, `PATCH → 0` | owner only |

Every ordinary pull request into `dev` advances `VERSION`; an unchanged or lower version fails CI. Choose the highest applicable component from delivered behaviour, not diff size. Closing a milestone requires at least `MINOR`. Before v1, non-playable repository work takes at most `MINOR`, and `MAJOR` normally marks meaningful playable progress.

Promotions and health checks compare cumulative endpoints. They preserve channel ordering without replaying reset or FINAL-marker rules already enforced when changes entered `dev`.

## The `FINAL` gate

Automation never bumps `FINAL`. The repository owner must add or change `docs/release/final-gate-approved.md` in the same pull request, naming the exact approved version and date:

```text
Approved version: 1.0.0.0
Date: 2026-07-25
```

`scripts/version_check.py` rejects stale, malformed, or mismatched approval. `v1.0.0.0` ships only when every [`docs/acceptance/v1.md`](docs/acceptance/v1.md) criterion is done or has a recorded waiver and the owner completes H-1 playtesting.

## Hotfixes

Security and serious player-facing fixes still enter through `dev` and traverse every rung. An owner may review stable and main promptly after evidence becomes current, but no hotfix bypasses source gates, objective checks, channel playtests, or review.
