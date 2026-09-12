//! Synthetic [`assets::pack::AssetPack`] fixtures shared by the scene test
//! suites ([`crate::main_menu::tests`], [`crate::intro::tests`]): entries
//! are built as typed [`pack_format::PackEntry`] values and serialized
//! through the real [`pack_format::PackWriter`], so no test hand-rolls the
//! pack's byte layout (issue #952).

use pack_format::{EntryKind, PackEntry, PackWriter};
use rendering::Bgr555;

/// An [`EntryKind::Image`] entry whose payload is one repeated palette
/// index (`fill`) across every pixel.
pub(crate) fn image_entry(
    id: &'static str,
    width: u32,
    height: u32,
    bit_depth: u8,
    fill: u8,
) -> PackEntry {
    PackEntry {
        id: id.into(),
        kind: EntryKind::Image {
            width,
            height,
            bit_depth,
        },
        payload: vec![fill; (width * height) as usize],
    }
}

/// An [`EntryKind::Palette`] entry of `color_count` packed BGR555 colours,
/// all zero (transparent black).
pub(crate) fn palette_entry(id: &'static str, color_count: u16) -> PackEntry {
    PackEntry {
        id: id.into(),
        kind: EntryKind::Palette { color_count },
        payload: vec![0u8; usize::from(color_count) * size_of::<u16>()],
    }
}

/// [`palette_entry`], with `color` written at `index` rather than left zeroed.
pub(crate) fn palette_entry_with_color(
    id: &'static str,
    color_count: u16,
    index: u8,
    color: Bgr555,
) -> PackEntry {
    let mut entry = palette_entry(id, color_count);
    let offset = usize::from(index) * size_of::<u16>();
    entry.payload[offset..offset + size_of::<u16>()].copy_from_slice(&color.raw().to_le_bytes());
    entry
}

/// Serialize `entries` into pack bytes through the real
/// [`PackWriter`] -- the one owner of the format's byte layout, so a format
/// change cannot leave a stale hand-rolled test encoder behind it.
pub(crate) fn pack_bytes(entries: Vec<PackEntry>) -> Vec<u8> {
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    writer.finish().unwrap()
}
