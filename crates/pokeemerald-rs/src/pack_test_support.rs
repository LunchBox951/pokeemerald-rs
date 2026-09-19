//! Synthetic [`assets::pack::AssetPack`] fixtures shared by the scene test
//! suites [`crate::main_menu::tests`], [`crate::intro::tests`], and
//! [`crate::start_menu::tests`].

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
    let mut colors = vec![0u16; usize::from(color_count)];
    colors[usize::from(index)] = color.raw();
    pack_format::palette_entry(id.into(), &colors).unwrap()
}

pub(crate) fn pack_bytes(entries: Vec<PackEntry>) -> Vec<u8> {
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    writer.finish().unwrap()
}
