//! Synthetic start-menu pack construction and scratch-file lifetime.

/// A unique scratch path for a synthetic pack fixture.
pub(super) fn synthetic_pack_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// A fixture file's identity, recorded when it is claimed so [`Drop`] removes
/// only the entry this guard created. On Unix that is device and inode. Other
/// platforms use length and creation time (a modification time can still change
/// when the writing handle closes); a file whose creation time the platform
/// cannot report has no identity, and [`Drop`] then leaves it in place rather
/// than risk unlinking an entry it cannot prove it owns.
#[derive(PartialEq, Eq)]
struct FileIdentity(u64, u64);

impl FileIdentity {
    // Unix always has an identity; other platforms may not.
    #[cfg_attr(unix, allow(clippy::unnecessary_wraps))]
    fn of(meta: &std::fs::Metadata) -> Option<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Some(Self(meta.dev(), meta.ino()))
        }
        #[cfg(not(unix))]
        {
            let created = meta
                .created()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?;
            Some(Self(meta.len(), u64::try_from(created.as_nanos()).ok()?))
        }
    }
}

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
/// file on drop, so a failed assertion still cleans up.
pub(super) struct TempPackFile {
    path: std::path::PathBuf,
    identity: Option<FileIdentity>,
}

impl TempPackFile {
    /// Claims the first free name at or after `path` (never an existing
    /// entry) and writes `bytes` to it; read the claimed name from `path()`.
    pub(super) fn write(path: &std::path::Path, bytes: &[u8]) -> Self {
        use std::io::Write as _;
        let (path, mut file) = claim_exclusive(path);
        file.write_all(bytes).expect("the scratch pack is writable");
        let identity = FileIdentity::of(&file.metadata().expect("the scratch pack has metadata"));
        Self { path, identity }
    }

    /// The file this guard claimed.
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempPackFile {
    fn drop(&mut self) {
        // Remove the entry only while it is still the regular file this guard
        // created; anything else now at the path belongs to someone else. The
        // identity check and the unlink are separate path lookups, a window
        // std cannot close (handle-bound delete is `win-fs`'s, off this
        // crate's dependency set); the path is a private candidate name this
        // guard created exclusively moments ago.
        if let Ok(meta) = std::fs::symlink_metadata(&self.path) {
            let owned = self.identity.is_some() && FileIdentity::of(&meta) == self.identity;
            if meta.is_file() && owned {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

/// The smallest pack [`StartMenuChrome::from_pack`] accepts, carrying two
/// distinguishable selectable standard frames -- frame 0 (green) and frame 5
/// (red) -- plus the fixed message box and the normal font sheet, so which
/// frame a menu drew is readable from a single border pixel.
pub(super) fn synthetic_start_menu_pack_bytes() -> Vec<u8> {
    use crate::pack_test_support::{image_entry, palette_entry, palette_entry_with_color};
    use rendering::Bgr555;

    const FRAME_SIDE: u32 = 24;
    const FRAME_BIT_DEPTH: u8 = 4;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    const FRAME_BORDER_PALETTE_INDEX: u8 = 1;
    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FONT_BIT_DEPTH: u8 = 2;

    crate::pack_test_support::pack_bytes(vec![
        image_entry(
            "text-window/image/1",
            FRAME_SIDE,
            FRAME_SIDE,
            FRAME_BIT_DEPTH,
            FRAME_BORDER_PALETTE_INDEX,
        ),
        palette_entry_with_color(
            "text-window/palette/1",
            PALETTE_COLOUR_COUNT,
            FRAME_BORDER_PALETTE_INDEX,
            Bgr555::from_channels(0, 31, 0),
        ),
        image_entry(
            "text-window/image/6",
            FRAME_SIDE,
            FRAME_SIDE,
            FRAME_BIT_DEPTH,
            FRAME_BORDER_PALETTE_INDEX,
        ),
        palette_entry_with_color(
            "text-window/palette/6",
            PALETTE_COLOUR_COUNT,
            FRAME_BORDER_PALETTE_INDEX,
            Bgr555::from_channels(31, 0, 0),
        ),
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ])
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
    let dir = claim_private_dir("start-occupant");
    let path = dir.join("scratch.pack");
    std::fs::write(&path, b"precious").unwrap();
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path().to_path_buf();
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
    let dir = claim_private_dir("start-skip");
    let path = dir.join("scratch.pack");
    std::fs::write(&path, b"zero").unwrap();
    std::fs::write(dir.join("scratch-1.pack"), b"one").unwrap();
    let fixture = TempPackFile::write(&path, &[9]);
    let claimed = fixture.path().to_path_buf();
    drop(fixture);
    let kept = (
        std::fs::read(&path).unwrap(),
        std::fs::read(dir.join("scratch-1.pack")).unwrap(),
    );
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(claimed.file_name().unwrap(), "scratch-2.pack");
    assert_eq!(kept, (b"zero".to_vec(), b"one".to_vec()));
}

#[test]
fn scratch_pack_guard_removes_only_the_file_it_created() {
    let dir = claim_private_dir("start-identity");
    let path = dir.join("scratch.pack");
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path().to_path_buf();
    // Keep the guard's file alive under another name so the replacement
    // below cannot be handed the same inode number.
    let kept = dir.join("kept");
    std::fs::hard_link(&claimed, &kept).unwrap();
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
    let dir = claim_private_dir("start-symlink");
    let victim = dir.join("victim");
    std::fs::write(&victim, b"precious").unwrap();
    let path = dir.join("scratch.pack");
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    let fixture = TempPackFile::write(&path, &[1, 2, 3]);
    let claimed = fixture.path().to_path_buf();
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
