# Docs

These docs are the **why**; the code is the **how** `(lean-docs)`. Read in this
order:

1. **[principles.md](principles.md)** — the invariants this project holds. Every
   rule elsewhere cites one of these by `handle`. Start here.
2. **[acceptance/v1.md](acceptance/v1.md)** — the definition of "done" for v1, as
   stable criteria IDs. The roadmap (GitHub issues) ladders up to these.

There are no design-spec or plan docs: the roadmap is dynamic and lives in GitHub
issues, PRs, and discussions `(constitution-vs-roadmap)`. Docs hold the durable
constitution; GitHub holds the dynamic plan, and the path to each acceptance
criterion is planned there, not frozen into a file.

Agents: start at [`AGENTS.md`](../AGENTS.md) (Claude Code also reads
[`CLAUDE.md`](../CLAUDE.md), which imports it) for the operational rules —
commands, autonomy boundaries, hard rules. For a subsystem's current
implementation state, prefer its crate's `//!` doc (`crates/*/src/lib.rs`)
over any hand-written status summary.
