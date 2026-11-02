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
/// link cannot redirect a write onto its target. An occupied `path` fails
/// the claim with a panic naming it: `AlreadyExists` does not show the
/// occupant is the caller's, so it is never unlinked. Install cleanup for
/// `path` only after this returns, so a failed claim never removes an entry
/// the caller does not own.
pub(crate) fn claim_scratch_exclusive(path: &std::path::Path) -> std::fs::File {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => panic!(
            "the scratch path {} is already occupied by an entry this fixture did not create; \
             refusing to remove it",
            path.display()
        ),
        Err(e) => panic!("the scratch directory is writable: {e}"),
    }
}

/// A scratch file claimed by [`claim_scratch_exclusive`] and removed on drop
/// only while its pathname still names the claimed file.
///
/// The guard holds the claim's open handle, so the file's identity is pinned
/// for the guard's whole life.
///
/// On Unix, drop reads the pathname's own metadata (`symlink_metadata`,
/// which never follows a link) and unlinks only an entry that is a regular
/// file with the held handle's device and inode. A file renamed away and
/// replaced under the same name, or a symlink planted there, is left alone
/// rather than deleted on the strength of a name this guard no longer owns.
/// Residual: the identity check and the `remove_file` that follows are two
/// pathname operations, so a swap landing between them can still unlink the
/// replacement. Closing that window needs a handle-bound delete, which is
/// the `rustix` Cargo-feature decision issue #2873 tracks; this crate's
/// default test leg stays std-only here.
///
/// On Windows the claim records the held handle's `win_fs::WindowsFileIdentity`
/// (volume serial and file ID) and drop deletes through
/// `win_fs::remove_through_verified_handle`, which opens the pathname without
/// following a reparse point, compares the file ID, and deletes through that
/// same handle, so no second lookup of the path remains. Timestamps and size
/// are never used: a replacement can copy both. If the identity cannot be
/// read, the file is left in the temp directory rather than risk removing a
/// replacement.
///
/// Install the guard only after the claim succeeded, which constructing it
/// does, so a failed claim never removes an entry the caller does not own.
pub(crate) struct ScratchClaim {
    path: std::path::PathBuf,
    file: std::fs::File,
    /// The claimed file's identity; `None` when it could not be read, which
    /// leaves the file in place at drop.
    #[cfg(windows)]
    identity: Option<win_fs::WindowsFileIdentity>,
}

impl ScratchClaim {
    /// Exclusively claims `path` through [`claim_scratch_exclusive`].
    pub(crate) fn claim(path: std::path::PathBuf) -> Self {
        let file = claim_scratch_exclusive(&path);
        Self {
            #[cfg(windows)]
            identity: win_fs::WindowsFileIdentity::of(&file).ok(),
            path,
            file,
        }
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
}

#[cfg(not(windows))]
impl ScratchClaim {
    /// Whether `path` is still a regular file with the held handle's
    /// identity. Off Unix there is no identity to compare, so nothing is
    /// removed.
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
        #[cfg(not(windows))]
        if self.pathname_is_still_ours() {
            let _ = std::fs::remove_file(&self.path);
        }
        #[cfg(windows)]
        if let Some(identity) = self.identity {
            let _ = win_fs::remove_through_verified_handle(&self.path, identity);
        }
    }
}

#[cfg(unix)]
fn same_file_identity(held: &std::fs::Metadata, found: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    (held.dev(), held.ino()) == (found.dev(), found.ino())
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(_held: &std::fs::Metadata, _found: &std::fs::Metadata) -> bool {
    false
}

/// Writes `bytes` to a fresh file at `path` claimed by
/// [`claim_scratch_exclusive`].
pub(crate) fn write_scratch_exclusive(path: &std::path::Path, bytes: impl AsRef<[u8]>) {
    use std::io::Write as _;
    claim_scratch_exclusive(path)
        .write_all(bytes.as_ref())
        .expect("the scratch pack is writable");
}

/// Whether `a` and `b` resolve to one inode (a hard link pair, or a link and
/// its target). Always true off
/// Unix, where identity cannot be compared with std.
fn same_inode(a: &std::path::Path, b: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        true
    }
}

/// Runs `write` against a planted symlink and against an occupied regular
/// file at `path`, asserting that `write` refuses (panics) without touching
/// the occupant: the entry at `path` is still the planted symlink or
/// hard-linked file and the victim keeps its sentinel bytes. It then removes
/// that entry and runs `write` again on the free `path`, asserting it leaves
/// a regular file holding `expected` and still never touches the victim.
/// The occupied-file case hard-links a victim at `path`, so a writer that
/// truncates in place (`std::fs::write`) rewrites the victim's shared inode.
/// `write` returns whatever guard keeps its file alive; the guard is dropped
/// only after the check, so a writer that skips the write fails. The symlink
/// case is unix-only. [`claim_scratch_exclusive`] on a lone single-link
/// occupant is covered by its own test.
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
    let check = |kind: &str, victim: &std::path::Path, planted_link: bool| {
        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&write));
        let still_planted = std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.file_type().is_symlink() == planted_link && !m.is_dir());
        let occupant = std::fs::read(path);
        let same_entry = same_inode(path, victim);
        let victim_after_refusal = std::fs::read(victim);
        let refused_ok = refused.is_err();
        drop(refused);

        let _ = std::fs::remove_file(path);
        let freed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&write));
        let replaced = std::fs::symlink_metadata(path).map(|m| m.file_type().is_file());
        let written = std::fs::read(path);
        let completed = freed.is_ok();
        drop(freed);
        let victim_after_write = std::fs::read(victim);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(victim);

        assert!(refused_ok, "the writer must refuse the {kind}, not take it");
        assert!(still_planted, "the writer replaced the {kind}");
        assert!(
            same_entry,
            "the writer unlinked the {kind} and recreated a lookalike"
        );
        assert_eq!(
            occupant.ok().as_deref(),
            Some(&sentinel[..]),
            "the writer changed what the {kind} held"
        );
        assert_eq!(
            victim_after_refusal.ok().as_deref(),
            Some(&sentinel[..]),
            "the {kind} target was overwritten"
        );
        assert!(completed, "the writer must claim the freed path, not panic");
        assert!(
            matches!(replaced, Ok(true)),
            "the writer must leave a regular file at the freed {kind} path, got {replaced:?}"
        );
        assert_eq!(
            written.ok().as_deref(),
            Some(expected),
            "the writer must write its bytes to the freed path"
        );
        assert_eq!(
            victim_after_write.ok().as_deref(),
            Some(&sentinel[..]),
            "the {kind} target was overwritten by the freed-path write"
        );
    };
    #[cfg(unix)]
    {
        let planted = victim("symlink");
        std::os::unix::fs::symlink(&planted, path).unwrap();
        check("planted symlink", &planted, true);
    }
    let occupied = victim("occupied");
    std::fs::hard_link(&occupied, path).unwrap();
    check("occupied file", &occupied, false);
}

/// The marker colours of [`synthetic_title_entries`]: the backdrop (and the
/// pixels outside the logo layer) are [`TITLE_BACKDROP`], the logo layer is
/// [`TITLE_LOGO`], and the copyright sprite is [`TITLE_SPRITE`].
pub(crate) const TITLE_BACKDROP: (u8, u8, u8) = (0, 0, 31);
/// See [`TITLE_BACKDROP`].
pub(crate) const TITLE_LOGO: (u8, u8, u8) = (31, 0, 0);
/// See [`TITLE_BACKDROP`].
pub(crate) const TITLE_SPRITE: (u8, u8, u8) = (0, 31, 0);

/// Every entry [`crate::title::TitleScene::from_pack`] reads, small enough to
/// build in a test but with one distinguishable BG colour (the logo layer
/// over a backdrop) and one OBJ colour (the always-visible copyright
/// banner), so a palette fade's independent BG and OBJ halves show up
/// separately in the composed frame.
pub(crate) fn synthetic_title_entries() -> Vec<PackEntry> {
    let colour = |(r, g, b): (u8, u8, u8)| Bgr555::from_channels(r, g, b);
    let raw = |id: &'static str, len: usize| pack_format::raw_entry(id.into(), vec![0u8; len]);
    vec![
        image_entry("title/image/rayquaza", 8, 8, 4, 0),
        image_entry("title/image/clouds", 8, 8, 4, 0),
        image_entry("title/image/pokemon_logo", 8, 8, 8, 1),
        palette_entry_with_colors(
            "title/palette/pokemon_logo",
            16,
            &[(0, colour(TITLE_BACKDROP)), (1, colour(TITLE_LOGO))],
        ),
        palette_entry("title/palette/rayquaza_and_clouds", 16),
        raw("title/raw/rayquaza", 2 * 32 * 32),
        raw("title/raw/clouds", 2 * 32 * 32),
        raw("title/raw/pokemon_logo", 32 * 32),
        image_entry("title/image/emerald_version", 128, 32, 8, 0),
        image_entry("title/image/press_start", 160, 24, 8, 1),
        palette_entry("title/palette/emerald_version", 16),
        palette_entry_with_color("title/palette/press_start", 16, 1, colour(TITLE_SPRITE)),
    ]
}

#[test]
fn claim_scratch_exclusive_leaves_a_foreign_single_link_occupant_intact() {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-claim-foreign-occupant-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let sentinel = b"not the fixture's file";
    write_scratch_exclusive(&path, sentinel);
    #[cfg(unix)]
    let planted_inode = {
        use std::os::unix::fs::MetadataExt as _;
        let meta = std::fs::metadata(&path).unwrap();
        (meta.dev(), meta.ino())
    };
    let outcome = std::panic::catch_unwind(|| drop(claim_scratch_exclusive(&path)));
    let after = std::fs::read(&path);
    #[cfg(unix)]
    let kept_inode = {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::metadata(&path).is_ok_and(|meta| (meta.dev(), meta.ino()) == planted_inode)
    };
    let _ = std::fs::remove_file(&path);
    #[cfg(unix)]
    assert!(kept_inode, "the claim unlinked the occupant it never owned");
    assert_eq!(
        after.ok().as_deref(),
        Some(&sentinel[..]),
        "claim_scratch_exclusive destroyed a pre-existing file it never owned"
    );
    let message = outcome
        .as_ref()
        .err()
        .and_then(|payload| payload.downcast_ref::<String>());
    assert!(
        message.is_some_and(|m| m.contains(&*path.to_string_lossy())),
        "the claim must fail naming the occupied path, got {message:?}"
    );
}
