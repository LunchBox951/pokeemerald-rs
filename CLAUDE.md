# pokeemerald-rs

@AGENTS.md

The imported `AGENTS.md` above is the full project memory, loaded the same as
anything written directly in this file. What follows is Claude-only and
additive — nothing here restates a default the system prompt already covers.

## Claude-specific notes

- Don't stack extra verification on top of what `cargo test`/`clippy`/`fmt`
  and CI already gate — re-checking work those commands already checked
  spends tokens without catching more.
- Within `AGENTS.md`'s autonomy boundaries, make the routine call yourself;
  pause only when two honest readings of an acceptance ID's scope would
  produce materially different work. This never overrides the **Confirm
  first** list above — a new dependency or a workflow/release-file edit
  still waits for confirmation, however unambiguous the call.
- Reserve Explore or subagent delegation for genuinely broad sweeps — a
  multi-crate consistency check, a `ledger.py gaps` survey across many
  entries — not a scoped, single-file change.
- Running long or unattended here (`/loop`, a background agent)? Ground
  progress claims in actual command output (`cargo test`, `ledger.py
  verify`) — this repo's own maintenance loop already works that way (see
  README's "How this project is built").
