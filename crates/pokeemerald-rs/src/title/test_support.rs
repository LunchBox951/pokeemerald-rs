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

/// How many candidate names a claim tries before giving up.
const MAX_CLAIM_ATTEMPTS: u32 = 1024;

/// Candidate `attempt` for `path`: attempt 0 is `path` itself, later attempts
/// insert `-{attempt}` before the extension.
fn candidate_path(path: &std::path::Path, attempt: u32) -> std::path::PathBuf {
    if attempt == 0 {
        return path.to_path_buf();
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let mut name = format!("{stem}-{attempt}");
    if let Some(ext) = path.extension() {
        name.push('.');
        name.push_str(&ext.to_string_lossy());
    }
    path.with_file_name(name)
}

/// Creates a new file at the first free candidate for `path`. `create_new`
/// (`O_EXCL`) refuses every existing entry, a symlink included, so nothing
/// already at a candidate is followed, truncated or unlinked: a taken name
/// moves on to the next candidate.
fn claim_exclusive(path: &std::path::Path) -> (std::path::PathBuf, std::fs::File) {
    for attempt in 0..MAX_CLAIM_ATTEMPTS {
        let candidate = candidate_path(path, attempt);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return (candidate, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => panic!(
                "the scratch directory is writable ({}): {e}",
                candidate.display()
            ),
        }
    }
    panic!("no free scratch name near {}", path.display());
}

/// Writes `bytes` to a scratch file it claims exclusively and removes that
/// file on drop, so a failed assertion or panic still cleans up.
pub(super) struct TempPackFile {
    pub(super) path: std::path::PathBuf,
    identity: FileIdentity,
    /// Kept open through `Drop`: it pins the inode the identity names.
    _claim: std::fs::File,
}

impl TempPackFile {
    /// Claims the first free name at or after `path` (never an existing
    /// entry) and writes `bytes` to it; read the claimed name from `path`.
    pub(super) fn write(path: &std::path::Path, bytes: &[u8]) -> Self {
        use std::io::Write as _;
        let (path, mut file) = claim_exclusive(path);
        file.write_all(bytes).expect("the scratch pack is writable");
        #[cfg(unix)]
        let identity = FileIdentity::of(&file.metadata().expect("the scratch pack has metadata"));
        #[cfg(windows)]
        let identity = FileIdentity::of(&file).expect("the scratch pack has a file identity");
        #[cfg(not(any(unix, windows)))]
        let identity = FileIdentity;
        Self {
            path,
            identity,
            _claim: file,
        }
    }
}

impl Drop for TempPackFile {
    /// Removes the file this guard created, and nothing else.
    ///
    /// On Windows the identity check and the delete go through one open
    /// handle (`win-fs`), so no second lookup of the path can be redirected.
    /// On Unix `std` has no delete bound to a verified object: the path is
    /// checked (`lstat`: still a regular file with the recorded device and
    /// inode) and then unlinked as two separate lookups, so another process
    /// that renames a different file over the path inside that window has
    /// its file removed. The path is a private candidate name this guard
    /// created exclusively moments earlier, which makes the window a
    /// current limitation rather than something a test run reaches.
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
}

/// Creates a directory no one else owns: `create_dir` fails on any existing
/// entry, so a taken name moves on to the next candidate.
fn claim_private_dir(label: &str) -> std::path::PathBuf {
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
    let dir = claim_private_dir("title-occupant");
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
fn scratch_pack_writer_skips_every_occupied_candidate_name() {
    let dir = claim_private_dir("title-skip");
    let path = dir.join("scratch.pack");
    std::fs::write(&path, b"zero").unwrap();
    std::fs::write(dir.join("scratch-1.pack"), b"one").unwrap();
    let fixture = TempPackFile::write(&path, &[9]);
    let claimed = fixture.path.clone();
    drop(fixture);
    let kept = (
        std::fs::read(&path).unwrap(),
        std::fs::read(dir.join("scratch-1.pack")).unwrap(),
    );
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(claimed.file_name().unwrap(), "scratch-2.pack");
    assert_eq!(kept, (b"zero".to_vec(), b"one".to_vec()));
}

#[cfg(unix)]
#[test]
fn scratch_pack_guard_removes_only_the_file_it_created() {
    let dir = claim_private_dir("title-identity");
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
    let dir = claim_private_dir("title-symlink");
    let victim = dir.join("victim");
    std::fs::write(&victim, b"precious").unwrap();
    let path = dir.join("scratch.pack");
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path.clone();
    drop(fixture);
    let victim_after = std::fs::read(&victim).unwrap();
    let link_survives = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    std::fs::remove_dir_all(&dir).unwrap();
    assert_ne!(claimed, path, "the writer reused the planted link's name");
    assert_eq!(
        victim_after, b"precious",
        "the writer followed the symlink and clobbered its target"
    );
    assert!(
        link_survives,
        "the writer or its guard unlinked the planted link"
    );
}
