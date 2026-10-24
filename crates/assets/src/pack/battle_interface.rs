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

/// Pixels along one side of an 8x8 tile.
const FRAME_TILE_PIXELS: u32 = 8;
/// Both singles sheets hold two metatiles side by side.
const FRAME_METATILES_ACROSS: u32 = 2;
const ENEMY_SHADOW_WIDTH: u32 = 32;
const ENEMY_SHADOW_HEIGHT: u32 = 8;

/// Which side's singles health-box frame, whose sheet `gbagfx` groups into
/// different metatiles (`graphics.c:628-629`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinglesHealthboxSide {
    /// `gHealthboxSinglesPlayerGfx`.
    Player,
    /// `gHealthboxSinglesOpponentGfx`.
    Opponent,
}

impl SinglesHealthboxSide {
    /// Metatile width and height in 8x8 tiles: 8x8 for the player, 8x4 for the
    /// opponent.
    #[must_use]
    pub const fn metatile_tiles(self) -> (u32, u32) {
        match self {
            Self::Player => (8, 8),
            Self::Opponent => (8, 4),
        }
    }

    /// Width and height in pixels of the source sheet: two metatiles across,
    /// one down.
    #[must_use]
    pub const fn image_size(self) -> (u32, u32) {
        let (width, height) = self.metatile_tiles();
        (
            FRAME_METATILES_ACROSS * width * FRAME_TILE_PIXELS,
            height * FRAME_TILE_PIXELS,
        )
    }

    const fn pack_name(self) -> &'static str {
        match self {
            Self::Player => "healthbox_singles_player",
            Self::Opponent => "healthbox_singles_opponent",
        }
    }
}

const TILE_4BPP_BYTES: usize = 32;
const HEALTHBOX_ELEMENT_COUNT: usize = 13;
/// `hpbar_anim_unused` (18 tiles), `numbers1` (11), then `numbers2` (12).
const BATTLE_BAR_TILES: usize = 18 + 11 + 12;

/// A constituent of `gHealthboxElementsGfxTable`, declared in upstream table
/// order (`graphics.c:358-370`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthboxElement {
    HpBar,
    ExpBar,
    Status,
    Misc,
    HpBarAnim,
    MiscFrameEnd,
    BallDisplay,
    BallCaughtIndicator,
    Status2,
    Status3,
    Status4,
    HealthboxDoublesFrameEnd,
    HealthboxDoublesFrameEndBar,
}

impl HealthboxElement {
    /// Every constituent in `gHealthboxElementsGfxTable` order.
    pub const TABLE: [Self; HEALTHBOX_ELEMENT_COUNT] = [
        Self::HpBar,
        Self::ExpBar,
        Self::Status,
        Self::Misc,
        Self::HpBarAnim,
        Self::MiscFrameEnd,
        Self::BallDisplay,
        Self::BallCaughtIndicator,
        Self::Status2,
        Self::Status3,
        Self::Status4,
        Self::HealthboxDoublesFrameEnd,
        Self::HealthboxDoublesFrameEndBar,
    ];

    /// 8x8 tiles in the constituent's `.4bpp` file.
    #[must_use]
    pub const fn tile_count(self) -> usize {
        match self {
            Self::HpBar => 12,
            Self::ExpBar => 9,
            Self::Status | Self::Status2 | Self::Status3 | Self::Status4 => 15,
            Self::Misc => 11,
            Self::HpBarAnim => 18,
            Self::BallDisplay => 4,
            Self::MiscFrameEnd
            | Self::BallCaughtIndicator
            | Self::HealthboxDoublesFrameEnd
            | Self::HealthboxDoublesFrameEndBar => 1,
        }
    }

    /// The pack name, which is the upstream file stem.
    const fn pack_name(self) -> &'static str {
        match self {
            Self::HpBar => "hpbar",
            Self::ExpBar => "expbar",
            Self::Status => "status",
            Self::Misc => "misc",
            Self::HpBarAnim => "hpbar_anim",
            Self::MiscFrameEnd => "misc_frameend",
            Self::BallDisplay => "ball_display",
            Self::BallCaughtIndicator => "ball_caught_indicator",
            Self::Status2 => "status2",
            Self::Status3 => "status3",
            Self::Status4 => "status4",
            Self::HealthboxDoublesFrameEnd => "healthbox_doubles_frameend",
            Self::HealthboxDoublesFrameEndBar => "healthbox_doubles_frameend_bar",
        }
    }
}

/// One side's singles health-box frame, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct SinglesHealthboxFrameHandle<'a> {
    image: ImageRef<'a>,
    side: SinglesHealthboxSide,
}

impl<'a> SinglesHealthboxFrameHandle<'a> {
    /// The source sheet in PNG raster order, one palette index per pixel.
    #[must_use]
    pub fn image(&self) -> ImageRef<'a> {
        self.image
    }

    /// The side whose geometry the sheet was validated against.
    #[must_use]
    pub fn side(&self) -> SinglesHealthboxSide {
        self.side
    }

    /// Metatile width and height in 8x8 tiles, as for [`SinglesHealthboxSide`].
    #[must_use]
    pub fn metatile_tiles(&self) -> (u32, u32) {
        self.side.metatile_tiles()
    }
}

/// The singles health-box frames and the two palettes beside them, borrowing
/// the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct SinglesHealthboxFramesHandle<'a> {
    /// `gHealthboxSinglesPlayerGfx`.
    pub player: SinglesHealthboxFrameHandle<'a>,
    /// `gHealthboxSinglesOpponentGfx`.
    pub opponent: SinglesHealthboxFrameHandle<'a>,
    /// `gBattleInterface_BallStatusBarPal`, 16 colours.
    pub ball_status_bar_palette: PaletteRef<'a>,
    /// `gBattleInterface_BallDisplayPal`, 16 colours.
    pub ball_display_palette: PaletteRef<'a>,
}

/// The 4bpp tiles of `gHealthboxElementsGfxTable`, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct HealthboxElementsHandle<'a> {
    /// Indexed by [`HealthboxElement`] discriminant, so in table order.
    sheets: [&'a [u8]; HEALTHBOX_ELEMENT_COUNT],
}

impl<'a> HealthboxElementsHandle<'a> {
    /// The raw 4bpp bytes of one constituent: 32 bytes per tile.
    #[must_use]
    pub fn sheet(&self, element: HealthboxElement) -> &'a [u8] {
        self.sheets[element as usize]
    }

    /// Constituents with their bytes, in table order.
    pub fn sheets(&self) -> impl Iterator<Item = (HealthboxElement, &'a [u8])> + '_ {
        HealthboxElement::TABLE
            .into_iter()
            .map(|element| (element, self.sheet(element)))
    }

    /// Row `index` of `gHealthboxElementsGfxTable`, the 32 bytes of one tile,
    /// or `None` past the last tile.
    #[must_use]
    pub fn tile(&self, index: usize) -> Option<&'a [u8]> {
        let mut first_tile = 0;
        for (element, sheet) in self.sheets() {
            let end_tile = first_tile + element.tile_count();
            if index < end_tile {
                let start = (index - first_tile) * TILE_4BPP_BYTES;
                return Some(&sheet[start..start + TILE_4BPP_BYTES]);
            }
            first_tile = end_tile;
        }
        None
    }
}

/// `gBattleInterfaceGfx_BattleBar` before compression, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct BattleBarHandle<'a> {
    /// `hpbar_anim_unused`, `numbers1`, and `numbers2` as raw 4bpp tiles, in that
    /// order (`graphics_file_rules.mk:90-93`).
    pub tiles: &'a [u8],
}

impl AssetPack {
    /// Bundle the singles health-box frames and their two palettes.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleInterfaceAsset`] when a
    /// frame's dimensions, depth, or pixel indices, or a palette's size, differ
    /// from upstream.
    pub fn singles_healthbox_frames(&self) -> Result<SinglesHealthboxFramesHandle<'_>, PackError> {
        let frame = |side: SinglesHealthboxSide| -> Result<_, PackError> {
            let (width, height) = side.image_size();
            Ok(SinglesHealthboxFrameHandle {
                image: self.battle_interface_bank_image(side.pack_name(), width, height)?,
                side,
            })
        };
        Ok(SinglesHealthboxFramesHandle {
            player: frame(SinglesHealthboxSide::Player)?,
            opponent: frame(SinglesHealthboxSide::Opponent)?,
            ball_status_bar_palette: self
                .battle_interface_palette("ball_status_bar", BANK_COLORS)?,
            ball_display_palette: self.battle_interface_palette("ball_display", BANK_COLORS)?,
        })
    }

    /// The enemy shadow sheet `LoadAndCreateEnemyShadowSprites` loads.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleInterfaceAsset`] when its
    /// dimensions, depth, or pixel indices differ from upstream.
    pub fn enemy_mon_shadow(&self) -> Result<ImageRef<'_>, PackError> {
        self.battle_interface_bank_image(
            "enemy_mon_shadow",
            ENEMY_SHADOW_WIDTH,
            ENEMY_SHADOW_HEIGHT,
        )
    }

    /// A 4bpp image of `width` x `height` whose indices stay within one bank.
    fn battle_interface_bank_image(
        &self,
        name: &str,
        width: u32,
        height: u32,
    ) -> Result<ImageRef<'_>, PackError> {
        let id = format!("{IMAGE_PREFIX}/{name}");
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
        if (image.width, image.height) != (width, height) {
            return Err(fault(format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
            )));
        }
        if image.pixels.len() != (width * height) as usize {
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

    /// The health-box element sheets in upstream table order.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleInterfaceAsset`] when a
    /// sheet's length differs from its upstream tile count.
    pub fn healthbox_elements(&self) -> Result<HealthboxElementsHandle<'_>, PackError> {
        let mut sheets = [&[][..]; HEALTHBOX_ELEMENT_COUNT];
        for element in HealthboxElement::TABLE {
            sheets[element as usize] = self.battle_interface_raw(
                &format!("healthbox_element/{}", element.pack_name()),
                element.tile_count() * TILE_4BPP_BYTES,
            )?;
        }
        Ok(HealthboxElementsHandle { sheets })
    }

    /// The generated battle bar.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedBattleInterfaceAsset`] when its
    /// length differs from the three source sheets' tile count.
    pub fn battle_bar(&self) -> Result<BattleBarHandle<'_>, PackError> {
        Ok(BattleBarHandle {
            tiles: self.battle_interface_raw("battle_bar", BATTLE_BAR_TILES * TILE_4BPP_BYTES)?,
        })
    }
}
