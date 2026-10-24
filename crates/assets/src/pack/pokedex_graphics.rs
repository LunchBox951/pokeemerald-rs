//! Typed Pokédex screen graphics (upstream `graphics/pokedex/`).
//!
//! Pack ids stay private; [`AssetPack::pokedex_graphics`] validates every entry
//! on each call.

use super::{AssetPack, PackError, PaletteRef, PalettedImage};

const IMAGE_PREFIX: &str = "pokedex-graphics/image";
const IMAGE_PALETTE_PREFIX: &str = "pokedex-graphics/image-palette";
const FILE_PALETTE_PREFIX: &str = "pokedex-graphics/palette";
const RAW_PREFIX: &str = "pokedex-graphics/raw";

const TEXT_TILEMAP_ENTRY_BYTES: usize = std::mem::size_of::<u16>();
const AFFINE_TILEMAP_ENTRY_BYTES: usize = std::mem::size_of::<u8>();
const PALETTE_ENTRY_BYTES: usize = std::mem::size_of::<u16>();
const COLORS_PER_4BPP_BANK: u16 = 1 << 4;
const FULL_PALETTE_COLORS: u16 = 1 << 8;
const REGION_MAP_AFFINE_TILEMAP_SIDE_ENTRIES: usize = 64;
const REGION_MAP_AFFINE_TILEMAP_BYTES: usize = REGION_MAP_AFFINE_TILEMAP_SIDE_ENTRIES
    * REGION_MAP_AFFINE_TILEMAP_SIDE_ENTRIES
    * AFFINE_TILEMAP_ENTRY_BYTES;
const CRY_SCREEN_BG_RAW_BYTES: usize = 32;

const fn text_tilemap_bytes(width_tiles: usize, height_tiles: usize) -> usize {
    width_tiles * height_tiles * TEXT_TILEMAP_ENTRY_BYTES
}

/// Every Pokédex graphic the pack carries, each validated against its upstream shape.
///
/// A sheet's palette is the one embedded in its own PNG; the `*_palette`
/// fields are separate standalone palettes.
#[derive(Debug, Clone, Copy)]
pub struct PokedexGraphicsHandle<'a> {
    /// 128x128, 4-bit, 16 colours.
    pub menu: PalettedImage<'a>,
    /// 32x512, 4-bit, 16 colours.
    pub interface: PalettedImage<'a>,
    /// 8x32, 4-bit, 16 colours.
    pub noball_unused: PalettedImage<'a>,
    /// 8x16, 4-bit, 16 colours.
    pub caught_ball: PalettedImage<'a>,
    /// 8x96, 4-bit, 16 colours.
    pub arrows: PalettedImage<'a>,
    /// 128x64, 4-bit, 16 colours.
    pub search_menu: PalettedImage<'a>,
    /// 32x96, 4-bit, 16 colours.
    pub area_glow: PalettedImage<'a>,
    /// 16x16, 4-bit, 16 colours.
    pub area_marker: PalettedImage<'a>,
    /// 32x96, 4-bit, 16 colours.
    pub area_unknown: PalettedImage<'a>,
    /// 128x120, 8-bit, 256 colours.
    pub region_map: PalettedImage<'a>,
    /// 128x120, 8-bit, 256 colours.
    pub region_map_affine: PalettedImage<'a>,
    /// 80x64, 4-bit, 16 colours.
    pub cry_meter: PalettedImage<'a>,
    /// 64x64, 4-bit, 16 colours.
    pub cry_meter_needle: PalettedImage<'a>,
    /// 8x8, 4-bit, 16 colours.
    pub cry_screen_bg: PalettedImage<'a>,
    /// 96 colours.
    pub bg_hoenn_palette: PaletteRef<'a>,
    /// 96 colours.
    pub bg_national_palette: PaletteRef<'a>,
    /// 96 colours.
    pub search_results_palette: PaletteRef<'a>,
    /// 64 colours.
    pub search_menu_palette: PaletteRef<'a>,
    /// 48 colours.
    pub region_map_palette: PaletteRef<'a>,
    /// 16 colours.
    pub size_silhouette_palette: PaletteRef<'a>,
    /// 32x12 text tilemap, 768 bytes.
    pub start_menu_main_tilemap: &'a [u8],
    /// 32x12 text tilemap, 768 bytes.
    pub start_menu_search_results_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub list_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub list_underlay_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub info_screen_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub size_screen_tilemap: &'a [u8],
    /// 32x3 text tilemap, 192 bytes.
    pub screen_select_bar_main_tilemap: &'a [u8],
    /// 32x3 text tilemap, 192 bytes.
    pub screen_select_bar_submenu_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub search_menu_hoenn_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub search_menu_national_tilemap: &'a [u8],
    /// 32x32 text tilemap, 2048 bytes.
    pub region_map_tilemap: &'a [u8],
    /// 64x64 affine tilemap of one-byte entries, 4096 bytes.
    pub region_map_affine_tilemap: &'a [u8],
    /// 32x20 text tilemap, 1280 bytes.
    pub cry_screen_tilemap: &'a [u8],
    /// 10x8 text tilemap, 160 bytes.
    pub cry_meter_tilemap: &'a [u8],
    /// Opaque extensionless `cry_screen_bg` file, 32 bytes.
    pub cry_screen_bg_raw: &'a [u8],
}

impl AssetPack {
    /// Every Pokédex graphic, palette, and tilemap, validated against the
    /// dimensions, bit depth, palette cardinality, and byte lengths upstream gives them.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedPokedexGraphic`] when an image's
    /// bit depth, dimensions, payload length, or pixel indices, a palette's
    /// colour count or payload length, or a raw entry's length differs from that shape.
    pub fn pokedex_graphics(&self) -> Result<PokedexGraphicsHandle<'_>, PackError> {
        Ok(PokedexGraphicsHandle {
            menu: self.pokedex_sheet("menu", (128, 128), COLORS_PER_4BPP_BANK)?,
            interface: self.pokedex_sheet("interface", (32, 512), COLORS_PER_4BPP_BANK)?,
            noball_unused: self.pokedex_sheet("noball_unused", (8, 32), COLORS_PER_4BPP_BANK)?,
            caught_ball: self.pokedex_sheet("caught_ball", (8, 16), COLORS_PER_4BPP_BANK)?,
            arrows: self.pokedex_sheet("arrows", (8, 96), COLORS_PER_4BPP_BANK)?,
            search_menu: self.pokedex_sheet("search_menu", (128, 64), COLORS_PER_4BPP_BANK)?,
            area_glow: self.pokedex_sheet("area_glow", (32, 96), COLORS_PER_4BPP_BANK)?,
            area_marker: self.pokedex_sheet("area_marker", (16, 16), COLORS_PER_4BPP_BANK)?,
            area_unknown: self.pokedex_sheet("area_unknown", (32, 96), COLORS_PER_4BPP_BANK)?,
            region_map: self.pokedex_sheet("region_map", (128, 120), FULL_PALETTE_COLORS)?,
            region_map_affine: self.pokedex_sheet(
                "region_map_affine",
                (128, 120),
                FULL_PALETTE_COLORS,
            )?,
            cry_meter: self.pokedex_sheet("cry_meter", (80, 64), COLORS_PER_4BPP_BANK)?,
            cry_meter_needle: self.pokedex_sheet(
                "cry_meter_needle",
                (64, 64),
                COLORS_PER_4BPP_BANK,
            )?,
            cry_screen_bg: self.pokedex_sheet("cry_screen_bg", (8, 8), COLORS_PER_4BPP_BANK)?,
            bg_hoenn_palette: self.pokedex_palette("bg_hoenn", 6 * COLORS_PER_4BPP_BANK)?,
            bg_national_palette: self.pokedex_palette("bg_national", 6 * COLORS_PER_4BPP_BANK)?,
            search_results_palette: self
                .pokedex_palette("search_results_bg", 6 * COLORS_PER_4BPP_BANK)?,
            search_menu_palette: self.pokedex_palette("search_menu", 4 * COLORS_PER_4BPP_BANK)?,
            region_map_palette: self.pokedex_palette("region_map", 3 * COLORS_PER_4BPP_BANK)?,
            size_silhouette_palette: self
                .pokedex_palette("size_silhouette", COLORS_PER_4BPP_BANK)?,
            start_menu_main_tilemap: self
                .pokedex_raw("start_menu_main", text_tilemap_bytes(32, 12))?,
            start_menu_search_results_tilemap: self
                .pokedex_raw("start_menu_search_results", text_tilemap_bytes(32, 12))?,
            list_tilemap: self.pokedex_raw("list", text_tilemap_bytes(32, 20))?,
            list_underlay_tilemap: self.pokedex_raw("list_underlay", text_tilemap_bytes(32, 20))?,
            info_screen_tilemap: self.pokedex_raw("info_screen", text_tilemap_bytes(32, 20))?,
            size_screen_tilemap: self.pokedex_raw("size_screen", text_tilemap_bytes(32, 20))?,
            screen_select_bar_main_tilemap: self
                .pokedex_raw("screen_select_bar_main", text_tilemap_bytes(32, 3))?,
            screen_select_bar_submenu_tilemap: self
                .pokedex_raw("screen_select_bar_submenu", text_tilemap_bytes(32, 3))?,
            search_menu_hoenn_tilemap: self
                .pokedex_raw("search_menu_hoenn", text_tilemap_bytes(32, 20))?,
            search_menu_national_tilemap: self
                .pokedex_raw("search_menu_national", text_tilemap_bytes(32, 20))?,
            region_map_tilemap: self.pokedex_raw("region_map", text_tilemap_bytes(32, 32))?,
            region_map_affine_tilemap: self
                .pokedex_raw("region_map_affine", REGION_MAP_AFFINE_TILEMAP_BYTES)?,
            cry_screen_tilemap: self.pokedex_raw("cry_screen", text_tilemap_bytes(32, 20))?,
            cry_meter_tilemap: self.pokedex_raw("cry_meter_map", text_tilemap_bytes(10, 8))?,
            cry_screen_bg_raw: self.pokedex_raw("cry_screen_bg", CRY_SCREEN_BG_RAW_BYTES)?,
        })
    }

    fn pokedex_sheet(
        &self,
        name: &str,
        (width, height): (u32, u32),
        palette_colors: u16,
    ) -> Result<PalettedImage<'_>, PackError> {
        let bit_depth = if palette_colors == FULL_PALETTE_COLORS {
            8
        } else {
            4
        };
        self.paired_image(
            &format!("{IMAGE_PREFIX}/{name}"),
            &format!("{IMAGE_PALETTE_PREFIX}/{name}"),
            (width, height),
            bit_depth,
            palette_colors,
            |id, detail| PackError::MalformedPokedexGraphic { id, detail },
        )
    }

    fn pokedex_palette(&self, name: &str, colors: u16) -> Result<PaletteRef<'_>, PackError> {
        self.pokedex_palette_entry(&format!("{FILE_PALETTE_PREFIX}/{name}"), colors)
    }

    fn pokedex_palette_entry(&self, id: &str, colors: u16) -> Result<PaletteRef<'_>, PackError> {
        let palette = self.palette(id)?;
        if palette.color_count != colors
            || palette.raw.len() != usize::from(colors) * PALETTE_ENTRY_BYTES
        {
            return Err(PackError::MalformedPokedexGraphic {
                id: id.to_owned(),
                detail: format!(
                    "declares {} colours in {} bytes: expected {colors} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            });
        }
        Ok(palette)
    }

    fn pokedex_raw(&self, name: &str, expected_len: usize) -> Result<&[u8], PackError> {
        let id = format!("{RAW_PREFIX}/{name}");
        let bytes = self.raw(&id)?;
        if bytes.len() != expected_len {
            return Err(PackError::MalformedPokedexGraphic {
                id,
                detail: format!("is {} bytes: expected {expected_len}", bytes.len()),
            });
        }
        Ok(bytes)
    }
}
