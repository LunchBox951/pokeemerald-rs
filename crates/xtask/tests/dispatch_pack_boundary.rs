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

    /// The `<dir>/title.rgb` path of `<rgb> (+ <meta>); rgb hash ...`.
    ///
    /// A checkout path may itself contain the ` (+ ` delimiter, so neither the
    /// first nor the last occurrence is reliably the separator. The two
    /// artifacts share one directory, so the separator is the occurrence
    /// whose left side ends in `title.rgb`, whose right side ends in
    /// `title.meta`, and whose directories (separators normalised) agree.
    fn rgb_path_of(line: &str) -> Option<&str> {
        const DELIMITER: &str = " (+ ";
        let (body, _) = line.rsplit_once("); rgb hash ")?;
        let norm = |path: &str| path.replace('\\', "/");
        body.match_indices(DELIMITER).find_map(|(at, _)| {
            let rgb = &body[..at];
            let meta = &body[at + DELIMITER.len()..];
            let rgb_dir = rgb.strip_suffix("title.rgb")?;
            let meta_dir = meta.strip_suffix("title.meta")?;
            (norm(rgb_dir) == norm(meta_dir)).then_some(rgb)
        })
    }

    /// Records the generation directory named by `record-snapshot`'s
    /// success line (`... to <dir>/title.rgb (+ <dir>/title.meta); rgb hash ...`), when it sits directly
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
        let Some(rgb_path) = Self::rgb_path_of(rest.lines().next().unwrap_or(rest)) else {
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

impl SnapshotsRestore {
    /// Puts the pointer back and removes this guard's own generations,
    /// returning every failure instead of swallowing it.
    ///
    /// The pointer is never checked and then rewritten in place: that leaves
    /// a gap in which a concurrent publisher's rename is overwritten. The
    /// guard instead takes whatever is at the pointer by renaming it aside,
    /// one atomic step that fixes which file is being judged, and then only
    /// ever puts a file back with `hard_link`, which refuses to replace. A
    /// pointer a publisher lands after the grab is therefore left as that
    /// publisher wrote it, and a foreign pointer the guard moved aside is put
    /// back only if nothing newer took its place.
    ///
    /// Generations are removed only after the pointer step succeeded, so a
    /// failure never leaves the pointer naming a deleted generation.
    /// `after_grab` runs between the grab and the put-back so a test can land
    /// a publisher's rename in that gap.
    fn finish_with(&mut self, after_grab: impl FnOnce()) -> Vec<String> {
        if self.own_generations.is_empty() {
            return Vec::new();
        }
        let mut errors = Vec::new();
        match self.restore_pointer(after_grab) {
            Ok(()) => {
                for name in std::mem::take(&mut self.own_generations) {
                    if let Err(error) = std::fs::remove_dir_all(self.dir.join(&name)) {
                        errors.push(format!("cannot remove generation {name}: {error}"));
                    }
                }
                if !self.dir_existed {
                    // Fails, harmlessly, when anything else is in it.
                    let _ = std::fs::remove_dir(&self.dir);
                }
            }
            Err(error) => {
                // The generations stay: the pointer may still name one.
                self.own_generations.clear();
                errors.push(error);
            }
        }
        errors
    }

    fn restore_pointer(&self, after_grab: impl FnOnce()) -> Result<(), String> {
        let pid = std::process::id();
        let aside = self.dir.join(format!("title.generation.guard-aside-{pid}"));
        match std::fs::rename(&self.pointer, &aside) {
            Ok(()) => {}
            // No pointer: nothing names a generation, nothing to restore.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "cannot take the snapshot pointer {} aside; generations {:?} kept: {error}",
                    self.pointer.display(),
                    self.own_generations
                ));
            }
        }
        after_grab();
        match self.settle_grabbed_pointer(&aside, pid) {
            Ok(()) => std::fs::remove_file(&aside)
                .map_err(|error| format!("cannot remove {}: {error}", aside.display())),
            Err(Settled::AsideDone(report)) => match std::fs::remove_file(&aside) {
                Ok(()) => Err(report),
                Err(error) => Err(format!(
                    "{report}; cannot remove {}: {error}",
                    aside.display()
                )),
            },
            // The aside may be the only copy of a pointer: never delete it.
            Err(Settled::AsideNeeded(report)) => Err(format!(
                "{report}; the grabbed pointer is preserved at {}",
                aside.display()
            )),
        }
    }

    /// Decides what the grabbed pointer at `aside` means and acts on it.
    ///
    /// An `Err` says whether the aside is still needed: it is the only copy
    /// of the pointer whenever the put-back itself failed.
    fn settle_grabbed_pointer(&self, aside: &Path, pid: u32) -> Result<(), Settled> {
        let put_back = |reason: String| -> Result<(), Settled> {
            match std::fs::hard_link(aside, &self.pointer) {
                Ok(()) => Err(Settled::AsideDone(format!(
                    "{reason}; the pointer was put back and the generations kept"
                ))),
                // A publisher already landed a newer pointer; it stands.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    Err(Settled::AsideDone(format!(
                        "{reason}; a newer pointer is in place and the generations are kept"
                    )))
                }
                Err(error) => Err(Settled::AsideNeeded(format!(
                    "{reason}; the pointer could not be put back from {}: {error}",
                    aside.display()
                ))),
            }
        };
        let text = match std::fs::read(aside) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                return put_back(format!(
                    "cannot read the snapshot pointer {}: {error}",
                    self.pointer.display()
                ));
            }
        };
        if !self.own_generations.iter().any(|own| own == text.trim()) {
            // Another publisher's pointer: give it back untouched.
            return match std::fs::hard_link(aside, &self.pointer) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                Err(error) => Err(Settled::AsideNeeded(format!(
                    "cannot put back another publisher's pointer from {}: {error}",
                    aside.display()
                ))),
            };
        }
        let Some(bytes) = &self.pointer_before else {
            // The pointer was absent before: it already is again.
            return Ok(());
        };
        let staged = self.dir.join(format!("title.generation.guard-stage-{pid}"));
        let restored = (|| {
            std::fs::write(&staged, bytes)?;
            std::fs::hard_link(&staged, &self.pointer)
        })();
        let _ = std::fs::remove_file(&staged);
        match restored {
            Ok(()) => Ok(()),
            // A publisher landed a pointer after the grab; it stands.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => put_back(format!(
                "cannot restore the snapshot pointer {}: {error}",
                self.pointer.display()
            )),
        }
    }
}

/// Why [`SnapshotsRestore::settle_grabbed_pointer`] did not finish cleanly.
enum Settled {
    /// The grabbed file is back at the pointer, or a newer pointer replaced
    /// it by design: the aside is redundant and may be removed.
    AsideDone(String),
    /// The aside may be the only copy of the pointer: it must stay.
    AsideNeeded(String),
}

impl Drop for SnapshotsRestore {
    /// A failure leaves the checkout changed, so it panics unless the thread
    /// is already unwinding (the `OwnedDir` convention).
    fn drop(&mut self) {
        let errors = self.finish_with(|| {});
        assert!(
            errors.is_empty() || std::thread::panicking(),
            "SnapshotsRestore left the checkout changed: {}",
            errors.join("; ")
        );
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

fn success_line(generation: &Path) -> String {
    format!(
        "recorded `title` snapshot (1 byte(s)) to {0}/title.rgb (+ {0}/title.meta); rgb hash 0\n",
        generation.display()
    )
}

/// A publisher that stages a pointer and renames it into place after the
/// guard has already judged the pointer it grabbed must keep that pointer.
#[test]
fn snapshots_restore_leaves_a_pointer_published_after_its_check() {
    let dir = scratch_snapshots("publish-after-check");
    std::fs::create_dir(dir.join("title.generation-1-0")).expect("old generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-1-0\n").expect("pointer");
    let mut restore = SnapshotsRestore::capture_in(dir.clone());
    let own = dir.join("title.generation-77-0");
    std::fs::create_dir(&own).expect("own generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-77-0\n").expect("pointer");
    restore.own_generation_from_stdout(&success_line(&own));
    let errors = restore.finish_with(|| {
        // The other publisher's stage-then-rename lands in the gap.
        let other = dir.join("title.generation-4294967295-0");
        std::fs::create_dir(&other).expect("other generation");
        let staged = dir.join("title.generation.stage-other");
        std::fs::write(&staged, b"title.generation-4294967295-0\n").expect("stage");
        std::fs::rename(&staged, dir.join("title.generation")).expect("publish");
    });
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        std::fs::read(dir.join("title.generation")).expect("pointer"),
        b"title.generation-4294967295-0\n",
        "the pointer a publisher renamed into place must not be overwritten"
    );
    assert!(dir.join("title.generation-4294967295-0").exists());
    assert!(!own.exists(), "the guard's own generation is still removed");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .expect("read dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        3,
        "no guard scratch files remain: {leftovers:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A restore that cannot be done must be reported and must not delete the
/// generation the pointer still names.
#[test]
#[cfg(unix)]
fn snapshots_restore_keeps_the_generation_when_the_pointer_restore_fails() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_snapshots("restore-fails");
    std::fs::create_dir(dir.join("title.generation-1-0")).expect("old generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-1-0\n").expect("pointer");
    let mut restore = SnapshotsRestore::capture_in(dir.clone());
    let own = dir.join("title.generation-77-0");
    std::fs::create_dir(&own).expect("own generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-77-0\n").expect("pointer");
    restore.own_generation_from_stdout(&success_line(&own));

    // A read-only directory refuses the rename that takes the pointer aside.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    if std::fs::write(dir.join("probe"), b"").is_ok() {
        eprintln!("skipped: root ignores directory modes, so 0555 stays writable");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let _ = std::fs::remove_dir_all(&dir);
        restore.own_generations.clear();
        return;
    }
    let errors = restore.finish_with(|| {});
    assert!(!errors.is_empty(), "the failed restore must be reported");
    // Re-arm, then check the drop surfaces the failure as a panic.
    restore.own_generation_from_stdout(&success_line(&own));
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(restore)));
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(dropped.is_err(), "a failed restore must panic from drop");
    assert!(
        own.exists(),
        "the generation the pointer names must survive"
    );
    assert_eq!(
        std::fs::read(dir.join("title.generation")).expect("pointer"),
        b"title.generation-77-0\n"
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

/// A checkout path may legally contain the ` (+ ` that separates the two
/// artifact paths on the success line; the generation must still be found.
#[test]
fn snapshots_restore_parses_a_path_containing_the_artifact_delimiter() {
    let base = scratch_snapshots("delimiter-in-path");
    let dir = base.join("odd (+ checkout").join("snapshots");
    std::fs::create_dir_all(&dir).expect("snapshots dir");
    std::fs::create_dir(dir.join("title.generation-1-0")).expect("old generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-1-0\n").expect("pointer");
    let mut restore = SnapshotsRestore::capture_in(dir.clone());
    let own = dir.join("title.generation-79-0");
    std::fs::create_dir(&own).expect("own generation");
    std::fs::write(dir.join("title.generation"), b"title.generation-79-0\n").expect("pointer");
    restore.own_generation_from_stdout(&success_line(&own));
    assert_eq!(
        restore.own_generations,
        ["title.generation-79-0"],
        "the delimiter inside the checkout path must not truncate the parsed path"
    );
    drop(restore);
    assert!(!own.exists(), "the child's own generation is removed");
    assert_eq!(
        std::fs::read(dir.join("title.generation")).expect("pointer"),
        b"title.generation-1-0\n"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// When the grabbed foreign pointer cannot be put back, the aside file is
/// the only remaining copy: it must be kept and named in the report.
#[test]
#[cfg(unix)]
fn snapshots_restore_keeps_the_aside_when_the_put_back_fails() {
    let dir = scratch_snapshots("put-back-fails");
    // The pointer is reached through a symlink so the test can break the
    // put-back without relying on directory modes (root ignores those).
    let real = dir.join("real");
    std::fs::create_dir(&real).expect("real dir");
    std::os::unix::fs::symlink(&real, dir.join("via")).expect("symlink");
    let foreign = b"title.generation-4294967295-0\n";
    std::fs::write(real.join("title.generation"), foreign).expect("foreign pointer");
    let mut restore = SnapshotsRestore::capture_in(dir.clone());
    restore.pointer = dir.join("via").join("title.generation");
    let own = dir.join("title.generation-77-0");
    std::fs::create_dir(&own).expect("own generation");
    restore.own_generation_from_stdout(&success_line(&own));
    assert_eq!(restore.own_generations, ["title.generation-77-0"]);

    let errors = restore.finish_with(|| {
        // Retarget the link after the grab: the put-back now fails ENOENT.
        std::fs::remove_file(dir.join("via")).expect("unlink");
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("via")).expect("relink");
    });
    let aside = dir.join(format!(
        "title.generation.guard-aside-{}",
        std::process::id()
    ));
    assert!(!errors.is_empty(), "the failed put-back must be reported");
    assert!(
        errors.iter().any(|e| e.contains(&*aside.to_string_lossy())),
        "the report must name the preserved aside {}: {errors:?}",
        aside.display()
    );
    assert_eq!(
        std::fs::read(&aside).expect("the aside must survive a failed put-back"),
        foreign
    );
    assert!(own.exists(), "the generations are kept on failure");
    let _ = std::fs::remove_dir_all(&dir);
}
