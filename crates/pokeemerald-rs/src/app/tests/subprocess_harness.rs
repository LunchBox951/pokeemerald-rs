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
/// the entry it made stays identifiable for as long as the guard lives.
struct ScratchPack {
    path: std::path::PathBuf,
    /// Held open so the created file's identity cannot be recycled for a replacement while we
    /// compare.
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
    /// Ownership could not be established -- the platform offers no file identity, or reading or
    /// acting on it failed -- so nothing was unlinked.
    Unverifiable,
}

impl ScratchPack {
    fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Unlinks the pathname only if it still names the file this guard created, and fails closed
    /// ([`ScratchRemoval::Unverifiable`]) wherever that cannot be established.
    ///
    /// Unix compares `(dev, ino)` of the path (`symlink_metadata`, so a planted symlink never
    /// matches) against the creating handle. std has no unlink-by-handle, so a swap between that
    /// check and the unlink remains possible; this closes the window the child run opens, not that
    /// one. Windows reads the handle's [`win_fs::WindowsFileIdentity`] and deletes through a
    /// handle opened on the path only if that handle has the same identity.
    fn remove_if_owned(&self) -> ScratchRemoval {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;

            let current = match std::fs::symlink_metadata(&self.path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return ScratchRemoval::Gone;
                }
                Err(_) => return ScratchRemoval::Unverifiable,
            };
            let Ok(created) = self.file.metadata() else {
                return ScratchRemoval::Unverifiable;
            };
            if (current.dev(), current.ino()) != (created.dev(), created.ino()) {
                return ScratchRemoval::Replaced;
            }
            match std::fs::remove_file(&self.path) {
                Ok(()) => ScratchRemoval::Removed,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => ScratchRemoval::Gone,
                Err(_) => ScratchRemoval::Unverifiable,
            }
        }
        #[cfg(windows)]
        {
            let Ok(identity) = win_fs::WindowsFileIdentity::of(&self.file) else {
                return ScratchRemoval::Unverifiable;
            };
            match win_fs::remove_through_verified_handle(&self.path, identity) {
                Ok(win_fs::VerifiedRemoval::NotOurs) => ScratchRemoval::Replaced,
                Ok(win_fs::VerifiedRemoval::Unlinked | win_fs::VerifiedRemoval::Pending) => {
                    ScratchRemoval::Removed
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => ScratchRemoval::Gone,
                Err(_) => ScratchRemoval::Unverifiable,
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = &self.file;
            ScratchRemoval::Unverifiable
        }
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

/// The `attempt`th candidate name for the replacement's staging file beside `pack`.
#[cfg(unix)]
fn staged_candidate(pack: &std::path::Path, attempt: u32) -> std::path::PathBuf {
    if attempt == 0 {
        pack.with_extension("staged")
    } else {
        pack.with_extension(format!("staged-{attempt}"))
    }
}

/// Replaces the pack at `path` with a different file holding the same bytes (so a child still
/// loads a valid entryless pack) and returns the name the original stays reachable under. The
/// replacement is staged under an exclusively created name -- an occupied name, including a
/// symlink, is skipped, never followed or truncated -- and swapped in with one rename, so the
/// pathname never lacks a pack. Everything it returns or leaves behind is the test's own to remove.
#[cfg(unix)]
fn replace_pack_with_lookalike(path: &std::path::Path) -> std::path::PathBuf {
    use std::io::Write as _;

    let bytes = std::fs::read(path).expect("the freshly created pack is readable");
    let (staged, mut handle) = (0..64)
        .find_map(|attempt| {
            let staged = staged_candidate(path, attempt);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)
            {
                Ok(handle) => Some((staged, handle)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => panic!("the staging file must be creatable: {error}"),
            }
        })
        .expect("a free staging name must exist");
    handle
        .write_all(&bytes)
        .expect("the staging file must be writable");
    drop(handle);
    let aside = staged.with_extension("aside");
    if let Err(error) = std::fs::hard_link(path, &aside) {
        drop(std::fs::remove_file(&staged));
        panic!("the original must be kept reachable beside the pack: {error}");
    }
    std::fs::rename(&staged, path).expect("the replacement must swap in");
    aside
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
            let aside = replace_pack_with_lookalike(path);
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

/// Staging the replacement must not write through a symlink planted at the first staging name:
/// its target keeps its contents and the staging file lands at the next free name.
#[cfg(unix)]
#[test]
fn scratch_pack_replacement_staging_does_not_write_through_a_planted_symlink() {
    let victim = std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-staged-victim-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    {
        use std::io::Write as _;

        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&victim)
            .expect("the victim must be creatable")
            .write_all(b"precious")
            .expect("the victim must be writable");
    }
    let state = std::cell::RefCell::new(None);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_boundary_harness(|path| {
            let planted = staged_candidate(path, 0);
            std::os::unix::fs::symlink(&victim, &planted)
                .expect("the first staging name must be free to plant the symlink");
            *state.borrow_mut() = Some((path.to_path_buf(), planted.clone(), None));
            let aside = replace_pack_with_lookalike(path);
            *state.borrow_mut() = Some((path.to_path_buf(), planted, Some(aside)));
        });
    }));
    let victim_contents = std::fs::read_to_string(&victim);
    drop(std::fs::remove_file(&victim));
    if let Some((path, planted, aside)) = state.into_inner() {
        drop(std::fs::remove_file(&path));
        drop(std::fs::remove_file(&planted));
        if let Some(aside) = aside {
            drop(std::fs::remove_file(&aside));
        }
    }
    outcome.expect("the harness itself must pass");
    assert_eq!(
        victim_contents.as_deref().ok(),
        Some("precious"),
        "the symlink target must stay unchanged"
    );
}

/// The `attempt`th candidate name for the pack's displaced original beside `pack`.
#[cfg(any(unix, windows))]
fn aside_candidate(pack: &std::path::Path, attempt: u32) -> std::path::PathBuf {
    if attempt == 0 {
        pack.with_extension("aside")
    } else {
        pack.with_extension(format!("aside-{attempt}"))
    }
}

/// Moves the pack at `path` to a free aside name and returns that name, leaving `path` vacant.
/// The aside name is claimed exclusively first (`create_new`, so an occupied name, including a
/// symlink, is skipped, never overwritten) and the pack is then renamed over that claim. The
/// rename moves the entry even while the guard's creating handle is open (std opens with
/// `FILE_SHARE_DELETE` on Windows), so the vacated name is free at once rather than
/// delete-pending as it would be after an unlink. The returned name is the test's own to remove.
#[cfg(any(unix, windows))]
fn move_pack_aside(path: &std::path::Path) -> std::path::PathBuf {
    let aside = (0..64)
        .find_map(|attempt| {
            let aside = aside_candidate(path, attempt);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&aside)
            {
                Ok(_) => Some(aside),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => panic!("the aside name must be claimable: {error}"),
            }
        })
        .expect("a free aside name must exist");
    if let Err(error) = std::fs::rename(path, &aside) {
        drop(std::fs::remove_file(&aside));
        panic!("the original must move aside: {error}");
    }
    aside
}

/// Swaps the pack at `path` for a different file and returns the name the original stays
/// reachable under (see [`move_pack_aside`]). The replacement is created with `create_new` once
/// the pack name is vacated, so a planted symlink there fails the test instead of being written
/// through. Everything it returns or leaves behind is the test's own to remove.
#[cfg(any(unix, windows))]
fn displace_pack_with_replacement(
    path: &std::path::Path,
    replacement: &[u8],
) -> std::path::PathBuf {
    use std::io::Write as _;

    let aside = move_pack_aside(path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .expect("the vacated pack name must be exclusively creatable")
        .write_all(replacement)
        .expect("the replacement must be writable");
    aside
}

/// The guard unlinks the file it created, but never an entry that replaced it, and says so
/// rather than staying silent. Exercises the cleanup directly, without a child process.
#[cfg(unix)]
#[test]
fn scratch_pack_guard_removes_only_the_file_it_created() {
    guard_removes_only_the_file_it_created();
}

#[cfg(unix)]
fn guard_removes_only_the_file_it_created() {
    let label = "guard-identity";
    let own = write_empty_scratch_pack(label);
    let own_path = own.path().to_path_buf();
    assert_eq!(own.remove_if_owned(), ScratchRemoval::Removed);
    assert!(!own_path.exists(), "the created pack must be unlinked");
    assert_eq!(own.remove_if_owned(), ScratchRemoval::Gone);

    let pack = write_empty_scratch_pack(label);
    let path = pack.path().to_path_buf();
    let aside = displace_pack_with_replacement(&path, b"replacement");
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

/// The guard regression must not overwrite or delete a pre-existing file at its predictable
/// first aside name, nor write through a symlink planted at the next one.
#[cfg(unix)]
#[test]
fn scratch_pack_guard_test_spares_preexisting_aside_names() {
    use std::io::Write as _;

    let pack_name = scratch_pack_candidate("guard-identity", 0);
    let aside = aside_candidate(&pack_name, 0);
    let planted = aside_candidate(&pack_name, 1);
    let victim = std::env::temp_dir().join(format!(
        "pokeemerald-rs-923-aside-victim-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    for (name, contents) in [(&aside, &b"precious"[..]), (&victim, &b"victim"[..])] {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(name)
            .expect("the name must be free to plant")
            .write_all(contents)
            .expect("the planted file must be writable");
    }
    if let Err(error) = std::os::unix::fs::symlink(&victim, &planted) {
        drop(std::fs::remove_file(&aside));
        drop(std::fs::remove_file(&victim));
        panic!("the second aside name must be free to plant the symlink: {error}");
    }

    let outcome = std::panic::catch_unwind(guard_removes_only_the_file_it_created);
    let aside_contents = std::fs::read(&aside);
    let victim_contents = std::fs::read(&victim);
    drop(std::fs::remove_file(&aside));
    drop(std::fs::remove_file(&planted));
    drop(std::fs::remove_file(&victim));
    outcome.expect("the guard regression must pass around planted names");
    assert_eq!(
        aside_contents.ok().as_deref(),
        Some(&b"precious"[..]),
        "the guard regression clobbered a file it never created"
    );
    assert_eq!(
        victim_contents.ok().as_deref(),
        Some(&b"victim"[..]),
        "the guard regression wrote through a planted symlink"
    );
}

/// Windows: the guard deletes the pack it created while its own creating handle is still open,
/// and spares an entry that replaced it.
#[cfg(windows)]
#[test]
fn scratch_pack_guard_removes_only_the_file_it_created_on_windows() {
    let label = "guard-identity-windows";
    let own = write_empty_scratch_pack(label);
    let own_path = own.path().to_path_buf();
    assert_eq!(own.remove_if_owned(), ScratchRemoval::Removed);
    drop(own);
    assert!(
        !own_path.exists(),
        "the created pack must be gone once the guard drops"
    );

    let pack = write_empty_scratch_pack(label);
    let path = pack.path().to_path_buf();
    let aside = displace_pack_with_replacement(&path, b"replacement");
    let outcome = pack.remove_if_owned();
    drop(pack);
    let survived = std::fs::read(&path);
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_file(&aside));
    assert_eq!(outcome, ScratchRemoval::Replaced);
    assert_eq!(survived.ok().as_deref(), Some(&b"replacement"[..]));
}

/// A symlink to the displaced original, planted at the pack's pathname, is a different entry: the
/// guard must report it as a replacement and leave it in place rather than follow it to a match.
#[cfg(unix)]
#[test]
fn scratch_pack_guard_spares_a_symlink_to_its_displaced_original() {
    let pack = write_empty_scratch_pack("guard-symlink-to-original");
    let path = pack.path().to_path_buf();
    let aside = move_pack_aside(&path);
    std::os::unix::fs::symlink(&aside, &path).expect("the vacated name must take the symlink");

    let outcome = pack.remove_if_owned();
    let still_symlink = std::fs::symlink_metadata(&path).map(|m| m.file_type().is_symlink());
    let aside_exists = aside.is_file();
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_file(&aside));
    drop(pack);
    assert_eq!(outcome, ScratchRemoval::Replaced);
    assert_eq!(
        still_symlink.ok(),
        Some(true),
        "the planted symlink must be left intact"
    );
    assert!(aside_exists, "the displaced original must survive");
}
