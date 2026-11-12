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
/// loads, and a blank keypad-icon sheet when `entries` has none.
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
    if !entries.iter().any(|entry| entry.id == "font/keypad_icons") {
        let pixels = vec![
            0u8;
            (assets::keypad_icons::SHEET_WIDTH * assets::keypad_icons::SHEET_HEIGHT)
                as usize
        ];
        entries.push(
            pack_format::image_entry(
                "font/keypad_icons".into(),
                assets::keypad_icons::SHEET_WIDTH,
                assets::keypad_icons::SHEET_HEIGHT,
                4,
                pixels,
            )
            .unwrap(),
        );
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

/// Writes `bytes` to a fresh file at `path` claimed by
/// [`claim_scratch_exclusive`], and returns the creating handle so a caller
/// can keep owning the entry it made (see [`ScratchPack`]).
pub(crate) fn write_scratch_exclusive(
    path: &std::path::Path,
    bytes: impl AsRef<[u8]>,
) -> std::fs::File {
    use std::io::Write as _;
    let mut file = claim_scratch_exclusive(path);
    file.write_all(bytes.as_ref())
        .expect("the scratch pack is writable");
    file
}

/// `path` with the wall-clock nanoseconds of this call before its extension, so a
/// fixture that claims a scratch pack more than once on one thread gets a free name
/// each time even where an earlier guard could not verify (and so left) its file.
/// The callers' paths already carry the pid and thread id; the clock reading is read
/// per call, so no counter outlives the fixture. A reading that lands on an occupied
/// name is redrawn.
pub(crate) fn unique_scratch_path(path: &std::path::Path) -> std::path::PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    loop {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let name = match path.extension() {
            Some(extension) => format!("{stem}-{nanos}.{}", extension.to_string_lossy()),
            None => format!("{stem}-{nanos}"),
        };
        let candidate = path.with_file_name(name);
        if std::fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
}

/// A scratch pack a fixture created, owned through cleanup: it keeps the
/// creating handle, so the entry it made stays identifiable by `(dev, ino)`
/// for as long as the guard lives. Dropping it unlinks the pathname only
/// while the pathname still names that file; a replacement installed at the
/// name is left alone (and reported), so cleanup never removes an entry the
/// fixture does not own.
///
/// `PackSource::Test` needs a `'static` path: [`ScratchPack::leaked_path`]
/// leaks a copy of the pathname for it. Nothing is read through the leaked
/// path after the guard drops.
pub(crate) struct ScratchPack {
    path: std::path::PathBuf,
    /// Held open so the created inode cannot be recycled for a replacement
    /// while the guard compares identities.
    file: std::fs::File,
    /// Set by [`ScratchPack::with_identity_unavailable`].
    identity_unavailable: bool,
}

/// What [`ScratchPack::remove_if_owned`] did to the entry at the pathname.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScratchRemoval {
    /// The entry was still the file this guard owns, and was unlinked.
    Removed,
    /// Nothing is at the pathname any more.
    Gone,
    /// A different entry now sits at the pathname; it was left alone.
    Replaced,
    /// This platform offers no identity to compare, so nothing was unlinked.
    Unverifiable,
}

impl ScratchPack {
    /// Takes ownership of the entry the handle `file` created at `path`.
    /// `file` must come from the exclusive creation of `path` (for example
    /// [`write_scratch_exclusive`]'s return value), never from an open of a
    /// pre-existing entry.
    pub(crate) fn adopt(path: &std::path::Path, file: std::fs::File) -> Self {
        Self {
            path: path.to_path_buf(),
            file,
            identity_unavailable: false,
        }
    }

    /// [`write_scratch_exclusive`], owned.
    pub(crate) fn write(path: &std::path::Path, bytes: impl AsRef<[u8]>) -> Self {
        Self::adopt(path, write_scratch_exclusive(path, bytes))
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// A `'static` copy of the pathname, leaked once per call and reclaimed
    /// only when the test process exits, for `PackSource::Test`.
    pub(crate) fn leaked_path(&self) -> &'static std::path::Path {
        Box::leak(self.path().to_path_buf().into_boxed_path())
    }

    /// Unlinks the pathname only if it still names the file this guard
    /// created. Unix compares `(dev, ino)` of the path (`symlink_metadata`
    /// does not follow a link, so a planted symlink never matches) against the
    /// creating handle; std has no unlink-by-handle, so a swap between that
    /// check and the unlink remains possible, and this closes the window the
    /// fixture's run opens, not that one. Windows reads the handle's
    /// [`win_fs::WindowsFileIdentity`] and deletes through a handle opened on
    /// the path only if that handle has the same identity. Elsewhere there is
    /// no identity to compare, so nothing is unlinked.
    pub(crate) fn remove_if_owned(&self) -> ScratchRemoval {
        if self.identity_unavailable {
            return ScratchRemoval::Unverifiable;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;

            let current = match std::fs::symlink_metadata(&self.path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return ScratchRemoval::Gone;
                }
                Err(_) => return ScratchRemoval::Replaced,
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
                Err(_) => ScratchRemoval::Replaced,
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

    /// Test seam: makes this guard behave as on a platform whose file identity
    /// cannot be read, so it leaves its file in place when dropped.
    #[cfg(test)]
    pub(crate) fn with_identity_unavailable(mut self) -> Self {
        self.identity_unavailable = true;
        self
    }
}

impl Drop for ScratchPack {
    fn drop(&mut self) {
        match self.remove_if_owned() {
            ScratchRemoval::Removed | ScratchRemoval::Gone => {}
            ScratchRemoval::Replaced => eprintln!(
                "the scratch pack {} was replaced after its fixture created it; leaving the \
                 replacement in place",
                self.path.display()
            ),
            ScratchRemoval::Unverifiable => eprintln!(
                "cannot verify the scratch pack {} is still the file its fixture created; \
                 leaving it in place",
                self.path.display()
            ),
        }
    }
}

/// Moves the entry at `path` to the free name `aside`. `hard_link` fails on an
/// occupied destination where `rename` would silently overwrite it, so a
/// pre-existing `aside` entry is never destroyed.
#[cfg(unix)]
fn move_aside(path: &std::path::Path, aside: &std::path::Path) {
    std::fs::hard_link(path, aside).expect("the aside name is free");
    std::fs::remove_file(path).expect("the owned pack is unlinkable");
}

/// Moves the entry `pack` owns aside, installs a different file at its
/// pathname, drops `pack`, and asserts the replacement survived the
/// guard's cleanup untouched. Removes the replacement and the moved-aside
/// original afterwards.
#[cfg(unix)]
pub(crate) fn assert_scratch_pack_cleanup_spares_a_replacement(pack: ScratchPack) {
    const REPLACEMENT: &[u8] = b"replacement the fixture never created";
    let path = pack.path().to_path_buf();
    let aside = path.with_extension("aside");
    move_aside(&path, &aside);
    write_scratch_exclusive(&path, REPLACEMENT);

    drop(pack);

    let survived = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&aside);
    assert_eq!(
        survived.ok().as_deref(),
        Some(REPLACEMENT),
        "cleanup unlinked a file the fixture did not create"
    );
}

#[test]
fn a_scratch_pack_guard_unlinks_the_file_it_created_when_dropped() {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-scratch-pack-owned-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let pack = ScratchPack::write(&path, b"owned");
    assert!(path.is_file());
    drop(pack);
    assert!(!path.exists(), "the guard must unlink what it created");
}

#[cfg(unix)]
#[test]
fn a_scratch_pack_guard_spares_a_replacement_and_a_symlink_at_its_path() {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-scratch-pack-replaced-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    assert_scratch_pack_cleanup_spares_a_replacement(ScratchPack::write(&path, b"owned"));

    let target = path.with_extension("target");
    let pack = ScratchPack::write(&path, b"owned");
    let aside = path.with_extension("aside");
    move_aside(&path, &aside);
    write_scratch_exclusive(&target, b"link target");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert_eq!(pack.remove_if_owned(), ScratchRemoval::Replaced);
    let link_kept = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    drop(pack);
    let link_kept_after_drop =
        std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&aside);
    assert!(
        link_kept && link_kept_after_drop,
        "the planted link survives"
    );
}

/// The Windows arm of [`ScratchPack::remove_if_owned`] compares the creating handle's
/// file identity with the entry at the path through `win-fs`; moving the owned pack
/// aside and planting a different file at its path must leave that file alone.
#[cfg(windows)]
#[test]
fn a_scratch_pack_guard_spares_a_windows_replacement_at_its_path() {
    const REPLACEMENT: &[u8] = b"replacement the fixture never created";
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-scratch-pack-windows-replaced-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let aside = path.with_extension("aside");
    let pack = ScratchPack::write(&path, b"owned");
    std::fs::rename(&path, &aside).expect("the owned pack moves aside");
    write_scratch_exclusive(&path, REPLACEMENT);

    let outcome = pack.remove_if_owned();
    drop(pack);

    let survived = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&aside);
    assert_eq!(outcome, ScratchRemoval::Replaced);
    assert_eq!(
        survived.ok().as_deref(),
        Some(REPLACEMENT),
        "cleanup unlinked a file the fixture did not create"
    );
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

/// Where a guard cannot read its file's identity it leaves the file rather
/// than risk a replacement; a fixture that claims its path again must still
/// get a free one, so the claim does not depend on the earlier cleanup.
#[test]
fn a_guard_without_a_file_identity_leaves_its_file_and_the_next_claim_still_succeeds() {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-scratch-pack-unverifiable-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let pack = ScratchPack::write(&path, b"owned").with_identity_unavailable();
    assert_eq!(pack.remove_if_owned(), ScratchRemoval::Unverifiable);
    drop(pack);
    assert!(path.is_file(), "an unverifiable file is left in place");

    let next = unique_scratch_path(&path);
    let again = ScratchPack::write(&next, b"owned again");
    assert!(next.is_file());
    drop(again);
    let _ = std::fs::remove_file(&path);
    assert!(!next.exists());
}
