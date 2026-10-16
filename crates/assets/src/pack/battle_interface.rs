//! Typed battle interface graphics (upstream `graphics/battle_interface/`).
//!
//! Pack ids stay private so scene code composes no strings. Every lookup
//! re-validates shapes, since a hand-built or corrupt pack can bypass extraction.

use super::{AssetPack, ImageRef, PackError, PaletteRef};

const IMAGE_PREFIX: &str = "battle-interface/image";
const PALETTE_PREFIX: &str = "battle-interface/palette";
const RAW_PREFIX: &str = "battle-interface/raw";

const TEXTBOX_SIZE: u32 = 128;
const TEXTBOX_BIT_DEPTH: u8 = 4;
const TEXTBOX_PALETTE_COLORS: u16 = 32;
/// A 4bpp bank, the size of both text palettes.
const BANK_COLORS: u16 = 16;
const PALETTE_ENTRY_BYTES: usize = 2;
/// `textbox_map.bin`: a 64x32 tilemap of 2-byte entries.
const TEXTBOX_TILEMAP_BYTES: usize = 64 * 32 * 2;

/// The battle textbox `LoadBattleTextboxAndBackground` loads, borrowing the
/// pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct BattleTextboxHandle<'a> {
    /// Tile sheet (`textbox.png`), 128x128.
    pub image: ImageRef<'a>,
    /// 32-colour palette: `textbox_0.pal` then `textbox_1.pal`.
    pub palette: PaletteRef<'a>,
    /// Tilemap (`textbox_map.bin`).
    pub tilemap: &'a [u8],
}

/// Every battle interface graphic this namespace holds, borrowing the pack for `'a`.
///
/// [`AssetPack::battle_interface`] returns it with every entry validated.
#[derive(Debug, Clone, Copy)]
pub struct BattleInterfaceHandle<'a> {
    /// Textbox image, palette, and tilemap.
    pub textbox: BattleTextboxHandle<'a>,
    /// Battle-window text palette (`text.pal`), 16 colours.
    pub window_text_palette: PaletteRef<'a>,
    /// PP text palette (`text_pp.pal`), 16 colours.
    pub pp_text_palette: PaletteRef<'a>,
}

impl AssetPack {
    /// Bundle the battle textbox and the two menu text palettes.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleInterfaceAsset`] when
    /// dimensions, palette size, pixel indices, or tilemap length differ from
    /// upstream.
    pub fn battle_interface(&self) -> Result<BattleInterfaceHandle<'_>, PackError> {
        let palette = self.battle_interface_palette("textbox", TEXTBOX_PALETTE_COLORS)?;
        Ok(BattleInterfaceHandle {
            textbox: BattleTextboxHandle {
                image: self.battle_textbox_image()?,
                palette,
                tilemap: self.battle_interface_raw("textbox_map", TEXTBOX_TILEMAP_BYTES)?,
            },
            window_text_palette: self.battle_interface_palette("window_text", BANK_COLORS)?,
            pp_text_palette: self.battle_interface_palette("pp_text", BANK_COLORS)?,
        })
    }

    fn battle_textbox_image(&self) -> Result<ImageRef<'_>, PackError> {
        let id = format!("{IMAGE_PREFIX}/textbox");
        let image = self.image(&id)?;
        let fault = |detail: String| PackError::MalformedBattleInterfaceAsset {
            id: id.clone(),
            detail,
        };
        if image.bit_depth != TEXTBOX_BIT_DEPTH {
            return Err(fault(format!(
                "is {}-bit: expected {TEXTBOX_BIT_DEPTH}-bit",
                image.bit_depth
            )));
        }
        if image.width != TEXTBOX_SIZE || image.height != TEXTBOX_SIZE {
            return Err(fault(format!(
                "is {}x{}: expected {TEXTBOX_SIZE}x{TEXTBOX_SIZE}",
                image.width, image.height
            )));
        }
        if image.pixels.len() != (TEXTBOX_SIZE * TEXTBOX_SIZE) as usize {
            return Err(fault(format!(
                "carries {} payload bytes: expected one per pixel",
                image.pixels.len()
            )));
        }
        if let Some(&pixel) = image
            .pixels
            .iter()
            .find(|&&pixel| u16::from(pixel) >= BANK_COLORS)
        {
            return Err(fault(format!(
                "has pixel index {pixel}: a 4bpp tile selects within one {BANK_COLORS}-colour bank"
            )));
        }
        Ok(image)
    }

    fn battle_interface_palette(
        &self,
        name: &str,
        colors: u16,
    ) -> Result<PaletteRef<'_>, PackError> {
        let id = format!("{PALETTE_PREFIX}/{name}");
        let palette = self.palette(&id)?;
        if palette.color_count != colors
            || palette.raw.len() != usize::from(colors) * PALETTE_ENTRY_BYTES
        {
            return Err(PackError::MalformedBattleInterfaceAsset {
                id,
                detail: format!(
                    "declares {} colours in {} bytes: expected {colors} colours",
                    palette.color_count,
                    palette.raw.len()
                ),
            });
        }
        Ok(palette)
    }

    fn battle_interface_raw(&self, name: &str, expected_len: usize) -> Result<&[u8], PackError> {
        let id = format!("{RAW_PREFIX}/{name}");
        let bytes = self.raw(&id)?;
        if bytes.len() != expected_len {
            return Err(PackError::MalformedBattleInterfaceAsset {
                id,
                detail: format!("is {} bytes: expected {expected_len}", bytes.len()),
            });
        }
        Ok(bytes)
    }
}
