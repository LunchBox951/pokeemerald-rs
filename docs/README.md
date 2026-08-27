# Documentation router

Read only the branch required by the task. Each target below owns the named context; follow its links only when they narrow the work further `(lean-docs)`.

| When the task involves | Read | Authority provided |
|---|---|---|
| Any change or review | [`principles.md`](principles.md) | Repository invariants and their stable handles |
| Rust implementation, source ownership, or code conventions | [`../crates/README.md`](../crates/README.md) | Workspace-wide Rust rules, crate ownership, and cross-crate seams |
| v1 scope or the definition of done | [`acceptance/v1.md`](acceptance/v1.md) | Stable acceptance criteria and status markers |
| Roadmap work, milestones, or issue placement | The relevant acceptance criterion, then [`../CONTRIBUTING.md`](../CONTRIBUTING.md) and the live GitHub milestone | Dynamic scope, sequencing, and completion work |
| Contributor setup or dependency policy | [`../README.md`](../README.md), then [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Human setup path, contribution contract, and approval requirements |
| Upstream provenance or ledger coverage | `python3 scripts/ledger.py -h`, focused subcommand help, then the read-only upstream reference | Artifact ownership, status, destination, and canonical behaviour |
| Player ROM import or the asset-pack format | [`../README.md#playing`](../README.md#playing), then `crates/rom-import/src/lib.rs` and `crates/pack-format/src/lib.rs` `//!` docs | Supported revision, hash-keyed import contract, and the pack layout both backends write |
| Deterministic frame captures | [`snapshots.md`](snapshots.md) | Snapshot format, generation, and comparison contract |
| Scripted headless application runs | [`scenarios.md`](scenarios.md) | Scenario definition and execution contract |
| Versions, releases, or channel promotion | [`../RELEASE.md`](../RELEASE.md), [`../CHANGELOG.md`](../CHANGELOG.md), and live GitHub state | Release policy, curated history, and current evidence |
| Bugs, questions, security, or conduct | [`../SUPPORT.md`](../SUPPORT.md), [`../SECURITY.md`](../SECURITY.md), and [`../CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) | Public reporting channels and response expectations |

Stable documentation owns contracts, invariants, constraints, and rationale that code cannot express. Code and tests own current behaviour. GitHub issues, pull requests, and milestones own roadmap state and implementation history. The ledger owns upstream provenance and Rust or asset destinations `(constitution-vs-roadmap)`.
