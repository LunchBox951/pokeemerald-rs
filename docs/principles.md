# Principles

> These invariants govern every human and agent working in this repository. Each has a stable handle. Cite an applicable invariant as `(behavioral-fidelity)` instead of restating it. Changing an invariant requires recorded rationale in the pull request that changes this file.

## Lookup

| # | Handle | Invariant |
|---|---|---|
| 1 | `behavioral-fidelity` | Match the game's observable behaviour, not its internal structure. |
| 2 | `reference-only` | `pokeemerald/` and `mgba/` are read-only references, never edited, committed, or linked. |
| 3 | `no-verbatim` | Re-implement behaviour idiomatically; never copy upstream source verbatim. |
| 4 | `no-ffi` | No FFI, `bindgen`, or linkage to upstream C. This is a clean rewrite, not a wrapper. |
| 5 | `minimal-deps` | Prefer `std`; every external Cargo dependency needs owner approval and PR justification. |
| 6 | `oop-boundaries` | Subsystems own their state behind explicit type and trait boundaries; no global mutable state. |
| 7 | `self-explanatory-code` | Comments explain what the code cannot; clear code carries its own mechanics. |
| 8 | `lean-docs` | Preserve durable context, route task-specific detail, and give each fact one owner. |
| 9 | `constitution-vs-roadmap` | Keep goals and criteria small and durable; keep roadmap state in GitHub. |
| 10 | `gated-by-default` | Protected branches require objective green gates; stable and main also require human review. |
| 11 | `test-ratchet` | Never delete, skip, or weaken a test to pass a gate. Tests only get stronger. |

## The port

**1. Behavioural fidelity (`behavioral-fidelity`).** The game should feel identical to someone who played the original: the same dialog, trainers, encounters, music, damage outcomes, and pacing. Internals may differ wherever a better native design preserves the player-visible result. We do not chase byte-for-byte hardware or emulator parity.

**2. Reference, not source (`reference-only`).** `pokeemerald/` is the canonical game specification. `mgba/` clarifies hardware behaviour. Both are gitignored read-only checkouts pulled by `init.sh`. Never edit, commit, or link against them.

mGBA pixel math varies by build. The oracle is stock desktop mGBA using 32-bit `mColor` with 8-bit channels, not `COLOR_16_BIT`. The derivation and source citations belong beside the implementation in `crates/rendering/src/effects.rs`.

**3. No verbatim copies (`no-verbatim`).** Read upstream behaviour, then re-express it in idiomatic Rust. Translating a table of constants is acceptable; transliterating a C function line by line is not.

**4. No FFI (`no-ffi`).** No `bindgen`, linkage to upstream C, or runtime shell-out to an emulator. The product is a native rewrite, not a wrapper.

**5. Minimal dependencies (`minimal-deps`).** Default to the standard library. Every external crate added to a `Cargo.toml` requires explicit owner approval and line-by-line PR justification. Defend the total non-`std` dependency count.

**6. Object-oriented boundaries (`oop-boundaries`).** Model each subsystem as a type that owns its state and exposes methods. Use traits for polymorphism and explicit module boundaries. Avoid global mutable state. A hand-authored file over roughly 600 lines is a design smell; generated and data-shaped files require separate judgment.

**7. Self-explanatory code (`self-explanatory-code`).** Comments explain what the code cannot. Make code carry its mechanics through clear names and structure. Use comments only for necessary context that cannot be expressed in code, never to narrate mechanics, restate names, or retain implementation history.

## Working here

**8. Lean docs (`lean-docs`).** Docs preserve durable information that code and tools cannot cheaply reveal. Route task-specific context with pointers that state when and why to read the target. Give each fact one owner and link instead of duplicating it. Delete prose that does not change how a reader acts.

**9. Constitution vs. roadmap (`constitution-vs-roadmap`).** Goals, principles, and v1 acceptance criteria form a small durable core. The path to v1 lives in GitHub issues, pull requests, discussions, and milestones. Do not freeze status, implementation history, or future plans into committed documentation.

**10. Gated by default (`gated-by-default`).** Every protected branch requires current objective CI and resolved review threads. `dev` is developer integration and `unstable` is the mechanical nightly. `stable` and `main` additionally require current CODEOWNER approval and manual merge. Evidence, not assertion, advances a change.

**11. Test ratchet (`test-ratchet`).** Never delete, skip, or weaken a test to make a gate pass. Fix incorrect code, or correct an invalid test with recorded rationale. Coverage and strictness only increase.
