//! Exercises the lock slot's identity, creation, and path-limit rules that
//! back [`SaveFile::lock`](super::SaveFile::lock): the basename, staging,
//! and aliasing boundaries [`super`] enforces before a lock is granted.

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::Path;
use std::path::PathBuf;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::save::file::tests::sibling_path;
use crate::save::file::tests::{saved_store, TempDir};
use crate::save::file::{staging, SaveFile, SaveFileError, LOCK_FILE_NAME, SAVE_FILE_NAME};

/// The longest basename `parent` accepts as a save file, grown one byte at a
/// time up to `PATH_MAX` until the host refuses one; a FUSE mount accepts
/// components past [`staging::MAX_COMPONENT_LEN`].
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn longest_valid_save_basename(parent: &Path) -> usize {
    (1..4096)
        .take_while(|&len| {
            let candidate = parent.join("n".repeat(len));
            match std::fs::File::create(&candidate) {
                Ok(_) => {
                    std::fs::remove_file(&candidate).unwrap();
                    true
                }
                Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename => false,
                Err(err) => {
                    panic!("unexpected error probing the host's real path limit: {err:?}")
                }
            }
        })
        .last()
        .unwrap_or(0)
}

/// A save at the host's longest valid basename, which overflows the
/// component limit once `.lock` is appended, must still be lockable.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_save_at_the_hosts_longest_valid_basename_is_still_lockable() {
    let dir = TempDir::new("long-basename-lock");
    let basename_len = longest_valid_save_basename(&dir.path);
    let path = dir.join(&"n".repeat(basename_len));
    let file = SaveFile::at(&path);
    std::fs::File::create(&path).expect("the longest valid basename must itself be writable");
    std::fs::remove_file(&path).unwrap();

    // A host that accepts the derived sibling cannot reproduce the boundary;
    // note it and assert lockability regardless.
    let derived_sibling = sibling_path(&path, ".lock");
    let boundary_reproduced = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&derived_sibling)
        .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename);
    if !boundary_reproduced {
        drop(std::fs::remove_file(&derived_sibling));
        eprintln!(
            "note: this host accepts a {basename_len}-byte basename's derived \
             sibling, so the boundary is not reproduced here"
        );
    }

    let guard = file
        .lock()
        .expect("a save this host accepts must be lockable");
    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[test]
fn two_saves_in_one_directory_serialise_on_the_directorys_lock() {
    let dir = TempDir::new("directory-lock-serialises");
    let first = SaveFile::at(dir.join("first.sav"));
    let second = SaveFile::at(dir.join("second.sav"));

    assert_eq!(
        first.lock_path(),
        second.lock_path(),
        "every save in one directory must take one lock"
    );

    let guard = first.lock().expect("the first save must be lockable");

    let probe =
        SaveFile::open_lock_file(&second.lock_path()).expect("the lock file must be openable");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the directory lock must exclude the second save, got {other:?}"),
    }
    drop(probe);
    drop(guard);

    let released = second
        .lock()
        .expect("the lock must be free once the first guard drops");
    drop(released);
}

/// The lock path is one fixed name per directory, never derived from the
/// save's basename, whose aliases (`K` U+212A against `k`) `std` cannot enumerate.
#[test]
fn the_lock_path_is_one_fixed_name_per_directory() {
    let dir = TempDir::new("lock-path-fixed");
    let spellings = [
        SAVE_FILE_NAME.to_owned(),
        "S".repeat(140),
        "s".repeat(140),
        "\u{c4}".repeat(70),
        "\u{e4}".repeat(70),
        // The straddling pair: 141 encoded bytes against 47, one entry.
        "k".repeat(141),
        "\u{212a}".repeat(47),
    ];

    for spelling in &spellings {
        assert_eq!(
            SaveFile::at(dir.join(spelling)).lock_path(),
            dir.path.join(LOCK_FILE_NAME),
            "every save in one directory must derive one fixed lock path, whatever \
             its basename's spelling or encoded length"
        );
    }
}

/// A save whose basename is not UTF-8 is still a save, and still takes the
/// directory's lock: the lock name never reads the save's own bytes.
#[cfg(unix)]
#[test]
fn a_non_utf8_save_basename_still_takes_the_directorys_lock() {
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new("non-utf8-lock");
    let file = SaveFile::at(dir.path.join(std::ffi::OsStr::from_bytes(b"\x80")));

    assert_eq!(file.lock_path(), dir.path.join(LOCK_FILE_NAME));
    let guard = file
        .lock()
        .expect("a non-UTF-8 save basename must still be lockable");
    drop(guard);
}

#[test]
fn the_save_lock_excludes_a_second_locker_until_dropped() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dir = TempDir::new("lock");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let first_lock_released = Arc::new(AtomicBool::new(false));

    let guard = file.lock().expect("first lock must succeed");

    let probe = std::fs::OpenOptions::new()
        .write(true)
        .open(file.lock_path())
        .expect("the lock file exists while the guard is held");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the held lock must exclude a second locker, got {other:?}"),
    }
    drop(probe);

    let contender = {
        let first_lock_released = Arc::clone(&first_lock_released);
        let file = SaveFile::at(&path);
        std::thread::spawn(move || {
            let _guard = file.lock().expect("second lock must eventually succeed");
            first_lock_released.load(Ordering::SeqCst)
        })
    };
    // This gives the contender a chance to block; the nonblocking probe above proves exclusion.
    std::thread::yield_now();
    first_lock_released.store(true, Ordering::SeqCst);
    drop(guard);
    assert!(
        contender.join().expect("contender must not panic"),
        "the second lock() returned while the first guard was still held"
    );
}

/// A bare relative save-file name, unique to this process and thread, that
/// removes itself on drop -- never touching the working directory every thread shares.
struct BareRelativeSave {
    name: PathBuf,
}

impl BareRelativeSave {
    fn unique(label: &str) -> Self {
        Self {
            name: PathBuf::from(format!(
                "pokeemerald-rs-save-file-{label}-{}-{:?}.sav",
                std::process::id(),
                std::thread::current().id()
            )),
        }
    }
}

impl Drop for BareRelativeSave {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.name));
    }
}

#[test]
fn a_bare_relative_save_path_syncs_the_working_directory_after_the_rename() {
    let bare = BareRelativeSave::unique("write");
    let file = SaveFile::at(bare.name.clone());
    let (store, _, _) = saved_store();

    let synced = std::cell::RefCell::new(Vec::new());
    file.write_with(
        &store,
        |path| synced.borrow_mut().push(path.to_path_buf()),
        |bytes| staging::StagingArea::beside(file.path()).stage(bytes),
        |_| {},
    )
    .expect("writing a bare relative save path must succeed");

    assert_eq!(
        synced.into_inner(),
        vec![PathBuf::from(".")],
        "a bare relative save path's directory entry lives in the working directory, and \
         the rename must best-effort sync it exactly once"
    );
}

/// A save configured at the lock path is refused, since its next write would
/// rename a fresh inode over the file every locker holds.
#[test]
fn a_save_configured_at_the_lock_path_is_refused_rather_than_locked() {
    let dir = TempDir::new("save-at-the-lock-path");
    let file = SaveFile::at(dir.join(LOCK_FILE_NAME));

    match file.lock() {
        Err(SaveFileError::LockPathIsSave { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a save at the lock path must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// The refusal recognises the lock file by entry, not spelling; a symlink
/// stands in for a case-folding alias on a case-sensitive host.
#[cfg(unix)]
#[test]
fn a_save_that_only_resolves_to_the_lock_path_is_refused_too() {
    let dir = TempDir::new("save-aliasing-the-lock-path");
    let file = SaveFile::at(dir.join("aliased.sav"));

    std::fs::write(dir.join(LOCK_FILE_NAME), b"").unwrap();
    std::os::unix::fs::symlink(dir.join(LOCK_FILE_NAME), file.path()).unwrap();
    assert_ne!(
        file.path().file_name(),
        file.lock_path().file_name(),
        "the save must not be refusable by its name alone, or this proves nothing"
    );

    match file.lock() {
        Err(SaveFileError::LockPathIsSave { .. }) => {}
        other => panic!(
            "a save resolving to the lock file must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// A held lock denies `FILE_SHARE_DELETE`, so no rename can replace the
/// entry, while a second locker still opens the same file and blocks.
#[cfg(windows)]
#[test]
fn a_held_lock_refuses_replacement_while_still_admitting_a_second_locker() {
    let dir = TempDir::new("windows-lock-sharing");
    let first = SaveFile::at(dir.join("first.sav"));
    let second = SaveFile::at(dir.join("second.sav"));

    let guard = first.lock().expect("the first save must be lockable");

    let probe = SaveFile::open_lock_file(&second.lock_path())
        .expect("denying delete sharing must still admit a second locker's open");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the held lock must exclude the second locker, got {other:?}"),
    }
    drop(probe);

    let replacement = dir.join("replacement");
    std::fs::write(&replacement, b"").unwrap();
    assert!(
        std::fs::rename(&replacement, first.lock_path()).is_err(),
        "renaming over a held lock must fail: replacing that inode would leave this \
         guard holding an unlinked file and exclude nobody afterwards"
    );
    assert!(
        std::fs::remove_file(first.lock_path()).is_err(),
        "unlinking a held lock must fail for the same reason"
    );

    drop(guard);
}

/// A symlink in the lock slot is refused: lockers would land on whatever it
/// points at, which a rename there splits across two inodes.
#[cfg(unix)]
#[test]
fn a_symlinked_lock_slot_is_refused() {
    let dir = TempDir::new("symlinked-lock-slot");
    let elsewhere = TempDir::new("symlinked-lock-slot-target");
    let target = elsewhere.join("someone-elses.sav");
    std::fs::write(&target, b"").unwrap();

    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::os::unix::fs::symlink(&target, file.lock_path()).unwrap();

    match file.lock() {
        Err(SaveFileError::LockPathIsAlias { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a symlinked lock slot must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// A dangling symlink in the lock slot is refused before the open, whose
/// `create` would otherwise make its target outside the save directory.
#[cfg(unix)]
#[test]
fn a_dangling_symlinked_lock_slot_is_refused_without_creating_its_target() {
    let dir = TempDir::new("dangling-lock-slot");
    let elsewhere = TempDir::new("dangling-lock-slot-target");
    let target = elsewhere.join("never-created.sav");

    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::os::unix::fs::symlink(&target, file.lock_path()).unwrap();

    match file.lock() {
        Err(SaveFileError::LockPathIsAlias { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a dangling symlinked lock slot must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
    assert!(
        !target.exists(),
        "the refusal must precede the open, which would otherwise have created {} \
         outside the save directory",
        target.display()
    );
}

/// A lock slot that is not a plain file must be refused rather than
/// opened: it cannot carry a lock, and opening a FIFO there would block.
/// A directory stands in, being the one such entry `std` can create.
#[test]
fn a_lock_slot_that_is_not_a_plain_file_is_refused() {
    let dir = TempDir::new("non-file-lock-slot");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::fs::create_dir(file.lock_path()).unwrap();

    match file.lock() {
        Err(SaveFileError::LockPathNotAPlainFile { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a lock slot that is not a plain file must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// A lock file this user cannot write is still lockable, through the
/// read-only open; only CI's unprivileged runners reach that path.
#[cfg(unix)]
#[test]
fn a_lock_file_this_user_cannot_write_is_still_lockable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("read-only-lock-file");
    let first = SaveFile::at(dir.join("first.sav"));
    let second = SaveFile::at(dir.join("second.sav"));

    std::fs::write(first.lock_path(), b"").unwrap();
    std::fs::set_permissions(first.lock_path(), std::fs::Permissions::from_mode(0o444)).unwrap();

    let guard = first
        .lock()
        .expect("a lock file this user cannot write must still be lockable");

    let probe = SaveFile::open_lock_file(&second.lock_path())
        .expect("a second saver must still open the shared lock");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("a read-only lock handle must still exclude a second saver, got {other:?}"),
    }
    drop(probe);
    drop(guard);
}

/// A hard link in the lock slot is accepted: it is a name of the inode the
/// guard holds, so later lockers still land on it.
#[cfg(unix)]
#[test]
fn a_hard_linked_lock_slot_is_still_accepted() {
    let dir = TempDir::new("hard-linked-lock-slot");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));

    std::fs::write(file.lock_path(), b"").unwrap();
    std::fs::hard_link(file.lock_path(), dir.join("backup-snapshot")).unwrap();

    let guard = file
        .lock()
        .expect("a second name for the lock file's own inode must not refuse the lock");
    drop(guard);
}

/// An ordinary lock file an earlier run left behind must still be usable:
/// the slot check must reject aliases, not every pre-existing file.
#[cfg(unix)]
#[test]
fn an_ordinary_pre_existing_lock_file_is_still_accepted() {
    let dir = TempDir::new("pre-existing-lock-slot");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::fs::write(file.lock_path(), b"left by an earlier run").unwrap();

    let guard = file
        .lock()
        .expect("an ordinary lock file from an earlier run must be reusable");
    drop(guard);
}

/// The refusal compares inode identity, not canonical text; a hard link
/// gives one inode two canonical paths on any Unix host.
#[cfg(unix)]
#[test]
fn a_save_sharing_the_lock_files_inode_is_refused_despite_a_different_canonical_path() {
    let dir = TempDir::new("save-hard-linked-to-the-lock");
    let file = SaveFile::at(dir.join("linked.sav"));

    std::fs::write(dir.join(LOCK_FILE_NAME), b"").unwrap();
    std::fs::hard_link(dir.join(LOCK_FILE_NAME), file.path()).unwrap();
    assert_ne!(
        std::fs::canonicalize(file.path()).unwrap(),
        std::fs::canonicalize(file.lock_path()).unwrap(),
        "the two names must canonicalise differently, or this does not exercise the \
         boundary a path-text comparison misses"
    );

    match file.lock() {
        Err(SaveFileError::LockPathIsSave { .. }) => {}
        other => panic!(
            "a save on the lock file's own inode must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// The refusal must catch only the save that *is* the lock file. An
/// ordinary save sharing the directory holds that lock rather than being
/// refused by it, before and after it exists on disk.
#[test]
fn an_ordinary_save_beside_the_lock_file_still_locks() {
    let dir = TempDir::new("ordinary-save-beside-lock");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    let guard = file
        .lock()
        .expect("a save that does not exist yet must be lockable");
    file.write(&store).unwrap();
    drop(guard);

    let guard = file
        .lock()
        .expect("an existing ordinary save must still be lockable");
    drop(guard);
}

/// `_POSIX_NAME_MAX` is 14.
#[test]
fn the_lock_file_name_fits_the_posix_minimum_component_limit() {
    assert!(
        LOCK_FILE_NAME.len() <= 14,
        "{LOCK_FILE_NAME:?} exceeds 14 bytes"
    );
}

/// A `umask 077` creator leaves `0600`, which no later owner can even read.
#[cfg(unix)]
#[test]
fn a_freshly_created_lock_file_is_lockable_by_every_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("lock-mode");
    let file = SaveFile::at(dir.path.join("a.sav"));
    let guard = file.lock().expect("a guard");
    let mode = std::fs::metadata(dir.path.join(LOCK_FILE_NAME))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o666,
        0o666,
        "lock mode {mode:o} shuts later owners out"
    );
    let leftovers: Vec<_> = std::fs::read_dir(&dir.path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name != LOCK_FILE_NAME)
        .collect();
    assert!(
        leftovers.is_empty(),
        "locking left {leftovers:?} beside the save"
    );
    drop(guard);
}

/// Only the creator widens; an existing slot keeps its owner's mode.
#[cfg(unix)]
#[test]
fn a_pre_existing_lock_file_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("lock-mode-kept");
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    std::fs::write(&lock_path, b"").unwrap();
    std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let file = SaveFile::at(dir.path.join("a.sav"));
    let guard = file.lock().expect("a guard");
    let mode = std::fs::metadata(&lock_path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "an existing slot's mode was rewritten to {mode:o}"
    );
    drop(guard);
}

/// Staging must never unlink an entry it did not create.
#[cfg(unix)]
#[test]
fn a_file_already_bearing_the_lock_staging_name_survives_a_lock() {
    let dir = TempDir::new("lock-stage-collision");
    let taken = SaveFile::staging_name();
    let bystander = dir.path.join(&taken);
    std::fs::write(&bystander, b"a save that happens to bear the staging name").unwrap();
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    let mut draws = vec![taken, SaveFile::staging_name()].into_iter();

    let created = SaveFile::create_lock_file_with(&lock_path, || draws.next().unwrap())
        .expect("a second draw finds a free staging name");

    assert!(created.is_some(), "the slot was created");
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"a save that happens to bear the staging name"
    );
    assert!(lock_path.exists());
    assert!(
        draws.next().is_none(),
        "the taken name was drawn and skipped"
    );
}

/// A pool of taken staging names exhausts creation rather than unlinking one.
#[cfg(unix)]
#[test]
fn creation_gives_up_when_every_drawn_staging_name_is_taken() {
    let dir = TempDir::new("lock-stage-exhausted");
    let taken = SaveFile::staging_name();
    let leftover = dir.path.join(&taken);
    std::fs::write(&leftover, b"leftover").unwrap();
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    let mut draws = 0usize;

    let error = SaveFile::create_lock_file_with(&lock_path, || {
        draws += 1;
        taken.clone()
    })
    .expect_err("no free staging name");

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(draws, usize::from(u8::MAX) + 1);
    assert_eq!(std::fs::read(&leftover).unwrap(), b"leftover");
    assert!(!lock_path.exists());
}

/// Leftover staging names never stand between a saver and an existing lock.
#[cfg(unix)]
#[test]
fn an_existing_lock_is_opened_even_when_every_staging_name_is_taken() {
    let dir = TempDir::new("lock-stage-existing");
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    std::fs::write(&lock_path, b"").unwrap();

    let opened = SaveFile::open_lock_file_with(&lock_path, |_| {
        panic!("an existing slot is opened without staging")
    })
    .expect("the existing lock is opened without staging");

    drop(opened);
}

/// Leftover staging names beside an absent slot are skipped, never unlinked.
#[cfg(unix)]
#[test]
fn leftover_staging_names_beside_the_lock_slot_still_admit_a_first_lock() {
    let dir = TempDir::new("lock-staging-names-taken");
    let leftovers: Vec<String> = (0..16).map(|_| SaveFile::staging_name()).collect();
    for leftover in &leftovers {
        std::fs::write(dir.path.join(leftover), b"leftover").unwrap();
    }
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    let mut draws = leftovers
        .iter()
        .cloned()
        .chain(std::iter::once(SaveFile::staging_name()));

    let file = SaveFile::open_lock_file_with(&lock_path, |path| {
        SaveFile::create_lock_file_with(path, || draws.next().unwrap())
    })
    .expect("a first save finds a free staging name");

    file.lock().expect("the created slot locks");
    for leftover in &leftovers {
        assert_eq!(
            std::fs::read(dir.path.join(leftover)).unwrap(),
            b"leftover",
            "{leftover:?} was disturbed"
        );
    }
}

#[cfg(unix)]
#[test]
fn the_drawn_lock_staging_name_is_no_longer_than_the_lock_name() {
    let pid_name = format!(".lk{}", std::process::id());
    for _ in 0..64 {
        let name = SaveFile::staging_name();
        let digits = name.strip_prefix(".lk").expect("a .lk staging name");
        assert_eq!(digits.len(), 10, "{name} is not ten digits wide");
        assert_eq!(name.len(), LOCK_FILE_NAME.len());
        assert!(
            digits.bytes().all(|b| b.is_ascii_hexdigit()),
            "{name} is not hexadecimal"
        );
        assert_ne!(name, pid_name);
    }
}

/// Staging cleanup unlinks only the inode it created.
#[cfg(unix)]
#[test]
fn lock_staging_cleanup_spares_an_entry_it_did_not_create() {
    let dir = TempDir::new("lock-stage-not-mine");
    let staged = dir.path.join(SaveFile::staging_name());
    let mine = std::fs::File::create(&staged).unwrap();
    std::fs::remove_file(&staged).unwrap();
    std::fs::write(&staged, b"a stranger's file under the staging name").unwrap();
    SaveFile::remove_only_own_staging(&staged, &mine);
    assert_eq!(
        std::fs::read(&staged).expect("the stranger's file survives"),
        b"a stranger's file under the staging name"
    );
}

/// A directory exactly `total_len` bytes long, from components under 255 bytes.
#[cfg(target_os = "linux")]
fn directory_of_length(root: &Path, total_len: usize) -> PathBuf {
    let mut path = root.to_path_buf();
    let mut remaining = total_len - path.as_os_str().len();
    assert!(remaining >= 2, "the scratch root must leave room to nest");
    let mut first = remaining % 201;
    if first < 2 {
        first += 201;
    }
    path.push("d".repeat(first - 1));
    std::fs::create_dir(&path).unwrap();
    remaining -= first;
    while remaining > 0 {
        path.push("d".repeat(200));
        std::fs::create_dir(&path).unwrap();
        remaining -= 201;
    }
    assert_eq!(path.as_os_str().len(), total_len);
    path
}

/// A directory with no room for the 13-byte lock name fails closed: no
/// guard, nothing created, no second lock identity.
#[cfg(target_os = "linux")]
#[test]
fn a_directory_with_no_room_for_the_lock_name_is_refused_rather_than_locked() {
    const LONGEST_PATH: usize = 4095;
    let temp = TempDir::new("deep-directory");
    let parent = directory_of_length(&temp.path, LONGEST_PATH - LOCK_FILE_NAME.len());
    let save_path = parent.join("a");
    std::fs::write(&save_path, [0u8; 1]).expect("the host accepts this save path");
    assert!(save_path.as_os_str().len() <= LONGEST_PATH);

    let file = SaveFile::at(&save_path);
    match file.lock() {
        Err(SaveFileError::Lock { path, source }) => {
            assert_eq!(
                path,
                file.lock_path(),
                "the refusal must name the lock this save cannot have"
            );
            assert_eq!(source.kind(), std::io::ErrorKind::InvalidFilename);
        }
        other => panic!(
            "a directory with no room for the lock name must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }

    let entries: Vec<_> = std::fs::read_dir(&parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name != "a")
        .collect();
    assert!(
        entries.is_empty(),
        "a refused lock must create nothing: {entries:?}"
    );
}

/// Two spellings of one save directory contend for the one `.emerald.lock`
/// inode; the spelling the name does not fit beside fails closed.
#[cfg(target_os = "linux")]
#[test]
fn two_spellings_of_one_save_directory_contend_for_one_lock() {
    const LONGEST_PATH: usize = 4095;
    let deep_root = TempDir::new("aliased-directory-deep");
    let short_root = TempDir::new("aliased-directory-short");
    let deep = directory_of_length(&deep_root.path, LONGEST_PATH - LOCK_FILE_NAME.len());
    let short = short_root.join("d");
    std::os::unix::fs::symlink(&deep, &short).unwrap();

    let long_spelling = SaveFile::at(deep.join("a"));
    let short_spelling = SaveFile::at(short.join("a"));
    std::fs::write(long_spelling.path(), [0u8; 1]).expect("the host accepts the long save path");
    assert!(
        short_spelling.path().exists(),
        "both spellings must reach the one save"
    );
    assert!(
        std::fs::File::create(deep.join(LOCK_FILE_NAME))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "the long spelling must have no room for the lock name the short spelling \
         has room for, or the two spellings were never at risk of splitting"
    );

    let guard = short_spelling
        .lock()
        .expect("the short spelling reaches the lock name");
    let probe = short_spelling
        .open_lock_slot()
        .expect("the one lock slot opens");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the one lock slot must exclude a second locker, got {other:?}"),
    }
    drop(probe);

    assert!(
        matches!(long_spelling.lock(), Err(SaveFileError::Lock { .. })),
        "the long spelling must fail closed rather than take a second identity \
         while the short spelling holds the directory's one lock"
    );
    drop(guard);
}
