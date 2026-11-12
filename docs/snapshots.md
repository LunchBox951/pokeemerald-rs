# Snapshots

Use deterministic snapshots to verify rendered output without opening a window:

```bash
cargo run -p xtask --features record-snapshot -- record-snapshot --scene <name>
```

The command drives a real headless application scene and writes a complete generation under the gitignored `snapshots/` directory. The full `cargo run` form is required because scene drivers use an optional Cargo feature. `crates/xtask/src/record_snapshot.rs` owns implementation details and error cases.

## Capture format

Each generation contains:

- `<generation>/<scene>.rgb`: raw 240×160 RGB pixels in row-major order.
- `<generation>/<scene>.meta`: scene, dimensions, pixel format, scripted inputs, `rgb_hash`, `pack_hash`, and `git_sha`. `git_sha` is the commit of the checkout the recorder binary was compiled from, fixed at build time; it does not follow the checkout the binary later runs in. A dirty build suffixes the SHA with `-dirty`, and `unknown` means the build could not establish a commit (no git, or a source tree that is not a repository root). A tracked file hidden from `git status` by an assume-unchanged or skip-worktree index flag also counts as dirty, because its edits cannot be seen.
- `<scene>.generation`: the atomic pointer to the complete visible generation.

The recorder stages both payload files before replacing the generation pointer. A failure before that replacement therefore leaves the previous complete generation visible, or leaves no generation on an initial run. Available scenes are `title`, `main-menu-new-game`, and `main-menu-option`. The title capture uses the documented visible “Press Start” frame.

A failed capture retains whatever it had staged or promoted and reports its last known path; inspect retained directories before removing them by hand. `crates/xtask/src/record_snapshot.rs` and its `staging` module own the retention, identity re-verification, and race cases, including the Unix case where a failed run has already replaced the pointer.

## Verification contract

Snapshots are agent-verification artifacts. Use them to detect and inspect visual changes during implementation and review. A hash mismatch proves bytes changed; a matching non-cryptographic hash does not prove equality. Claim identical output only after exact byte comparison.

Clean captures are deterministic for the same source commit, retained asset pack, scene, and inputs. Dirty captures do not identify the uncommitted source that produced them and therefore cannot establish reproducibility. Cargo recomputes the build identity on every invocation, so a rebuilt recorder never keeps a label from an earlier tree; a binary kept from an earlier build keeps the label it was built with.

Keep generated RGB and metadata local. Record relevant hashes or comparison results in pull-request test evidence when a change affects a captured scene. Human side-by-side playtesting against the real game is a separate channel-validation contract in [`../RELEASE.md`](../RELEASE.md).
