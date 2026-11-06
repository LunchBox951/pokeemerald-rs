//! Pins which pack `cargo xtask record-snapshot` and `cargo xtask scenario`
//! read: the checkout pack (`AssetPack::repo_pack_path()`), never whatever
//! `$POKEEMERALD_PACK` (the player resolver's override) names.
//!
//! Lives in an integration test that spawns the `xtask` binary for the same
//! reason `scenario_pack_pin.rs` does: the override is process environment,
//! and setting it in a test binary would race every concurrent `getenv`
//! (`crates/README.md`'s `(oop-boundaries)`). A child owns its environment.
//!
//! The unit tests in `main.rs` (`*_dispatch_fails_closed_without_a_pack`)
//! guard on `repo_pack_path()` for the same reason; these tests are the
//! out-of-process proof that the guard probes the right path:
//!
//! - checkout pack present (ignored, needs `cargo xtask extract`): with the
//!   override naming a missing file, both commands still succeed;
//! - checkout pack absent (runs everywhere, skips read-only where the pack
//!   exists): with the override naming a file that *does* exist, both
//!   commands still fail, and the error names the checkout pack path, not
//!   the override.

#![cfg(feature = "scenes")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A path no pack can be at.
const MISSING_PACK: &str = "/nonexistent/pokeemerald-dispatch-boundary-test.pack";

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/xtask is always two levels under the repo root")
}

fn checkout_pack() -> PathBuf {
    pack_format::repo_pack_path()
}

fn run_xtask(args: &[&str], pack_override: impl AsRef<std::ffi::OsStr>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .env(pack_format::PACK_PATH_ENV, pack_override)
        .output()
        .expect("the xtask binary this test is built against must be spawnable")
}

fn assert_fails_naming_the_checkout_pack(output: &Output, override_path: &str, what: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "{what} must fail with no checkout pack, even though $POKEEMERALD_PACK names an \
         existing file; exit status {status}\n--- stdout ---\n{stdout}--- stderr ---\n{stderr}",
        status = output.status,
    );
    // Which path the command opened is the boundary under test: an error
    // naming the override would mean the command followed it (an invalid
    // file fails too, so "mentions the pack" alone cannot tell the two apart).
    let checkout = checkout_pack();
    assert!(
        stderr.contains(&*checkout.to_string_lossy()),
        "{what}: the failure must name the checkout pack {}, got:\n{stderr}",
        checkout.display()
    );
    assert!(
        !stderr.contains(override_path),
        "{what}: the failure must not name the override {override_path}, got:\n{stderr}"
    );
}

/// The checkout-absent half: an override pointing at a file that exists (the
/// xtask binary itself) must not rescue a missing checkout pack.
fn dispatch_fails_without_checkout_pack(args: &[&str], what: &str) {
    if checkout_pack().exists() {
        return;
    }
    let existing_override = env!("CARGO_BIN_EXE_xtask");
    assert!(Path::new(existing_override).exists());
    assert_fails_naming_the_checkout_pack(
        &run_xtask(args, existing_override),
        existing_override,
        what,
    );
}

#[test]
fn record_snapshot_ignores_an_existing_override_without_the_checkout_pack() {
    dispatch_fails_without_checkout_pack(
        &["record-snapshot", "--scene", "title"],
        "record-snapshot",
    );
}

#[test]
#[cfg(feature = "scenario")]
fn scenario_ignores_an_existing_override_without_the_checkout_pack() {
    dispatch_fails_without_checkout_pack(&["scenario", "--name", "boot-to-main-menu"], "scenario");
}

/// Restores `snapshots/` after a child `record-snapshot` publishes into it:
/// removes only generation directories the child created and puts the
/// `title.generation` pointer back as it was, so the real-pack test leaves
/// the checkout as it found it (the command's output directory is fixed at
/// `<repo root>/snapshots`; there is no scratch-directory input).
struct SnapshotsRestore {
    dir: PathBuf,
    dir_existed: bool,
    pointer: PathBuf,
    pointer_before: Option<Vec<u8>>,
    entries_before: std::collections::BTreeSet<std::ffi::OsString>,
}

impl SnapshotsRestore {
    fn capture() -> Self {
        let dir = repo_root().join("snapshots");
        let pointer = dir.join("title.generation");
        let entries_before = list_entries(&dir);
        Self {
            dir_existed: dir.exists(),
            pointer_before: std::fs::read(&pointer).ok(),
            entries_before,
            dir,
            pointer,
        }
    }
}

fn list_entries(dir: &Path) -> std::collections::BTreeSet<std::ffi::OsString> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name())
        .collect()
}

impl Drop for SnapshotsRestore {
    fn drop(&mut self) {
        for name in list_entries(&self.dir).difference(&self.entries_before) {
            // Only the generation directories this child published; the
            // pointer and staging names are handled or already gone.
            if name.to_string_lossy().starts_with("title.generation-") {
                let _ = std::fs::remove_dir_all(self.dir.join(name));
            }
        }
        match &self.pointer_before {
            Some(bytes) => {
                let _ = std::fs::write(&self.pointer, bytes);
            }
            None => {
                let _ = std::fs::remove_file(&self.pointer);
            }
        }
        if !self.dir_existed {
            let _ = std::fs::remove_dir(&self.dir);
        }
    }
}

fn assert_succeeds(output: &Output, what: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{what} reads the checkout pack and must succeed regardless of \
         $POKEEMERALD_PACK={MISSING_PACK}; exit status {status}\n--- stdout ---\n{stdout}\
         --- stderr ---\n{stderr}",
        status = output.status,
    );
}

#[test]
#[ignore = "needs a local pack produced by `cargo xtask extract`"]
fn record_snapshot_reads_the_checkout_pack_even_with_a_missing_override() {
    assert!(
        checkout_pack().exists(),
        "run `cargo xtask extract` first: {}",
        checkout_pack().display()
    );
    let _restore = SnapshotsRestore::capture();
    let output = run_xtask(&["record-snapshot", "--scene", "title"], MISSING_PACK);
    assert_succeeds(&output, "record-snapshot");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("recorded `title` snapshot"),
        "expected the dispatch success line, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
#[ignore = "needs a local pack produced by `cargo xtask extract`"]
#[cfg(feature = "scenario")]
fn scenario_reads_the_checkout_pack_even_with_a_missing_override() {
    assert!(
        checkout_pack().exists(),
        "run `cargo xtask extract` first: {}",
        checkout_pack().display()
    );
    let output = run_xtask(&["scenario", "--name", "boot-to-main-menu"], MISSING_PACK);
    assert_succeeds(&output, "scenario");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("scenario `boot-to-main-menu` passed"),
        "expected the dispatch success line, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
