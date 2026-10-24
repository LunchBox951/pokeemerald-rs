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
