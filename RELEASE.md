# Release policy & checklist

The canonical release policy for `pokeemerald-rs`: the branch ladder, how work is
promoted, the per-rung gates, the version scheme, and hotfixes. CI enforces the
objective parts; a human owner gates the player-facing rungs `(gated-by-default)`.

## The branch ladder

Four channel branches, strictness tightening toward production:

| Branch     | Channel   | Audience                                   |
|------------|-----------|--------------------------------------------|
| `dev`      | developer | integration — reviewed work lands here     |
| `unstable` | nightly   | freshest playable build                    |
| `stable`   | beta      | broadly-validated build                    |
| `main`     | stable    | the release players get                    |

Normal contributions target `dev`. `unstable`, `stable`, and `main` are gated and
accept **only** promotion PRs whose source is a `release/*` branch — never a direct
push.

## Promotion: the `release/*` model

Promotion rides a long-lived **`release/X`** branch (named for its target version,
e.g. `release/0.1`) rather than fast-forwarding the channel branches directly:

1. When a release cycle opens, `release/X` is cut from `dev`.
2. CI opens a promotion PR `release/X → unstable`. Stabilization fixes land on
   `release/X` (directly, or via PRs into it) and show up on that open PR.
3. When a promotion PR merges into a rung, CI
   (`.github/workflows/promote.yml`) **auto-opens the next-rung PR** from the same
   `release/X`, walking `unstable → stable → main`.
4. `release/X` persists until it lands on `main`, then remains as that version's
   maintenance line for patches. The next version cuts a fresh `release/Y`.

This keeps every in-flight release on one trackable branch, lets fixes ride up the
ladder as a unit, and offloads the mechanical PR-opening to CI. The cut from `dev`
and every player-facing merge remain deliberate owner decisions.

> **No rung-skipping — by design.** A release always climbs every rung in order
> (`unstable → stable → main`); nothing jumps a rung. Simple patches clear the
> gates fast and bubble up quickly, while complex changes take longer — the ladder
> *is* the throttle, sorting work by how much validation it still needs.

## Consolidating release branches

The maintenance routine may, on its own, **merge two related `release/*` branches**
into one — two patch lines for the same version, or a fix that belongs with an
in-flight release — to keep the promotion graph tidy and let related work ride up
together. Two rules keep that autonomy from becoming a bypass:

- **A merge never grants a rung.** The consolidated branch sits at the **lower** of
  its inputs' positions: if `release/A` had reached `stable` and `release/B` only
  `unstable`, the merged branch re-enters at `unstable` and must re-clear `stable`'s
  gates before `main`. Content is only ever as promoted as its least-promoted part —
  consolidation re-runs the gates for anything that hasn't passed them.
- **No path skips a rung.** There is no route from `dev` (or a fresh `release/*`) to
  `main` that skips `unstable` or `stable`. Every change rides `unstable → stable →
  main` in order: `promote.yml` only ever opens the *next* rung, branch protection
  accepts a channel's promotion PR only from a `release/*` that has cleared the rung
  below, and the player-channel gates re-run on the combined content. Consolidating,
  cutting, or culling release branches at the `dev`/`unstable` tier is the routine's
  call; merges into `stable` and `main` stay owner-gated (`needs-operator`)
  regardless of how the branch was assembled.

## Per-rung gates

### `dev` — integration

Reviewed, CI-green, ledger-verified work merges here. Dependency additions are
owner-approved `(minimal-deps)`. Release impact is recorded on each PR.

**Ledger — the L-1 accounting rule.** The ledger may carry *sub-file artifacts*:
a single data table (e.g. `gTypeEffectiveness`) carved out of a large
multi-concern source file (`src/battle_main.c#gTypeEffectiveness`), each with its
own status and `rust_target`. This keeps coverage honest for partially-ported
files. A file's own `status` covers everything **not** broken out into a named
artifact; a file counts as accounted for (does **not** count toward `pending`,
does not block **L-1**) **only when its own status is terminal AND every one of
its sub-artifacts is terminal** — otherwise it counts as `pending`. So one ported
table can never over-claim the whole file, and extracting a table under-claims
nothing once the file's own status covers the remainder. Files with no
sub-artifacts count exactly as their own status. `ledger.py verify` checks
sub-artifact `rust_target` pointers alongside file-level ones.

### `release/X → unstable` — nightly (may auto-advance)

Promote when **all** hold; otherwise skip rather than ship a broken nightly:

- candidate passes normal CI and `ledger.py verify`;
- `cargo run -p xtask --features smoke -- e2e --suite smoke` passes (the
  `smoke` feature keeps the platform stack out of default xtask builds, so the
  plain `cargo xtask` alias cannot run this suite);
- channel artifacts build;
- no known launch-blocking, crash-on-start, or save-corrupting issue.

### `unstable → stable` — beta (prepare, then owner-gated)

- ≥ 7 calendar days on `unstable` (owner may expedite);
- no unresolved blocker-class issues (`release-blocker`, crash, save-loss, severe
  performance regression);
- `cargo xtask e2e --suite full --release` passes (V-2);
- visual snapshot changes reviewed, not hash-blessed `(test-ratchet)`;
- performance budgets pass in release mode;
- active waivers listed and owner-accepted.

### `stable → main` — release (prepare, then owner-gated)

A candidate already on `stable` may be released to `main` only when **all** of
these hold:

- [ ] **Burn-in:** ≥ 7 further calendar days on `stable`, unless the owner approves
      an expedited release.
- [ ] **CI green:** normal CI passes; `cargo build/test/clippy/fmt --workspace`
      pass once the workspace exists.
- [ ] **Full E2E:** `cargo xtask e2e --suite full --release` passes (V-2).
- [ ] **Soak E2E:** `cargo xtask e2e --suite soak --release` passes **before the
      tag** (V-3).
- [ ] **Clean-machine artifact run:** the release tarball runs on a machine with
      **no Rust toolchain** installed (R-1).
- [ ] **Operator playtest signoff (H-1):** the owner has played the candidate
      end-to-end and signed off on feel, audio, visuals, stability, performance.
- [ ] **Docs current:** `CHANGELOG.md` and `VERSION` are updated (R-2); this
      `RELEASE.md` is complete.
- [ ] **Waivers disclosed:** every active E2E waiver is listed in the release notes
      with its accepted reason and follow-up issue (H-4).
- [ ] **No open blockers:** no unresolved `release-blocker` / crash / save-loss /
      severe-performance-regression issue linked to the candidate.

A burn-in window is not a substitute for validation: the standard is "no known
unresolved blocker-class regressions after exposure," not "nobody complained."

## Promotion PR shape

Mechanical and reviewable: source→target in the title, candidate SHA, linked
CI/E2E/artifact/playtest evidence, known issues + active waivers, declared release
impact. Never fold feature work into a promotion. If a promotion needs a fix, fix
the **source** (`release/X` or the rung below) first, let its gates pass, and let
CI reopen the promotion.

## Repository controls go-live

The workflow files establish the review and scan policy; repository settings make
it enforceable. Apply and verify these settings as an owner-level operation before
claiming R-4 is complete:

1. On `dev`, `unstable`, `stable`, and `main`, require pull requests, one approval,
   CODEOWNERS review, dismissal of stale approvals, and branches up to date before
   merge. Include administrators; disallow direct pushes, force pushes, and branch
   deletion.
2. After `policy`, all three `native` matrix legs, and `merge-gate` have reported
   successfully, atomically replace the obsolete per-command CI contexts on all
   four channel branches with the single `merge-gate` context. Do not remove the
   old contexts in a separate operation that could leave branch protection
   deadlocked. `merge-gate` covers the exact default format, Clippy, release-build,
   and test commands; Linux/macOS/Windows smoke; and Ubuntu real-pack validation.
   Also require `dependency-review`, `codeql (actions)`, `codeql (python)`, and
   `codeql (rust)` after each has reported successfully on the protected branch,
   plus the rule that blocks merges on critical CodeQL alerts.
3. On `unstable`, `stable`, and `main`, also require `require-release-source` and
   `require-rung-cleared`. Follow the bootstrap order documented at the head of
   `channel-merge-policy.yml` before registering them, or the first promotion will
   deadlock.
4. Keep merge commits enabled and disable squash and rebase merging for promotions
   into `unstable`, `stable`, and `main`; their ancestry checks depend on preserved
   `release/*` history. Enable Actions to create pull requests for `promote.yml`.
   For every promotion PR it opens with `GITHUB_TOKEN`, a maintainer with write
   access must select **Approve workflows to run** before required checks start.
5. Set the repository's default **Workflow permissions** to read-only. Workflows
   that mutate repository state grant only their required write scopes explicitly.
6. After `.github/workflows/codeql.yml` is on `dev`, disable GitHub's CodeQL
   **default setup** in the repository security settings, set the Actions
   repository variable `CODEQL_ADVANCED_UPLOADS_ENABLED` to `true`, re-enable the
   `codeql` workflow if default setup disabled it, then manually dispatch `codeql`
   and confirm its results upload. Until the variable is enabled, the advanced
   jobs analyze without uploading because default setup rejects repository-owned
   advanced-workflow results. The advanced workflow scans Rust, Python, and Actions
   weekly and on relevant pushes and pull requests.
7. Keep secret scanning and push protection enabled. They are already enabled for
   this repository; their alerts are triaged alongside CodeQL and Dependabot.

Unsafe-code alerts are deliberately non-blocking. Each finding must be reviewed
as accepted (with its safety rationale), a refactor candidate, or a linked
follow-up issue. An alert is an inventory entry, not proof that the code is
incorrect or that `unsafe` is inherently slower than safe Rust.

## Version scheme — `vFINAL.MAJOR.MINOR.PATCH`

The canonical version lives in [`VERSION`](VERSION) **without** the `v` prefix;
tags and Releases add it. Versions are compared lexicographically as four unsigned
ints, and CI (`scripts/version_check.py`) rejects regressions and bad resets.

| Component | Bump when | Resets | Authority |
|-----------|-----------|--------|-----------|
| `PATCH`   | fixes, docs, CI, ledger, packaging — normal flow | — | normal PR flow |
| `MINOR`   | a completed milestone / user-visible capability | `PATCH → 0` | maintainer / owner |
| `MAJOR`   | a large project phase or breaking repository contract | `MINOR`, `PATCH → 0` | maintainer |
| `FINAL`   | the project is agreed complete (`0` → `1`) | `MAJOR`, `MINOR`, `PATCH → 0` | **maintainer only — never automated** |

A `MAJOR` or `MINOR` bump **must** reset the lower components to `0`, or CI rejects
it. While `FINAL = 0`, every release is a **prerelease**; nightly/beta channel
artifacts use channel + date + short-SHA names and do **not** require a `VERSION`
bump per promotion.

## The `FINAL` gate

`FINAL` is the project-completion epoch, not an ordinary version component. It is
**maintainer-only and is never bumped by automation or drive-by contributors**. A
`FINAL` bump requires an explicit, auditable approval marker committed to the repo:

- **`docs/release/final-gate-approved.md`** — present only for an approved `FINAL`
  bump, naming the approved version and the date, e.g.:

  ```
  Approved version: 1.0.0.0
  Date: 2026-07-25
  ```

`scripts/version_check.py` fails any change to `FINAL` unless, in the *same*
proposed change, this marker is added or edited (a marker byte-identical to the
base revision's does not count — a stale marker approves nothing), parses
successfully, and its `Approved version` names the exact proposed `VERSION`.
This keeps the decision in git history — tied to one specific transition —
rather than a boolean file-exists check or a buried workflow click. v1 ships
(and `FINAL` becomes `1`) only when every row in
[`docs/acceptance/v1.md`](docs/acceptance/v1.md) is done or has a recorded waiver
and the Operator's playtest (H-1) is signed.

## Hotfixes

Serious player-facing defects bypass the ladder: branch from `main`, fix only the
minimal issue, validate to the risk, merge to `main`, then back-merge into
`stable`, `unstable`, `dev`, and the active `release/*`. Don't pull unrelated work
into `main`. Record any protected-gate bypass in the release notes or a follow-up
issue.
