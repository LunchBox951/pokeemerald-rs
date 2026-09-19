# Snapshots

Use deterministic snapshots to verify rendered output without opening a window:

```bash
cargo run -p xtask --features record-snapshot -- record-snapshot --scene <name>
```

The command drives a real headless application scene and writes a complete generation under the gitignored `snapshots/` directory. The full `cargo run` form is required because scene drivers use an optional Cargo feature. `crates/xtask/src/record_snapshot.rs` owns implementation details and error cases.

## Capture format

Each generation contains:

- `<generation>/<scene>.rgb`: raw 240×160 RGB pixels in row-major order.
- `<generation>/<scene>.meta`: scene, dimensions, pixel format, scripted inputs, `rgb_hash`, `pack_hash`, and `git_sha`; dirty captures suffix the SHA with `-dirty`.
- `<scene>.generation`: the atomic pointer to the complete visible generation.

The recorder stages both payload files before replacing the generation pointer. A failure therefore leaves the previous complete generation visible or leaves no generation on an initial run. Available scenes are `title`, `main-menu-new-game`, and `main-menu-option`. The title capture uses the documented visible “Press Start” frame.

## Verification contract

Snapshots are agent-verification artifacts. Use them to detect and inspect visual changes during implementation and review. A hash mismatch proves bytes changed; a matching non-cryptographic hash does not prove equality. Claim identical output only after exact byte comparison.

Clean captures are deterministic for the same source commit, retained asset pack, scene, and inputs. Dirty captures do not identify the uncommitted source that produced them and therefore cannot establish reproducibility.

Keep generated RGB and metadata local. Record relevant hashes or comparison results in pull-request test evidence when a change affects a captured scene. Human side-by-side playtesting against the real game is a separate channel-validation contract in [`../RELEASE.md`](../RELEASE.md).
