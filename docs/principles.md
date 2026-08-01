# Principles

> The invariants for **anyone working in this repository** — human or agent.
> Each has a stable **handle** (the `code-font` word). Cite it inline anywhere —
> code comments, PR bodies, issues — as `(behavioral-fidelity)`, so a rule can be
> referenced without restating it. These docs are the *why*; the code is the *how*.
>
> Changing an invariant is a deliberate act: it needs a recorded rationale in the
> PR that amends this file, never a silent exception.

## Lookup

| # | Handle | Invariant |
|---|--------|-----------|
| 1 | `behavioral-fidelity` | Match the game's observable behaviour, not its internal structure. |
| 2 | `reference-only` | `pokeemerald/` and `mgba/` are read-only references — never edited, committed, or linked. |
| 3 | `no-verbatim` | Re-implement behaviour idiomatically; never copy upstream source verbatim. |
| 4 | `no-ffi` | No FFI, `bindgen`, or linkage to the upstream C. A clean rewrite, not a wrapper. |
| 5 | `minimal-deps` | Prefer `std`; every new Cargo dependency needs owner approval, justified in the PR. |
| 6 | `oop-boundaries` | Subsystems are owned types with methods and traits; explicit boundaries; no global mutable state. |
| 7 | `lean-docs` | Docs state principles and rules, not step-by-step prose. One concept per file; link, don't duplicate. |
| 8 | `constitution-vs-roadmap` | The goals, principles, and acceptance criteria are fixed and small; the roadmap is dynamic and lives in GitHub issues/PRs/discussions — not in committed plan or spec docs. |
| 9 | `gated-by-default` | Nothing reaches a protected branch without green objective CI; stable and main additionally require human review. Strictness tightens toward `main`. |
| 10 | `test-ratchet` | Never delete, skip, or weaken a test to pass a gate. Tests only get stronger. |

## The port

**1. Behavioural fidelity (`behavioral-fidelity`).** The game should feel
identical to someone who played the original: same dialog, trainers, encounter
tables, music, damage outcomes, pacing. Internals may differ wildly where a
better solution exists. We do **not** chase byte-for-byte parity with hardware or
mGBA — we chase the player-visible result.

**2. Reference, not source (`reference-only`).** `pokeemerald/` is the canonical
specification of the game (data, scripts, text, formulas); `mgba/` clarifies how
hardware would have behaved. Both are gitignored, read-only, and pulled by
`init.sh`. Never edit them, never commit them, never link against them.

**3. No verbatim copies (`no-verbatim`).** Read the upstream behaviour, then
re-express it in idiomatic Rust. Translating a table of constants is fine;
transliterating a C function line-for-line is not.

**4. No FFI (`no-ffi`).** The goal is a native rewrite, not a wrapper. No
`bindgen`, no linking to upstream C, no shelling out to an emulator at runtime.

**5. Minimal dependencies (`minimal-deps`).** Default to the standard library.
Every entry added to a `Cargo.toml` requires explicit owner approval and a
line-by-line justification in the PR description. The total non-`std` dep count is
a number we defend, not one we let drift.

**6. Object-oriented boundaries (`oop-boundaries`).** Model each subsystem as a
type that owns its state and exposes methods; use traits for polymorphism; keep
module boundaries explicit. Avoid global mutable state. A file over ~600 lines is
a smell — it is usually doing too much.

## Working here

**7. Lean docs (`lean-docs`).** A doc explains a principle or a rule and then gets
out of the way. One concept per file; keep files short enough to read in full;
link to the file that owns a topic instead of restating it.

**8. Constitution vs. roadmap (`constitution-vs-roadmap`).** A small, durable core
— the goals, these principles, and the v1 acceptance criteria — is fixed and
changes rarely. The *roadmap* — how we actually get to v1 — is dynamic and lives
in GitHub issues, PRs, and discussions, not in committed plan or spec docs. Don't
freeze the path into documents; let it adapt. Look for "what to do next" in
GitHub, not in a static plan.

**9. Gated by default (`gated-by-default`).** Nothing reaches a protected branch
without green objective CI. `dev` is developer-facing and `unstable` is the
mechanical nightly, so neither requires an approval; both still require a pull
request, current checks, and resolved review threads. `stable` and `main` add a
current CODEOWNER approval and manual merge. The bar tightens toward production,
and every merge rests on evidence rather than assertion.

**10. Test ratchet (`test-ratchet`).** It is never acceptable to delete, skip, or
weaken a test to make a gate pass. If a test is wrong, fix the test with a
recorded reason; otherwise fix the code. Coverage and strictness only go up.
