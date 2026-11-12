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

/// A scratch directory under the build's temp storage, removed on drop.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "dispatch-pack-boundary-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What a checkout's `snapshots/` holds: its generation directories and the
/// bytes of the `title.generation` pointer.
fn snapshot_state(dir: &Path) -> (Vec<String>, Option<Vec<u8>>) {
    let mut generations: Vec<String> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|entry| entry.expect("snapshots entry"))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.starts_with("title.generation-"))
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("cannot list {}: {error}", dir.display()),
    };
    generations.sort();
    let pointer = match std::fs::read(dir.join("title.generation")) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("cannot read the snapshot pointer: {error}"),
    };
    (generations, pointer)
}

/// `record-snapshot` publishes a new generation without removing the one it
/// supersedes, so a developer's `snapshots/` normally holds several and a
/// test cannot put it back as it found it. The child therefore records into
/// a scratch directory (`$POKEEMERALD_SNAPSHOT_DIR`), and the developer's
/// directory must come out exactly as it went in, whatever it holds.
#[test]
#[ignore = "needs a local pack produced by `cargo xtask extract`"]
fn record_snapshot_reads_the_checkout_pack_even_with_a_missing_override() {
    assert!(
        checkout_pack().exists(),
        "run `cargo xtask extract` first: {}",
        checkout_pack().display()
    );
    let checkout_snapshots = repo_root().join("snapshots");
    let before = snapshot_state(&checkout_snapshots);
    let scratch = ScratchDir::new("real-pack-record-snapshot");
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["record-snapshot", "--scene", "title"])
        .env(pack_format::PACK_PATH_ENV, MISSING_PACK)
        .env("POKEEMERALD_SNAPSHOT_DIR", &scratch.0)
        .output()
        .expect("the xtask binary this test is built against must be spawnable");
    assert_succeeds(&output, "record-snapshot");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("recorded `title` snapshot"),
        "expected the dispatch success line, got:\n{stdout}"
    );
    let (generations, pointer) = snapshot_state(&scratch.0);
    assert_eq!(
        generations.len(),
        1,
        "the child publishes one generation into the scratch directory: {generations:?}"
    );
    assert_eq!(
        pointer
            .as_deref()
            .map(|bytes| String::from_utf8_lossy(bytes).trim().to_owned()),
        Some(generations[0].clone()),
        "the scratch pointer names the published generation"
    );
    assert_eq!(
        snapshot_state(&checkout_snapshots),
        before,
        "record-snapshot must leave the developer's snapshots/ as it found it"
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

/// With the checkout pack absent the command fails before it writes, so a
/// scratch output directory is left empty and the override changes nothing
/// about which pack is named.
#[test]
fn record_snapshot_with_an_output_override_still_needs_the_checkout_pack() {
    if checkout_pack().exists() {
        return;
    }
    let scratch = ScratchDir::new("override-without-pack");
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["record-snapshot", "--scene", "title"])
        .env(pack_format::PACK_PATH_ENV, MISSING_PACK)
        .env("POKEEMERALD_SNAPSHOT_DIR", &scratch.0)
        .output()
        .expect("the xtask binary this test is built against must be spawnable");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&checkout_pack().display().to_string()),
        "the error names the checkout pack:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        snapshot_state(&scratch.0),
        (Vec::new(), None),
        "nothing is published without a pack"
    );
}
