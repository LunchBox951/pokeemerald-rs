//! Typed Pokédex screen graphics (upstream `graphics/pokedex/`).
//!
//! Pack ids stay private so scene code composes no strings. Every lookup
//! re-validates shapes, since a hand-built or corrupt pack can bypass extraction.

use super::{AssetPack, PackError, PaletteRef, PalettedImage};

const IMAGE_PREFIX: &str = "pokedex-graphics/image";
const IMAGE_PALETTE_PREFIX: &str = "pokedex-graphics/image-palette";
const FILE_PALETTE_PREFIX: &str = "pokedex-graphics/palette";
const RAW_PREFIX: &str = "pokedex-graphics/raw";

const TILEMAP_ENTRY_BYTES: usize = 2;
const PALETTE_ENTRY_BYTES: usize = 2;
const BANK_COLORS: u16 = 16;
const FULL_PALETTE_COLORS: u16 = 256;
/// Byte length of the extensionless `cry_screen_bg` file.
const CRY_SCREEN_BG_BYTES: usize = 32;

/// Every Pokédex tilemap is a screen of 2-byte entries, `width` x `height` tiles.
const fn tilemap_bytes(width: usize, height: usize) -> usize {
    width * height * TILEMAP_ENTRY_BYTES
}

/// Every Pokédex graphic the scenes draw, borrowing the pack for `'a`.
///
/// [`AssetPack::pokedex_graphics`] returns it with every entry validated.
/// Each sheet pairs with the palette embedded in its upstream PNG. The
/// standalone palettes (`bg_*.pal`, `search_results_bg.pal`, `size_silhouette.pal`,
/// `search_menu.pal`, `region_map.pal`) are what upstream loads for the menu,
/// interface, search, and regional-map sheets.
#[derive(Debug, Clone, Copy)]
pub struct PokedexGraphicsHandle<'a> {
    /// Main screen sheet (`menu.png`), 128x128.
    pub menu: PalettedImage<'a>,
    /// Interface chrome sheet (`interface.png`), 32x512.
    pub interface: PalettedImage<'a>,
    /// Unused no-ball cursor (`noball_unused.png`), 8x32.
    pub noball_unused: PalettedImage<'a>,
    /// Caught-ball marker (`caught_ball.png`), 8x16.
    pub caught_ball: PalettedImage<'a>,
    /// List arrows (`arrows.png`), 8x96.
    pub arrows: PalettedImage<'a>,
    /// Search menu sheet (`search_menu.png`), 128x64.
    pub search_menu: PalettedImage<'a>,
    /// Area-screen glow (`area_glow.png`), 32x96.
    pub area_glow: PalettedImage<'a>,
    /// Area-screen marker (`area_marker.png`), 16x16.
    pub area_marker: PalettedImage<'a>,
    /// Area-screen unknown-area art (`area_unknown.png`), 32x96.
    pub area_unknown: PalettedImage<'a>,
    /// Regional map (`region_map.png`), 128x120, 256 colours.
    pub region_map: PalettedImage<'a>,
    /// Affine regional map (`region_map_affine.png`), 128x120, 256 colours.
    pub region_map_affine: PalettedImage<'a>,
    /// Cry meter (`cry_meter.png`), 80x64.
    pub cry_meter: PalettedImage<'a>,
    /// Cry meter needle (`cry_meter_needle.png`), 64x64.
    pub cry_meter_needle: PalettedImage<'a>,
    /// Cry screen background tile (`cry_screen_bg.png`), 8x8.
    pub cry_screen_bg: PalettedImage<'a>,
    /// Hoenn background palette (`bg_hoenn.pal`), 96 colours.
    pub bg_hoenn_palette: PaletteRef<'a>,
    /// National background palette (`bg_national.pal`), 96 colours.
    pub bg_national_palette: PaletteRef<'a>,
    /// Search results background palette (`search_results_bg.pal`), 96 colours.
    pub search_results_palette: PaletteRef<'a>,
    /// Search menu palette (`search_menu.pal`), 64 colours.
    pub search_menu_palette: PaletteRef<'a>,
    /// Regional map palette (`region_map.pal`), 48 colours.
    pub region_map_palette: PaletteRef<'a>,
    /// Size screen silhouette palette (`size_silhouette.pal`), 16 colours.
    pub size_silhouette_palette: PaletteRef<'a>,
    /// Start menu main tilemap (`start_menu_main.bin`).
    pub start_menu_main_tilemap: &'a [u8],
    /// Start menu search results tilemap (`start_menu_search_results.bin`).
    pub start_menu_search_results_tilemap: &'a [u8],
    /// List tilemap (`list.bin`).
    pub list_tilemap: &'a [u8],
    /// List underlay tilemap (`list_underlay.bin`).
    pub list_underlay_tilemap: &'a [u8],
    /// Info screen tilemap (`info_screen.bin`).
    pub info_screen_tilemap: &'a [u8],
    /// Size screen tilemap (`size_screen.bin`).
    pub size_screen_tilemap: &'a [u8],
    /// Main screen-select bar tilemap (`screen_select_bar_main.bin`).
    pub screen_select_bar_main_tilemap: &'a [u8],
    /// Submenu screen-select bar tilemap (`screen_select_bar_submenu.bin`).
    pub screen_select_bar_submenu_tilemap: &'a [u8],
    /// Hoenn search menu tilemap (`search_menu_hoenn.bin`).
    pub search_menu_hoenn_tilemap: &'a [u8],
    /// National search menu tilemap (`search_menu_national.bin`).
    pub search_menu_national_tilemap: &'a [u8],
    /// Regional map tilemap (`region_map.bin`).
    pub region_map_tilemap: &'a [u8],
    /// Affine regional map tilemap (`region_map_affine.bin`).
    pub region_map_affine_tilemap: &'a [u8],
    /// Cry screen tilemap (`cry_screen.bin`).
    pub cry_screen_tilemap: &'a [u8],
    /// Unused cry meter tilemap (`cry_meter_map.bin`).
    pub cry_meter_tilemap: &'a [u8],
    /// Extensionless `cry_screen_bg`, 32 bytes; tracked but unreferenced upstream.
    pub cry_screen_bg_raw: &'a [u8],
}

impl AssetPack {
    /// Bundle every Pokédex screen graphic, palette, and tilemap.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedPokedexGraphic`] when dimensions,
    /// palette size, pixel indices, or tilemap length differ from upstream.
    pub fn pokedex_graphics(&self) -> Result<PokedexGraphicsHandle<'_>, PackError> {
        Ok(PokedexGraphicsHandle {
            menu: self.pokedex_sheet("menu", (128, 128), BANK_COLORS)?,
            interface: self.pokedex_sheet("interface", (32, 512), BANK_COLORS)?,
            noball_unused: self.pokedex_sheet("noball_unused", (8, 32), BANK_COLORS)?,
            caught_ball: self.pokedex_sheet("caught_ball", (8, 16), BANK_COLORS)?,
            arrows: self.pokedex_sheet("arrows", (8, 96), BANK_COLORS)?,
            search_menu: self.pokedex_sheet("search_menu", (128, 64), BANK_COLORS)?,
            area_glow: self.pokedex_sheet("area_glow", (32, 96), BANK_COLORS)?,
            area_marker: self.pokedex_sheet("area_marker", (16, 16), BANK_COLORS)?,
            area_unknown: self.pokedex_sheet("area_unknown", (32, 96), BANK_COLORS)?,
            region_map: self.pokedex_sheet("region_map", (128, 120), FULL_PALETTE_COLORS)?,
            region_map_affine: self.pokedex_sheet(
                "region_map_affine",
                (128, 120),
                FULL_PALETTE_COLORS,
            )?,
            cry_meter: self.pokedex_sheet("cry_meter", (80, 64), BANK_COLORS)?,
            cry_meter_needle: self.pokedex_sheet("cry_meter_needle", (64, 64), BANK_COLORS)?,
            cry_screen_bg: self.pokedex_sheet("cry_screen_bg", (8, 8), BANK_COLORS)?,
            bg_hoenn_palette: self.pokedex_palette("bg_hoenn", 96)?,
            bg_national_palette: self.pokedex_palette("bg_national", 96)?,
            search_results_palette: self.pokedex_palette("search_results_bg", 96)?,
            search_menu_palette: self.pokedex_palette("search_menu", 64)?,
            region_map_palette: self.pokedex_palette("region_map", 48)?,
            size_silhouette_palette: self.pokedex_palette("size_silhouette", 16)?,
            start_menu_main_tilemap: self.pokedex_raw("start_menu_main", tilemap_bytes(32, 12))?,
            start_menu_search_results_tilemap: self
                .pokedex_raw("start_menu_search_results", tilemap_bytes(32, 12))?,
            list_tilemap: self.pokedex_raw("list", tilemap_bytes(32, 20))?,
            list_underlay_tilemap: self.pokedex_raw("list_underlay", tilemap_bytes(32, 20))?,
            info_screen_tilemap: self.pokedex_raw("info_screen", tilemap_bytes(32, 20))?,
            size_screen_tilemap: self.pokedex_raw("size_screen", tilemap_bytes(32, 20))?,
            screen_select_bar_main_tilemap: self
                .pokedex_raw("screen_select_bar_main", tilemap_bytes(32, 3))?,
            screen_select_bar_submenu_tilemap: self
                .pokedex_raw("screen_select_bar_submenu", tilemap_bytes(32, 3))?,
            search_menu_hoenn_tilemap: self
                .pokedex_raw("search_menu_hoenn", tilemap_bytes(32, 20))?,
            search_menu_national_tilemap: self
                .pokedex_raw("search_menu_national", tilemap_bytes(32, 20))?,
            region_map_tilemap: self.pokedex_raw("region_map", tilemap_bytes(32, 32))?,
            region_map_affine_tilemap: self.pokedex_raw("region_map_affine", 64 * 64)?,
            cry_screen_tilemap: self.pokedex_raw("cry_screen", tilemap_bytes(32, 20))?,
            cry_meter_tilemap: self.pokedex_raw("cry_meter_map", tilemap_bytes(10, 8))?,
            cry_screen_bg_raw: self.pokedex_raw("cry_screen_bg", CRY_SCREEN_BG_BYTES)?,
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
