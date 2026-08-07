# Snapshots

```
cargo run -p xtask --features record-snapshot -- record-snapshot --scene <name>
```

That command (F-3, V-4) drives one of the real headless scenes
`pokeemerald-rs` exposes and writes a deterministic capture to `snapshots/`
(gitignored, like `assets-pack/` — see
`crates/xtask/src/record_snapshot.rs`'s module docs for the exact mechanism
and error cases). The full `cargo run` form is the working one: the scene
drivers sit behind an optional cargo feature (below), and the `cargo xtask`
alias has no way to pass `--features`. Two files per scene:

- `<scene>.rgb` — raw `240x160`, 3-byte-per-pixel (R, G, B) row-major pixels.
  No PNG encoder: that would be a new Cargo dependency `(minimal-deps)`.
- `<scene>.meta` — plain text: `scene`, `width`, `height`, `pixel_format`,
  `inputs` (the scripted button presses the scene's state implies, or
  `none`), `rgb_hash`, `pack_hash` (both `fnv1a64`, a small owned hash — no
  `sha2` dependency), and `git_sha` — suffixed `-dirty` when the capture was
  recorded with uncommitted changes in the worktree, so a hash that cannot
  be reproduced from the recorded commit says so.

Both files are a pure function of the pack's bytes, the scene, and the
current commit — no timestamp, no RNG — so two captures of the same pack on
the same commit are byte-identical. They are staged through `.tmp` siblings
and renamed into place, so a failed capture leaves neither file rather than
an orphaned `.rgb` beside a stale `.meta`. Available scene names: `title`,
`main-menu-new-game`, `main-menu-option`.

The `record-snapshot` feature is kept optional so a default `cargo build -p
xtask` stays dependency-free (`crate::record_snapshot`'s module docs); it is
an alias for the shared `scenes` feature, which is what the module itself is
gated on so CI's `cargo test -p xtask --features smoke` leg compiles and runs
its tests.

## Blessing workflow `(gated-by-default)`

V-4 requires visual snapshots "blessed via review, not hash alone" — a
`snapshot-review-needed`-labeled PR must never advance on a hash match
alone. The mechanism:

1. A developer runs `record-snapshot` against a real local pack
   (`cargo xtask extract` first) for every scene their change could have
   affected, and includes the printed `rgb_hash`/`pack_hash`/`git_sha` in
   the PR's test evidence. A `-dirty` `git_sha` is fine as evidence during
   review, but a row is only ever blessed at a clean SHA — nobody else can
   reproduce the hash otherwise.
2. An operator (a human decision, in the spirit of `docs/acceptance/v1.md`'s
   `H-1` playtest signoff — never automated) visually inspects the `.rgb`
   capture — e.g. against a real mGBA run of the same scene — and either
   signs off or requests changes.
3. On sign-off, the PR adds or updates that scene's row in the **Blessed
   snapshots** table below: the scene name, the blessed `rgb_hash`, the
   `git_sha` it was blessed at, and the operator's name — a plain record a
   later PR (or the maintenance routine) can diff against, so a
   `snapshot-review-needed` item only needs fresh human attention when the
   *current* `rgb_hash` no longer matches the last blessed one for that
   scene.

A hash match against the table means "pixel-identical to what was already
reviewed", not "reviewed" on its own — a scene with no blessed row yet has
never been reviewed, however long its `record-snapshot` output has been
byte-identical across runs.

### Blessed snapshots

| Scene | Blessed `rgb_hash` | `git_sha` | Operator | Status |
|---|---|---|---|---|
| `main-menu-new-game` | `fnv1a64:a4cfe59245374632` | `e5b3784292ec3956389aab1dea5327d2d45ff947` | — | pending operator sign-off (issue #226 proving run; not yet reviewed against mGBA) |

The `main-menu-new-game` row exercises the loop end-to-end (issue #226's
definition of done) but is deliberately **not** a blessed reference yet —
no operator has compared it against mGBA. It stays `pending` until a human
does; V-4 itself stays open until at least one row leaves that state.
