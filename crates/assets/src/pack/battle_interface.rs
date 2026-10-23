use super::{AssetPack, ImageRef, PackError, PaletteRef};

const IMAGE_PREFIX: &str = "battle-interface/image";
const PALETTE_PREFIX: &str = "battle-interface/palette";
const RAW_PREFIX: &str = "battle-interface/raw";

const TEXTBOX_SIZE: u32 = 128;
const TEXTBOX_BIT_DEPTH: u8 = 4;
const COLORS_PER_4BPP_BANK: u16 = 1 << TEXTBOX_BIT_DEPTH;
const TEXTBOX_PALETTE_BANKS: u16 = 2;
const TEXTBOX_PALETTE_COLORS: u16 = TEXTBOX_PALETTE_BANKS * COLORS_PER_4BPP_BANK;
const PALETTE_ENTRY_BYTES: usize = std::mem::size_of::<u16>();
const TILEMAP_BLOCK_SIDE_TILES: usize = 32;
const TEXTBOX_TILEMAP_BLOCKS: usize = 2;
const TILEMAP_ENTRY_BYTES: usize = std::mem::size_of::<u16>();
const TEXTBOX_TILEMAP_BYTES: usize = TEXTBOX_TILEMAP_BLOCKS
    * TILEMAP_BLOCK_SIDE_TILES
    * TILEMAP_BLOCK_SIDE_TILES
    * TILEMAP_ENTRY_BYTES;

/// Textbox pixels address one palette bank; tilemap entries select the bank.
/// Upstream: `src/graphics.c:4-6`, `src/bg.c:1169-1186`.
#[derive(Debug, Clone, Copy)]
pub struct BattleTextboxHandle<'a> {
    /// 128x128 4bpp tile sheet.
    pub image: ImageRef<'a>,
    /// `textbox_0.pal` followed by `textbox_1.pal` (`graphics_file_rules.mk:60-62`).
    pub palette: PaletteRef<'a>,
    /// Little-endian two-byte tilemap entries.
    pub tilemap: &'a [u8],
}

/// Fixed battle-interface entries validated by [`AssetPack::battle_interface`].
#[derive(Debug, Clone, Copy)]
pub struct BattleInterfaceHandle<'a> {
    /// Main battle message-window graphics.
    pub textbox: BattleTextboxHandle<'a>,
    /// Single-bank battle-window text palette.
    pub window_text_palette: PaletteRef<'a>,
    /// Single-bank PP text palette.
    pub pp_text_palette: PaletteRef<'a>,
}

impl AssetPack {
    /// Battle textbox and menu-text assets, validated on every call.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleInterfaceAsset`] for invalid
    /// image shape or bit depth, palette size, pixel indices, or tilemap length.
    pub fn battle_interface(&self) -> Result<BattleInterfaceHandle<'_>, PackError> {
        let palette = self.battle_interface_palette("textbox", TEXTBOX_PALETTE_COLORS)?;
        Ok(BattleInterfaceHandle {
            textbox: BattleTextboxHandle {
                image: self.battle_textbox_image()?,
                palette,
                tilemap: self.battle_interface_raw("textbox_map", TEXTBOX_TILEMAP_BYTES)?,
            },
            window_text_palette: self
                .battle_interface_palette("window_text", COLORS_PER_4BPP_BANK)?,
            pp_text_palette: self.battle_interface_palette("pp_text", COLORS_PER_4BPP_BANK)?,
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
            .find(|&&pixel| u16::from(pixel) >= COLORS_PER_4BPP_BANK)
        {
            return Err(fault(format!(
                "has pixel index {pixel}: a 4bpp tile selects within one {COLORS_PER_4BPP_BANK}-colour bank"
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
