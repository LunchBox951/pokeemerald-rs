//! Shared fixtures for the title test modules: synthetic images and scratch pack files.

use crate::pack_test_support::pack_bytes;

pub(super) const RED_BGR555: u16 = 0x001F;
pub(super) const GREEN_BGR555: u16 = 0x03E0;

pub(super) fn tiled_image(
    width: usize,
    height: usize,
    tile_value: impl Fn(usize, usize) -> u8,
) -> Vec<u8> {
    let mut pixels = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = tile_value(x / 8, y / 8);
        }
    }
    pixels
}

/// A fixture file's identity, read from the handle the guard keeps open so
/// [`Drop`] can tell the file it created from anything later placed at the
/// same path. Holding that handle also pins the inode, so deleting the file
/// and creating another cannot hand the replacement the same identity.
#[cfg(unix)]
#[derive(PartialEq, Eq)]
struct FileIdentity(u64, u64);

#[cfg(unix)]
impl FileIdentity {
    fn of(meta: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt as _;
        Self(meta.dev(), meta.ino())
    }
}

/// Windows reads the volume serial number and file ID off the open handle
/// (`win-fs` owns that query), so a same-length file recreated under the
/// same name is a different identity.
#[cfg(windows)]
type FileIdentity = win_fs::WindowsFileIdentity;

/// No supported identity: [`Drop`] leaves the file in place rather than risk
/// unlinking an entry it cannot prove it owns.
#[cfg(not(any(unix, windows)))]
struct FileIdentity;

/// How many private-directory names a claim tries before giving up.
const MAX_CLAIM_ATTEMPTS: u32 = 1024;

/// Creates the private directory a guard's file lives in, next to `path`.
/// `create_dir` (`mkdir`) refuses every existing entry, a symlink or another
/// run's directory included, so nothing already at a candidate name is
/// followed or reused: a taken name moves on to the next candidate. On Unix
/// the directory is created `0700`, so no other user can place an entry in
/// it. The name is neither `path` nor derived from it alone, so a file a
/// test or another process leaves at `path` is never inside the directory.
fn claim_private_scratch_dir(path: &std::path::Path) -> std::path::PathBuf {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    for attempt in 0..MAX_CLAIM_ATTEMPTS {
        let dir = parent.join(format!("{stem}-{}-{attempt}.d", std::process::id()));
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt as _;
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = std::fs::DirBuilder::new();
        match builder.create(&dir) {
            Ok(()) => return dir,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => panic!("the scratch directory is writable ({}): {e}", dir.display()),
        }
    }
    panic!("no free scratch directory near {}", path.display());
}

/// Creates the guard's file inside a fresh private directory and returns the
/// directory, the file's path and the open file. The file keeps `path`'s
/// name; `create_new` (`O_EXCL`) adds a second refusal of anything already
/// there, though the directory was created a moment earlier and is empty.
fn claim_exclusive(
    path: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf, std::fs::File) {
    let dir = claim_private_scratch_dir(path);
    let file_path = dir.join(path.file_name().unwrap_or_default());
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file_path)
    {
        Ok(file) => (dir, file_path, file),
        Err(e) => {
            let _ = std::fs::remove_dir(&dir);
            panic!(
                "the scratch directory is writable ({}): {e}",
                file_path.display()
            );
        }
    }
}

/// Writes `bytes` to a scratch file it claims inside a private directory it
/// creates, and removes that file and then that directory on drop, so a
/// failed assertion still cleans up.
pub(super) struct TempPackFile {
    pub(super) path: std::path::PathBuf,
    /// The private directory holding the file, removed after it.
    dir: std::path::PathBuf,
    identity: FileIdentity,
    /// Kept open until `Drop` has removed the file: it pins the inode the
    /// identity names.
    claim: Option<std::fs::File>,
}

impl TempPackFile {
    /// Creates a private directory next to `path` and writes `bytes` to a
    /// file named like `path` inside it; read the claimed file from `path`.
    /// Anything already at `path` is left alone.
    pub(super) fn write(path: &std::path::Path, bytes: &[u8]) -> Self {
        use std::io::Write as _;
        let (dir, path, mut file) = claim_exclusive(path);
        file.write_all(bytes).expect("the scratch pack is writable");
        #[cfg(unix)]
        let identity = FileIdentity::of(&file.metadata().expect("the scratch pack has metadata"));
        #[cfg(windows)]
        let identity = FileIdentity::of(&file).expect("the scratch pack has a file identity");
        #[cfg(not(any(unix, windows)))]
        let identity = FileIdentity;
        Self {
            path,
            dir,
            identity,
            claim: Some(file),
        }
    }
}

impl Drop for TempPackFile {
    /// Removes the file this guard created and then its private directory,
    /// and nothing else.
    ///
    /// The file sits in a directory this guard created with `create_dir`
    /// (`0700` on Unix) under a name nothing but this run knows, so a
    /// replacement raced onto the path from outside the test cannot land
    /// where cleanup looks. The directory is removed with `remove_dir`, which
    /// refuses a directory that still holds anything: an entry the guard did
    /// not create keeps the directory and is never deleted.
    ///
    /// On Windows the identity check and the delete go through one open
    /// handle (`win-fs`), so no second lookup of the path can be redirected.
    /// On Unix `std` has no delete bound to a verified object: the file is
    /// checked (`lstat`: still a regular file with the recorded device and
    /// inode) and then unlinked as two separate lookups. A process of the
    /// same user that finds the private directory and renames a different
    /// file into it inside that window still has that file removed; no other
    /// user can, and nothing outside this run has a reason to. That is the
    /// residual window. If `lstat` or the check fails the file stays, and
    /// `remove_dir` then fails closed on the non-empty directory.
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Ok(meta) = std::fs::symlink_metadata(&self.path) {
            if meta.is_file() && FileIdentity::of(&meta) == self.identity {
                let _ = std::fs::remove_file(&self.path);
            }
        }
        #[cfg(windows)]
        if let Ok(handle) = win_fs::open_for_verified_delete(&self.path) {
            let _ = win_fs::remove_through_open_handle(&handle, self.identity, |_| {});
        }
        // Close the pinned file before the directory goes, so a platform that
        // defers a delete until the last handle closes has finished with it.
        drop(self.claim.take());
        let _ = std::fs::remove_dir(&self.dir);
    }
}

pub(super) fn write_synthetic_palette_pack(entries: &[(&str, &[u16])]) -> TempPackFile {
    let out = pack_bytes(
        entries
            .iter()
            .map(|(id, colors)| pack_format::palette_entry((*id).into(), colors).unwrap())
            .collect(),
    );
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-title-test-palette-{}-{}.pack",
        std::process::id(),
        out.len()
    ));
    TempPackFile::write(&path, &out)
}

#[test]
fn synthetic_palette_pack_matches_the_pack_writer_serialization() {
    let fixture = write_synthetic_palette_pack(&[
        ("title/palette/z_second", &[GREEN_BGR555]),
        ("title/palette/a_first", &[RED_BGR555]),
    ]);
    let expected = pack_bytes(vec![
        pack_format::palette_entry("title/palette/z_second".into(), &[GREEN_BGR555]).unwrap(),
        pack_format::palette_entry("title/palette/a_first".into(), &[RED_BGR555]).unwrap(),
    ]);
    assert_eq!(std::fs::read(&fixture.path).unwrap(), expected);
}

#[test]
fn temp_pack_file_removes_the_scratch_file_while_a_panic_unwinds() {
    let observed = std::cell::RefCell::new(std::path::PathBuf::new());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fixture =
            write_synthetic_palette_pack(&[("title/palette/pokemon_logo", &[RED_BGR555])]);
        *observed.borrow_mut() = fixture.path.clone();
        assert!(
            fixture.path.exists(),
            "the scratch pack is written up front"
        );
        panic!("simulated assertion failure inside the test body");
    }));
    assert!(result.is_err(), "the simulated failure must unwind");
    let path = observed.borrow();
    assert!(
        !path.exists(),
        "the Drop guard must remove {} during unwinding",
        path.display()
    );
    let private_dir = path.parent().unwrap();
    assert!(
        !private_dir.exists(),
        "the Drop guard must remove {} during unwinding",
        private_dir.display()
    );
}

/// Creates a directory the tests own: `create_dir` fails on any existing
/// entry, so a taken name moves on to the next candidate.
fn claim_test_root(label: &str) -> std::path::PathBuf {
    for attempt in 0.. {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-{label}-{}-{attempt}",
            std::process::id()
        ));
        match std::fs::create_dir(&dir) {
            Ok(()) => return dir,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => panic!("the temp directory is writable: {e}"),
        }
    }
    unreachable!()
}

#[test]
fn scratch_pack_writer_does_not_unlink_an_unrelated_regular_file() {
    let dir = claim_test_root("title-occupant");
    let path = dir.join("scratch.pack");
    std::fs::write(&path, b"precious").unwrap();
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path.clone();
    let claimed_bytes = std::fs::read(&claimed).unwrap();
    let occupant = std::fs::read(&path).ok();
    drop(fixture);
    let occupant_after_drop = std::fs::read(&path).ok();
    let claimed_after_drop = claimed.exists();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_ne!(claimed, path, "the writer reused an occupied name");
    assert_eq!(claimed_bytes, [1, 2, 3]);
    assert_eq!(
        occupant.as_deref(),
        Some(&b"precious"[..]),
        "the writer unlinked an unrelated occupant it never created"
    );
    assert_eq!(
        occupant_after_drop.as_deref(),
        Some(&b"precious"[..]),
        "dropping the guard removed an unrelated occupant"
    );
    assert!(!claimed_after_drop, "the guard left its own file behind");
}

#[test]
fn scratch_pack_writer_skips_every_occupied_private_directory_name() {
    let dir = claim_test_root("title-skip");
    let path = dir.join("scratch.pack");
    let taken = |attempt: u32| dir.join(format!("scratch-{}-{attempt}.d", std::process::id()));
    for attempt in 0..2 {
        std::fs::create_dir(taken(attempt)).unwrap();
        std::fs::write(taken(attempt).join("scratch.pack"), b"theirs").unwrap();
    }
    let fixture = TempPackFile::write(&path, &[9]);
    let claimed = fixture.path.clone();
    let claimed_parent = claimed.parent().unwrap().to_path_buf();
    drop(fixture);
    let kept: Vec<_> = (0..2)
        .map(|attempt| std::fs::read(taken(attempt).join("scratch.pack")).unwrap())
        .collect();
    let claimed_dir_left = claimed_parent.exists();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(claimed_parent, taken(2));
    assert_eq!(kept, [b"theirs".to_vec(), b"theirs".to_vec()]);
    assert!(!claimed_dir_left, "the guard left its own directory behind");
}

#[test]
fn scratch_pack_guard_removes_its_directory_and_keeps_a_stranger_in_it() {
    let dir = claim_test_root("title-stranger");
    let path = dir.join("scratch.pack");
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path.clone();
    let private = claimed.parent().unwrap().to_path_buf();
    let stranger = private.join("stranger.bin");
    std::fs::write(&stranger, b"not ours").unwrap();
    drop(fixture);
    let claimed_after = claimed.exists();
    let stranger_after = std::fs::read(&stranger).ok();
    let private_after = private.exists();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(!claimed_after, "the guard left its own file behind");
    assert_eq!(
        stranger_after.as_deref(),
        Some(&b"not ours"[..]),
        "the guard removed an entry it did not create"
    );
    assert!(
        private_after,
        "remove_dir must refuse a non-empty directory"
    );
}

#[cfg(unix)]
#[test]
fn scratch_pack_directory_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = claim_test_root("title-mode");
    let fixture = TempPackFile::write(&dir.join("scratch.pack"), &[1]);
    let mode = std::fs::metadata(fixture.path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    drop(fixture);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(mode & 0o077, 0, "the private directory is open to others");
}

#[cfg(unix)]
#[test]
fn scratch_pack_guard_removes_only_the_file_it_created() {
    let dir = claim_test_root("title-identity");
    let path = dir.join("scratch.pack");
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path.clone();
    // The guard still holds its file open, so the freed name cannot hand the
    // replacement the guard's inode number.
    std::fs::remove_file(&claimed).unwrap();
    std::fs::write(&claimed, b"someone else's").unwrap();
    drop(fixture);
    let after = std::fs::read(&claimed).ok();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        after.as_deref(),
        Some(&b"someone else's"[..]),
        "the guard removed a file that replaced the one it created"
    );
}

#[cfg(unix)]
#[test]
fn scratch_pack_writer_does_not_follow_a_planted_symlink() {
    let dir = claim_test_root("title-symlink");
    let victim = dir.join("victim");
    std::fs::write(&victim, b"precious").unwrap();
    let path = dir.join("scratch.pack");
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    // A link planted at the first private-directory name must be skipped too.
    let planted_dir = dir.join(format!("scratch-{}-0.d", std::process::id()));
    let victim_dir = dir.join("victim-dir");
    std::fs::create_dir(&victim_dir).unwrap();
    std::os::unix::fs::symlink(&victim_dir, &planted_dir).unwrap();
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path.clone();
    drop(fixture);
    let victim_after = std::fs::read(&victim).unwrap();
    let victim_dir_entries = std::fs::read_dir(&victim_dir).unwrap().count();
    let link_survives = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    let dir_link_survives =
        std::fs::symlink_metadata(&planted_dir).is_ok_and(|m| m.file_type().is_symlink());
    std::fs::remove_dir_all(&dir).unwrap();
    assert_ne!(claimed, path, "the writer reused the planted link's name");
    assert_eq!(
        victim_after, b"precious",
        "the writer followed the symlink and clobbered its target"
    );
    assert_eq!(
        victim_dir_entries, 0,
        "the writer wrote through a planted directory link"
    );
    assert!(
        link_survives && dir_link_survives,
        "the writer or its guard unlinked a planted link"
    );
}
