//! Synthetic [`assets::pack::AssetPack`] fixtures shared by the crate's scene
//! test suites.

use pack_format::{PackEntry, PackWriter};
use rendering::Bgr555;

/// An image entry whose raster is `fill` at every pixel.
pub(crate) fn image_entry(
    id: &'static str,
    width: u32,
    height: u32,
    bit_depth: u8,
    fill: u8,
) -> PackEntry {
    let pixels = vec![fill; (width * height) as usize];
    pack_format::image_entry(id.into(), width, height, bit_depth, pixels).unwrap()
}

/// A palette entry of `color_count` colours, all transparent black.
pub(crate) fn palette_entry(id: &'static str, color_count: u16) -> PackEntry {
    let colors = vec![0u16; usize::from(color_count)];
    pack_format::palette_entry(id.into(), &colors).unwrap()
}

/// [`palette_entry`], with `color` at `index`.
pub(crate) fn palette_entry_with_color(
    id: &'static str,
    color_count: u16,
    index: u8,
    color: Bgr555,
) -> PackEntry {
    palette_entry_with_colors(id, color_count, &[(index, color)])
}

/// [`palette_entry`], with each `(index, color)` pair set.
pub(crate) fn palette_entry_with_colors(
    id: &'static str,
    color_count: u16,
    entries: &[(u8, Bgr555)],
) -> PackEntry {
    let mut colors = vec![0u16; usize::from(color_count)];
    for &(index, color) in entries {
        colors[usize::from(index)] = color.raw();
    }
    pack_format::palette_entry(id.into(), &colors).unwrap()
}

/// [`pack_bytes`], first adding a blank 2-bit glyph sheet for every
/// [`assets::fonts::FontId`] that `entries` leaves out, so a fixture that
/// customises one font still carries the five sheets every printer owner
/// loads.
pub(crate) fn pack_bytes_with_fonts(mut entries: Vec<PackEntry>) -> Vec<u8> {
    for font in assets::fonts::FontId::ALL {
        let id = format!("font/{}/glyphs", font.pack_name());
        if !entries.iter().any(|entry| entry.id == id) {
            let pixels =
                vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];
            entries.push(
                pack_format::image_entry(
                    id,
                    assets::fonts::SHEET_WIDTH,
                    assets::fonts::SHEET_HEIGHT,
                    2,
                    pixels,
                )
                .unwrap(),
            );
        }
    }
    pack_bytes(entries)
}

/// Asserts `printer` carries a source for every modelled [`assets::fonts::FontId`],
/// each reporting its own font.
pub(crate) fn assert_carries_every_font<S: assets::fonts::GlyphSource>(
    printer: &engine::text::render::Printer<S>,
) {
    for font in assets::fonts::FontId::ALL {
        let source = printer
            .source(font)
            .unwrap_or_else(|| panic!("the printer must carry the {font:?} glyph source"));
        assert_eq!(source.font(), font);
    }
}

pub(crate) fn pack_bytes(entries: Vec<PackEntry>) -> Vec<u8> {
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    writer.finish().unwrap()
}

/// Claims a fresh, writable file at `path` and returns its owned handle.
/// `create_new` (`O_EXCL`) never follows a symlink at `path`, so a planted
/// link cannot redirect a write onto its target. A stale occupant is
/// unlinked (the entry itself, never what it points to) and the claim
/// retried once. Install cleanup for `path` only after this returns, so a
/// failed claim never removes an entry the caller does not own.
pub(crate) fn claim_scratch_exclusive(path: &std::path::Path) -> std::fs::File {
    let mut file = None;
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(f) => {
                file = Some(f);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                std::fs::remove_file(path).expect("a stale scratch entry is removable");
            }
            Err(e) => panic!("the scratch directory is writable: {e}"),
        }
    }
    file.expect("the scratch path is exclusively creatable")
}

/// A scratch file claimed by [`claim_scratch_exclusive`] and removed on drop
/// only while its pathname still names the claimed file.
///
/// The guard holds the claim's open handle, so the file's identity (device
/// and inode on Unix) is pinned for the guard's whole life. Drop reads the
/// pathname's own metadata (`symlink_metadata`, which never follows a link)
/// and unlinks only an entry that is a regular file with the held handle's
/// identity. A file renamed away and replaced under the same name, or a
/// symlink planted there, is therefore left alone rather than deleted on the
/// strength of a name this guard no longer owns. Install the guard only after
/// the claim succeeded, which constructing it does, so a failed claim never
/// removes an entry the caller does not own.
///
/// Residual: the identity check and the `remove_file` that follows are two
/// pathname operations, so a swap landing between them can still unlink the
/// replacement. Closing that window needs a handle-bound delete, which is
/// the `rustix` Cargo-feature decision issue #2873 tracks; this crate's
/// default test leg stays std-only. On Windows `std` exposes no stable file
/// index, so the creation, write, and size triple of the held handle stands
/// in for the identity.
pub(crate) struct ScratchClaim {
    path: std::path::PathBuf,
    file: std::fs::File,
}

impl ScratchClaim {
    /// Exclusively claims `path` through [`claim_scratch_exclusive`].
    pub(crate) fn claim(path: std::path::PathBuf) -> Self {
        let file = claim_scratch_exclusive(&path);
        Self { path, file }
    }

    /// [`claim`](Self::claim), then writes `bytes` through the claimed handle.
    /// A write panic still unwinds through the installed guard and cleans up.
    pub(crate) fn claim_with(path: std::path::PathBuf, bytes: impl AsRef<[u8]>) -> Self {
        use std::io::Write as _;
        let mut claim = Self::claim(path);
        claim
            .file
            .write_all(bytes.as_ref())
            .expect("the scratch pack is writable");
        claim
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The handle the claim returned, for writing without reopening the
    /// shared pathname.
    pub(crate) fn file_mut(&mut self) -> &mut std::fs::File {
        &mut self.file
    }

    /// Whether `path` is still a regular file with the held handle's
    /// identity.
    fn pathname_is_still_ours(&self) -> bool {
        let Ok(found) = std::fs::symlink_metadata(&self.path) else {
            return false;
        };
        found.file_type().is_file()
            && self
                .file
                .metadata()
                .is_ok_and(|held| same_file_identity(&held, &found))
    }
}

impl Drop for ScratchClaim {
    fn drop(&mut self) {
        if self.pathname_is_still_ours() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn same_file_identity(held: &std::fs::Metadata, found: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    (held.dev(), held.ino()) == (found.dev(), found.ino())
}

#[cfg(windows)]
fn same_file_identity(held: &std::fs::Metadata, found: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    (
        held.creation_time(),
        held.last_write_time(),
        held.file_size(),
    ) == (
        found.creation_time(),
        found.last_write_time(),
        found.file_size(),
    )
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(_held: &std::fs::Metadata, _found: &std::fs::Metadata) -> bool {
    true
}

/// Writes `bytes` to a fresh file at `path` claimed by
/// [`claim_scratch_exclusive`].
pub(crate) fn write_scratch_exclusive(path: &std::path::Path, bytes: impl AsRef<[u8]>) {
    use std::io::Write as _;
    claim_scratch_exclusive(path)
        .write_all(bytes.as_ref())
        .expect("the scratch pack is writable");
}

/// Runs `write` once against a planted symlink and once against an
/// occupied regular file at `path`, asserting each pre-existing target
/// keeps its sentinel bytes and `path` ends as a regular file holding
/// `expected`. The occupied-file case hard-links a victim at `path`, so a
/// writer that truncates in place (`std::fs::write`) rewrites the victim's
/// shared inode, while one that unlinks the entry and re-creates it leaves
/// the victim alone. `write` returns whatever guard keeps its file alive;
/// the guard is dropped only after the check, so a writer that skips the
/// write fails. The symlink case is unix-only.
pub(crate) fn assert_scratch_claim_is_exclusive<G>(
    path: &std::path::Path,
    expected: &[u8],
    write: impl Fn() -> G,
) {
    let sentinel = b"victim must survive";
    let victim = |kind: &str| {
        let victim = std::env::temp_dir().join(format!(
            "pokeemerald-rs-claim-victim-{kind}-{}-{:?}-{}",
            std::process::id(),
            std::thread::current().id(),
            path.file_name().unwrap().to_string_lossy()
        ));
        write_scratch_exclusive(&victim, sentinel);
        let _ = std::fs::remove_file(path);
        victim
    };
    let check = |kind: &str, victim: &std::path::Path| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&write));
        let replaced = std::fs::symlink_metadata(path).map(|m| m.file_type().is_file());
        let written = std::fs::read(path);
        let completed = outcome.is_ok();
        drop(outcome);
        let after = std::fs::read(victim).unwrap();
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(victim);
        assert_eq!(after, sentinel, "the {kind} target was overwritten");
        assert!(completed, "the writer must replace the {kind}, not panic");
        assert!(
            matches!(replaced, Ok(true)),
            "the writer must leave a regular file where the {kind} was, got {replaced:?}"
        );
        assert_eq!(
            written.ok().as_deref(),
            Some(expected),
            "the writer must write its bytes to the replaced path"
        );
    };
    #[cfg(unix)]
    {
        let planted = victim("symlink");
        std::os::unix::fs::symlink(&planted, path).unwrap();
        check("planted symlink", &planted);
    }
    let occupied = victim("occupied");
    std::fs::hard_link(&occupied, path).unwrap();
    check("occupied file", &occupied);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-scratch-claim-{name}-{}.bin",
            std::process::id()
        ))
    }

    fn exists(path: &std::path::Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    #[test]
    fn dropping_a_claim_removes_the_file_it_owns() {
        let path = scratch_path("owned");
        let claim = ScratchClaim::claim_with(path.clone(), b"owned");
        assert!(exists(&path));
        drop(claim);
        assert!(!exists(&path), "the owned scratch file survived its guard");
    }

    #[test]
    fn dropping_a_claim_whose_file_was_already_removed_does_nothing() {
        let path = scratch_path("already-gone");
        let claim = ScratchClaim::claim(path.clone());
        std::fs::remove_file(&path).unwrap();
        drop(claim);
        assert!(!exists(&path));
    }

    #[test]
    fn dropping_a_claim_leaves_a_same_name_replacement_alone() {
        let path = scratch_path("replaced");
        let moved = scratch_path("replaced-moved-away");
        let _ = std::fs::remove_file(&moved);
        let claim = ScratchClaim::claim_with(path.clone(), b"original");
        std::fs::rename(&path, &moved).unwrap();
        write_scratch_exclusive(&path, b"replacement");

        drop(claim);

        let survivor = std::fs::read(&path);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&moved);
        assert_eq!(
            survivor.ok().as_deref(),
            Some(&b"replacement"[..]),
            "the guard unlinked a same-name file it did not create"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dropping_a_claim_never_follows_or_removes_a_planted_symlink() {
        let path = scratch_path("planted-symlink");
        let victim = scratch_path("planted-symlink-victim");
        write_scratch_exclusive(&victim, b"victim must survive");
        let claim = ScratchClaim::claim(path.clone());
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&victim, &path).unwrap();

        drop(claim);

        let link_survived = std::fs::symlink_metadata(&path)
            .map(|meta| meta.file_type().is_symlink())
            .unwrap_or(false);
        let victim_bytes = std::fs::read(&victim);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&victim);
        assert_eq!(
            victim_bytes.ok().as_deref(),
            Some(&b"victim must survive"[..]),
            "the guard reached the symlink's target"
        );
        assert!(
            link_survived,
            "the guard unlinked a symlink it did not plant"
        );
    }

    #[test]
    fn the_claimed_handle_writes_without_reopening_the_pathname() {
        use std::io::Write as _;
        let path = scratch_path("handle-write");
        let mut claim = ScratchClaim::claim(path.clone());
        claim.file_mut().write_all(b"through the handle").unwrap();
        let bytes = std::fs::read(claim.path());
        drop(claim);
        assert_eq!(bytes.unwrap(), b"through the handle");
    }
}
