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

/// Restores `snapshots/` after a child `record-snapshot` publishes into it,
/// so the real-pack test leaves the checkout as it found it (the command's
/// output directory is fixed at `<repo root>/snapshots`; there is no
/// scratch-directory input).
///
/// It touches only what its own child created: the generation directory the
/// child reported ([`Self::own_generation_from_stdout`]), and the
/// `title.generation` pointer only while that pointer still names such a
/// generation. A generation or pointer from a concurrent publisher is left
/// as that publisher wrote it.
struct SnapshotsRestore {
    dir: PathBuf,
    dir_existed: bool,
    pointer: PathBuf,
    pointer_before: Option<Vec<u8>>,
    own_generations: Vec<String>,
}

impl SnapshotsRestore {
    fn capture() -> Self {
        Self::capture_in(repo_root().join("snapshots"))
    }

    /// # Panics
    ///
    /// When the pointer exists but cannot be read: recording that as absent
    /// would delete a file this guard never understood.
    fn capture_in(dir: PathBuf) -> Self {
        let pointer = dir.join("title.generation");
        let pointer_before = match std::fs::read(&pointer) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!(
                "cannot read the existing snapshot pointer {}: {error}",
                pointer.display()
            ),
        };
        Self {
            dir_existed: dir.exists(),
            pointer_before,
            own_generations: Vec::new(),
            dir,
            pointer,
        }
    }

    /// Records the generation directory named by `record-snapshot`'s
    /// success line (`... to <dir>/title.rgb (+ ...`), when it sits directly
    /// under this guard's directory. The line carries a native
    /// `Path::display()` path, so `/` and `\` both separate components
    /// (Windows prints `\`); the comparison normalises both to `/`.
    fn own_generation_from_stdout(&mut self, stdout: &str) {
        let Some((_, rest)) = stdout.split_once(" snapshot (") else {
            return;
        };
        let Some((_, rest)) = rest.split_once(" to ") else {
            return;
        };
        let Some((rgb_path, _)) = rest.split_once(" (+ ") else {
            return;
        };
        let rgb_path = rgb_path.replace('\\', "/");
        let Some((rgb_dir, rgb_name)) = rgb_path.rsplit_once('/') else {
            return;
        };
        if rgb_name != "title.rgb" {
            return;
        }
        let Some((parent, name)) = rgb_dir.rsplit_once('/') else {
            return;
        };
        let own_dir = self.dir.to_string_lossy().replace('\\', "/");
        if parent != own_dir {
            return;
        }
        if name.starts_with("title.generation-") {
            self.own_generations.push(name.to_owned());
        }
    }
}

impl Drop for SnapshotsRestore {
    fn drop(&mut self) {
        if self.own_generations.is_empty() {
            return;
        }
        // Restore the pointer only while it still names a generation this
        // guard's child created; another publisher's pointer stays.
        let pointer_is_ours = std::fs::read_to_string(&self.pointer)
            .is_ok_and(|text| self.own_generations.iter().any(|own| own == text.trim()));
        if pointer_is_ours {
            match &self.pointer_before {
                Some(bytes) => {
                    let _ = std::fs::write(&self.pointer, bytes);
                }
                None => {
                    let _ = std::fs::remove_file(&self.pointer);
                }
            }
        }
        for name in &self.own_generations {
            let _ = std::fs::remove_dir_all(self.dir.join(name));
        }
        if !self.dir_existed {
            // Fails, harmlessly, when anything else is in it.
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
    let mut restore = SnapshotsRestore::capture();
    let output = run_xtask(&["record-snapshot", "--scene", "title"], MISSING_PACK);
    restore.own_generation_from_stdout(&String::from_utf8_lossy(&output.stdout));
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

/// A scratch `snapshots/` stand-in owned by one regression test.
fn scratch_snapshots(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "dispatch-pack-boundary-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

#[test]
#[cfg(unix)]
fn adjudicator_restore_keeps_a_pointer_it_could_not_read() {
    let dir = scratch_snapshots("unreadable-pointer");
    let pointer = dir.join("title.generation");
    // A self-referential symlink: it exists, but reading it fails with
    // ELOOP, which is not "absent".
    std::os::unix::fs::symlink(&pointer, &pointer).expect("symlink");
    let captured = std::panic::catch_unwind(|| SnapshotsRestore::capture_in(dir.clone()));
    assert!(
        captured.is_err(),
        "capture must abort on an unreadable pointer rather than record it as absent"
    );
    drop(captured);
    assert!(
        std::fs::symlink_metadata(&pointer).is_ok(),
        "an unreadable pointer must survive"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn snapshots_restore_keeps_a_concurrent_publishers_generation() {
    let dir = scratch_snapshots("concurrent-publisher");
    let restore = SnapshotsRestore::capture_in(dir.clone());
    // Another publisher lands a generation and repoints the pointer while
    // the guard is held; this guard's own child published nothing.
    let other = dir.join("title.generation-4294967295-0");
    std::fs::create_dir(&other).expect("other generation");
    std::fs::write(other.join("title.rgb"), b"rgb").expect("payload");
    std::fs::write(
        dir.join("title.generation"),
        b"title.generation-4294967295-0\n",
    )
    .expect("pointer");
    drop(restore);
    assert!(
        other.join("title.rgb").exists(),
        "a generation this guard's child did not create must survive"
    );
    assert_eq!(
        std::fs::read(dir.join("title.generation")).expect("pointer"),
        b"title.generation-4294967295-0\n",
        "a pointer written by another publisher must be left alone"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn snapshots_restore_parses_a_windows_backslash_success_line() {
    let dir = scratch_snapshots("windows-separators");
    std::fs::create_dir(dir.join("title.generation-1-0")).expect("old generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-1-0\n").expect("pointer");
    let mut restore = SnapshotsRestore::capture_in(dir.clone());
    let own = dir.join("title.generation-78-0");
    std::fs::create_dir(&own).expect("own generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-78-0\n").expect("pointer");
    // `Path::display()` on Windows: every separator is a backslash.
    let native = own.display().to_string().replace('/', "\\");
    restore.own_generation_from_stdout(&format!(
        "recorded `title` snapshot (1 byte(s)) to {native}\\title.rgb (+ {native}\\title.meta); rgb hash 0\n",
    ));
    drop(restore);
    assert!(!own.exists(), "the child's own generation is removed");
    assert_eq!(
        std::fs::read(dir.join("title.generation")).expect("pointer"),
        b"title.generation-1-0\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn snapshots_restore_removes_only_its_own_generation_and_restores_the_pointer() {
    let dir = scratch_snapshots("own-generation");
    std::fs::create_dir(dir.join("title.generation-1-0")).expect("old generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-1-0\n").expect("pointer");
    let mut restore = SnapshotsRestore::capture_in(dir.clone());
    // The child publishes its generation and repoints the pointer.
    let own = dir.join("title.generation-77-0");
    std::fs::create_dir(&own).expect("own generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-77-0\n").expect("pointer");
    restore.own_generation_from_stdout(&format!(
        "recorded `title` snapshot (1 byte(s)) to {}/title.rgb (+ {}/title.meta); rgb hash 0\n",
        own.display(),
        own.display()
    ));
    drop(restore);
    assert!(!own.exists(), "the child's own generation is removed");
    assert!(dir.join("title.generation-1-0").exists());
    assert_eq!(
        std::fs::read(dir.join("title.generation")).expect("pointer"),
        b"title.generation-1-0\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
