//! Typed opening-scene graphics: Birch's speech scene and the wall clock
//! (upstream `src/main_menu.c`, `src/field_effect.c`, `src/wallclock.c`).
//!
//! Pack ids stay private so scene code composes no strings. Every lookup
//! re-validates shapes, since a hand-built or corrupt pack can bypass extraction.
//! Palettes are 16-colour banks that pixel indices select from; index 0 is
//! transparent wherever a sheet is drawn as a sprite or a tilemapped background.

use super::{AssetPack, ImageRef, PackError, PaletteRef, PalettedImage};

const SHADOW_IMAGE_ID: &str = "opening-scene/image/shadow";
const BIRCH_IMAGE_ID: &str = "opening-scene/image/birch";
const LOTAD_IMAGE_ID: &str = "opening-scene/image/lotad";
const CLOCK_IMAGE_ID: &str = "opening-scene/image/wall_clock";
const HAND_IMAGE_ID: &str = "opening-scene/image/clock_hand";

const BACKGROUND_PALETTE_IDS: [&str; 2] =
    ["opening-scene/palette/bg0", "opening-scene/palette/bg1"];
const GRADIENT_PALETTE_ID: &str = "opening-scene/palette/bg2";
const BIRCH_PALETTE_ID: &str = "opening-scene/palette/birch";
const LOTAD_PALETTE_ID: &str = "opening-scene/palette/lotad";
const CLOCK_MALE_PALETTE_ID: &str = "opening-scene/palette/clock_male";
const CLOCK_FEMALE_PALETTE_ID: &str = "opening-scene/palette/clock_female";
const CLOCK_TEXT_PROMPT_PALETTE_ID: &str = "opening-scene/palette/clock_text_prompt";

const BIRCH_MAP_ID: &str = "opening-scene/raw/birch_speech_map";
const CLOCK_START_MAP_ID: &str = "opening-scene/raw/clock_start";
const CLOCK_VIEW_MAP_ID: &str = "opening-scene/raw/clock_view";

const BIT_DEPTH: u8 = 4;
const BANK_COLORS: u16 = 16;
const TEXT_PROMPT_COLORS: u16 = 4;
const PALETTE_ENTRY_BYTES: usize = 2;
/// A 32x20 screen of two-byte tilemap entries.
const SCREEN_TILEMAP_BYTES: usize = 32 * 20 * 2;

const SHADOW_SHAPE: (u32, u32) = (128, 24);
const PIC_SIDE: u32 = 64;
const CLOCK_SHAPE: (u32, u32) = (128, 64);
const HAND_SHAPE: (u32, u32) = (64, 144);

/// Every graphic Birch's new-game speech scene draws, borrowing the pack for `'a`.
///
/// [`AssetPack::birch_speech`] returns it with every entry validated.
#[derive(Debug, Clone, Copy)]
pub struct BirchSpeechHandle<'a> {
    /// The 128x24 shadow and platform tile sheet; its pixels select from
    /// either of [`background_palettes`](Self::background_palettes).
    pub shadow: ImageRef<'a>,
    /// The two 16-colour background banks (`bg0.pal`, `bg1.pal`) loaded at
    /// background palette slots 0 and 1.
    pub background_palettes: [PaletteRef<'a>; 2],
    /// The 16-colour gradient palette (`bg2.pal`) whose colours 8-15 are
    /// loaded over background colours 1-8.
    pub gradient_palette: PaletteRef<'a>,
    /// The 32x20 background tilemap (`map.bin`).
    pub background_tilemap: &'a [u8],
    /// Birch's 64x64 sprite with the palette its own PNG gives it.
    pub birch: PalettedImage<'a>,
    /// Lotad's 64x64 normal front picture with its normal palette.
    pub lotad: PalettedImage<'a>,
}

/// Every graphic the wall-clock screen draws, borrowing the pack for `'a`.
///
/// [`AssetPack::wall_clock`] returns it with every entry validated. The
/// player's gender picks the male or female bank for both the screen and the
/// hand sprite.
#[derive(Debug, Clone, Copy)]
pub struct WallClockHandle<'a> {
    /// The 128x64 clock face tile sheet; its pixels select from either gender bank.
    pub screen: ImageRef<'a>,
    /// The 64x144 clock-hand sprite sheet. Upstream stores it as a 4-bit
    /// grayscale PNG, which `gbagfx` inverts into palette indices; the pack
    /// holds those indices, which select from either gender bank.
    pub hand: ImageRef<'a>,
    /// The male 16-colour bank (`male.pal`).
    pub male_palette: PaletteRef<'a>,
    /// The female 16-colour bank (`female.pal`).
    pub female_palette: PaletteRef<'a>,
    /// The four-colour "Cancel"/"Confirm" prompt palette (`text_prompt.pal`).
    pub text_prompt_palette: PaletteRef<'a>,
    /// The 32x20 clock-setting tilemap (`clock_start.bin`).
    pub start_tilemap: &'a [u8],
    /// The 32x20 clock-viewing tilemap (`clock_view.bin`).
    pub view_tilemap: &'a [u8],
}

impl AssetPack {
    /// Bundle every Birch speech-scene graphic.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedOpeningSceneAsset`] when
    /// dimensions, bit depth (4 for every sheet), palette size, pixel indices,
    /// or tilemap length differ from upstream.
    pub fn birch_speech(&self) -> Result<BirchSpeechHandle<'_>, PackError> {
        let [bg0, bg1] = BACKGROUND_PALETTE_IDS;
        let shadow = self.scene_image(SHADOW_IMAGE_ID, bg0, SHADOW_SHAPE, BANK_COLORS)?;
        self.scene_image(SHADOW_IMAGE_ID, bg1, SHADOW_SHAPE, BANK_COLORS)?;
        Ok(BirchSpeechHandle {
            shadow: shadow.image,
            background_palettes: [shadow.palette, self.scene_palette(bg1, BANK_COLORS)?],
            gradient_palette: self.scene_palette(GRADIENT_PALETTE_ID, BANK_COLORS)?,
            background_tilemap: self.scene_tilemap(BIRCH_MAP_ID)?,
            birch: self.scene_image(
                BIRCH_IMAGE_ID,
                BIRCH_PALETTE_ID,
                (PIC_SIDE, PIC_SIDE),
                BANK_COLORS,
            )?,
            lotad: self.scene_image(
                LOTAD_IMAGE_ID,
                LOTAD_PALETTE_ID,
                (PIC_SIDE, PIC_SIDE),
                BANK_COLORS,
            )?,
        })
    }

    /// Bundle every wall-clock graphic.
    ///
    /// # Errors
    ///
    /// Same as [`birch_speech`](Self::birch_speech).
    pub fn wall_clock(&self) -> Result<WallClockHandle<'_>, PackError> {
        let screen = self.scene_image(
            CLOCK_IMAGE_ID,
            CLOCK_MALE_PALETTE_ID,
            CLOCK_SHAPE,
            BANK_COLORS,
        )?;
        let hand = self.scene_image(
            HAND_IMAGE_ID,
            CLOCK_MALE_PALETTE_ID,
            HAND_SHAPE,
            BANK_COLORS,
        )?;
        // Either gender's bank may be selected, so each must cover the pixels.
        self.scene_image(
            CLOCK_IMAGE_ID,
            CLOCK_FEMALE_PALETTE_ID,
            CLOCK_SHAPE,
            BANK_COLORS,
        )?;
        self.scene_image(
            HAND_IMAGE_ID,
            CLOCK_FEMALE_PALETTE_ID,
            HAND_SHAPE,
            BANK_COLORS,
        )?;
        Ok(WallClockHandle {
            screen: screen.image,
            hand: hand.image,
            male_palette: screen.palette,
            female_palette: self.scene_palette(CLOCK_FEMALE_PALETTE_ID, BANK_COLORS)?,
            text_prompt_palette: self
                .scene_palette(CLOCK_TEXT_PROMPT_PALETTE_ID, TEXT_PROMPT_COLORS)?,
            start_tilemap: self.scene_tilemap(CLOCK_START_MAP_ID)?,
            view_tilemap: self.scene_tilemap(CLOCK_VIEW_MAP_ID)?,
        })
    }

    fn scene_image(
        &self,
        image_id: &str,
        palette_id: &str,
        shape: (u32, u32),
        palette_colors: u16,
    ) -> Result<PalettedImage<'_>, PackError> {
        self.paired_image(
            image_id,
            palette_id,
            shape,
            BIT_DEPTH,
            palette_colors,
            |id, detail| PackError::MalformedOpeningSceneAsset { id, detail },
        )
    }

    fn scene_palette(&self, id: &str, colors: u16) -> Result<PaletteRef<'_>, PackError> {
        let palette = self.palette(id)?;
        if palette.color_count != colors
            || palette.raw.len() != usize::from(colors) * PALETTE_ENTRY_BYTES
        {
            return Err(PackError::MalformedOpeningSceneAsset {
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

    fn scene_tilemap(&self, id: &str) -> Result<&[u8], PackError> {
        let bytes = self.raw(id)?;
        if bytes.len() != SCREEN_TILEMAP_BYTES {
            return Err(PackError::MalformedOpeningSceneAsset {
                id: id.to_owned(),
                detail: format!("is {} bytes: expected {SCREEN_TILEMAP_BYTES}", bytes.len()),
            });
        }
        Ok(bytes)
    }
}
