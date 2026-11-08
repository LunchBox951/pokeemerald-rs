//! The subprocess stderr harness: re-executes this test binary to read back
//! what [`App::start_title_music`]'s own `eprintln!` wrote.

use super::super::App;

/// Set only on the child process re-executed below, so it can tell that run
/// apart from an ordinary one -- including CI's blanket `cargo test -p
/// pokeemerald-rs -- --ignored` real-pack sweep, where this stays unset.
const START_TITLE_MUSIC_BOUNDARY_CHILD: &str =
    "POKEEMERALD_RS_923_START_TITLE_MUSIC_BOUNDARY_CHILD";

/// The `attempt`th candidate scratch path: unique per test/thread, mirroring
/// `flow::tests::TempSave::new`, with a counter after the first to step past an occupied name.
fn scratch_pack_candidate(label: &str, attempt: u32) -> std::path::PathBuf {
    let suffix = if attempt == 0 {
        String::new()
    } else {
        format!("-{attempt}")
    };
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-{label}-{}-{:?}{suffix}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// A scratch pack this harness created, owned through cleanup: it keeps the creating handle, so
/// the entry it made stays identifiable by `(dev, ino)` for as long as the guard lives.
struct ScratchPack {
    path: std::path::PathBuf,
    /// Held open so the created inode cannot be recycled for a replacement while we compare.
    file: std::fs::File,
}

/// What [`ScratchPack::remove_if_owned`] did to the entry at the pack's pathname.
#[derive(Debug, PartialEq, Eq)]
enum ScratchRemoval {
    /// The entry was still the file this guard created, and was unlinked.
    Removed,
    /// Nothing is at the pathname any more.
    Gone,
    /// A different entry now sits at the pathname; it was left alone.
    Replaced,
    /// This platform offers no identity to compare, so nothing was unlinked.
    Unverifiable,
}

impl ScratchPack {
    fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Unlinks the pathname only if it still names the file this guard created. `symlink_metadata`
    /// does not follow a link, so a symlink planted at the name never matches the created file.
    /// (std has no unlink-by-handle, so a swap between the check and the unlink remains possible;
    /// this closes the window the child run opens, not that one.)
    fn remove_if_owned(&self) -> ScratchRemoval {
        let current = match std::fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return ScratchRemoval::Gone;
            }
            Err(_) => return ScratchRemoval::Replaced,
        };
        let created = self.file.metadata().ok().and_then(|m| file_identity(&m));
        let Some(created) = created else {
            return ScratchRemoval::Unverifiable;
        };
        if file_identity(&current) != Some(created) {
            return ScratchRemoval::Replaced;
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => ScratchRemoval::Removed,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ScratchRemoval::Gone,
            Err(_) => ScratchRemoval::Replaced,
        }
    }
}

/// What distinguishes the file `metadata` describes from a later file at the same pathname: the
/// `(dev, ino)` pair on Unix. Windows std exposes no file id on stable, so it falls back to the
/// 100 ns creation time and the length, which a freshly created replacement does not share.
/// `None` where the platform offers neither.
fn file_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        Some((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        Some((metadata.creation_time(), metadata.file_size()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = metadata;
        None
    }
}

impl Drop for ScratchPack {
    fn drop(&mut self) {
        match self.remove_if_owned() {
            ScratchRemoval::Removed | ScratchRemoval::Gone => {}
            ScratchRemoval::Replaced => eprintln!(
                "the scratch pack {} was replaced after this harness created it; leaving the replacement in place",
                self.path.display()
            ),
            ScratchRemoval::Unverifiable => eprintln!(
                "cannot verify the scratch pack {} is still the file this harness created; leaving it in place",
                self.path.display()
            ),
        }
    }
}

/// A scratch, valid but entryless asset pack: an empty
/// [`pack_format::PackWriter`] already serializes exactly this, at the live
/// [`pack_format::FORMAT_VERSION`]. The file is created exclusively
/// (`O_EXCL`), so a name already occupied -- including by a symlink -- is
/// skipped rather than followed and truncated, and the bytes go through the
/// handle that creation returned, which the returned guard keeps.
fn write_empty_scratch_pack(label: &str) -> ScratchPack {
    use std::io::Write as _;

    let bytes = pack_format::PackWriter::new()
        .finish()
        .expect("an entryless pack always serializes");
    for attempt in 0..64 {
        let path = scratch_pack_candidate(label, attempt);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .expect("the scratch pack path must be writable");
                return ScratchPack { path, file };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("the scratch pack path must be creatable: {error}"),
        }
    }
    panic!("every candidate scratch pack name was already occupied");
}

/// A symlink planted at candidate 0 (the name the helper tries first) must be left alone: its
/// target keeps its contents and the pack lands at a different name.
#[cfg(unix)]
#[test]
fn scratch_pack_creation_does_not_write_through_a_planted_symlink() {
    let label = "symlink-regression";
    let target = std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-{label}-target-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    {
        use std::io::Write as _;

        let mut target_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .expect("the symlink target must be creatable");
        target_file
            .write_all(b"precious")
            .expect("the symlink target must be writable");
    }
    // Candidate 0 is the helper's first choice; an occupied name fails the test.
    let planted = scratch_pack_candidate(label, 0);
    if let Err(error) = std::os::unix::fs::symlink(&target, &planted) {
        drop(std::fs::remove_file(&target));
        panic!("candidate 0 must be free to plant the symlink: {error}");
    }

    let pack = write_empty_scratch_pack(label);
    let written = pack.path().to_path_buf();

    let target_contents = std::fs::read_to_string(&target);
    let pack_exists = written.is_file();
    drop(pack);
    drop(std::fs::remove_file(&planted));
    drop(std::fs::remove_file(&target));
    assert_eq!(
        target_contents.as_deref().ok(),
        Some("precious"),
        "the symlink target must stay unchanged"
    );
    assert_ne!(written, planted, "the occupied name must be skipped");
    assert_eq!(
        written,
        scratch_pack_candidate(label, 1),
        "the pack must land at the next candidate"
    );
    assert!(pack_exists, "the pack must be written at the fresh name");
}

/// Re-executes this test binary as a child against a scratch entryless pack
/// (pack load succeeds, song lookup fails), because only a subprocess can
/// read back what [`App::start_title_music`]'s own `eprintln!` wrote.
#[test]
fn start_title_music_failure_emits_its_subsystem_prefix_once_at_the_eprintln_boundary() {
    if std::env::var_os(START_TITLE_MUSIC_BOUNDARY_CHILD).is_some() {
        let mut context = crate::music::MusicContext::new();
        // Never actually called: the song lookup fails first.
        let played = App::start_title_music(
            &crate::pack_source::PackSource::Runtime,
            &mut context,
            || {
                Ok(platform::AudioOutput::null(
                    crate::music::RING_CAPACITY_FRAMES,
                ))
            },
        );
        assert!(
            played.is_none(),
            "an entryless pack has no `mus_title` entry, so the song lookup must fail"
        );
        return;
    }

    run_boundary_harness(|_| {});
}

/// The parent half of the harness: creates the scratch pack, hands its path to `after_create`
/// (a seam for a test to interfere with the pack before the child runs), re-executes this binary
/// against it, checks the child's stderr, then lets the pack guard clean up.
fn run_boundary_harness(after_create: impl FnOnce(&std::path::Path)) {
    let pack = write_empty_scratch_pack("start-title-music-boundary");
    after_create(pack.path());
    let exe = std::env::current_exe().expect("the running test binary has a path");
    let output = std::process::Command::new(exe)
        .args([
            "--exact",
            "--nocapture",
            "app::tests::subprocess_harness::start_title_music_failure_emits_its_subsystem_prefix_once_at_the_eprintln_boundary",
        ])
        .env(START_TITLE_MUSIC_BOUNDARY_CHILD, "1")
        .env(pack_format::PACK_PATH_ENV, pack.path())
        .output()
        .expect("re-running this test binary must succeed");
    drop(pack);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("the title screen will play without music"),
        "the child never reached the boundary; stderr:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("music:").count(),
        1,
        "the eprintln! boundary must name its subsystem once across all of stderr, not once per prefix layer:\n{stderr}"
    );
    assert!(
        output.status.success(),
        "the child test must pass: status {:?}\nstderr:\n{stderr}",
        output.status
    );
}

/// A file that replaces the scratch pack at its pathname during the harness run must not be
/// unlinked by the harness's cleanup. The replacement is swapped in through the harness's
/// `after_create` seam, so it is installed before the child runs and before cleanup, with no
/// scheduling dependence.
#[cfg(unix)]
#[test]
fn scratch_pack_cleanup_spares_a_file_that_replaced_it_during_the_child_run() {
    let swapped = std::cell::RefCell::new(None);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_boundary_harness(|path| {
            let bytes = std::fs::read(path).expect("the freshly created pack is readable");
            let aside = path.with_extension("aside");
            // Stage the replacement (same bytes, so the child still loads a valid entryless
            // pack), keep the original reachable under another name, then swap with one rename
            // so the pathname never lacks a pack.
            let staged = path.with_extension("staged");
            std::fs::write(&staged, &bytes).unwrap();
            std::fs::hard_link(path, &aside).unwrap();
            std::fs::rename(&staged, path).unwrap();
            *swapped.borrow_mut() = Some((path.to_path_buf(), aside));
        });
    }));
    let (path, aside) = swapped
        .into_inner()
        .expect("the after_create seam must run");
    let replacement_survived = path.exists();
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_file(&aside));
    outcome.expect("the harness itself must pass");
    assert!(
        replacement_survived,
        "cleanup unlinked a replacement file it never created"
    );
}

/// The guard unlinks the file it created, but never an entry that replaced it, and says so
/// rather than staying silent. Exercises the cleanup directly, without a child process.
#[cfg(unix)]
#[test]
fn scratch_pack_guard_removes_only_the_file_it_created() {
    let label = "guard-identity";
    let own = write_empty_scratch_pack(label);
    let own_path = own.path().to_path_buf();
    assert_eq!(own.remove_if_owned(), ScratchRemoval::Removed);
    assert!(!own_path.exists(), "the created pack must be unlinked");
    assert_eq!(own.remove_if_owned(), ScratchRemoval::Gone);

    let pack = write_empty_scratch_pack(label);
    let path = pack.path().to_path_buf();
    let aside = path.with_extension("aside");
    std::fs::rename(&path, &aside).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    let outcome = pack.remove_if_owned();
    let survived = std::fs::read(&path);
    drop(pack);
    let survived_drop = std::fs::read(&path);
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_file(&aside));
    assert_eq!(outcome, ScratchRemoval::Replaced);
    assert_eq!(survived.ok().as_deref(), Some(&b"replacement"[..]));
    assert_eq!(survived_drop.ok().as_deref(), Some(&b"replacement"[..]));
}
