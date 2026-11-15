//! The chooser's decoded graphics and the BG1-3 and OBJ composition
//! (`pokeemerald/src/starter_choose.c:120-149`, `:374-463`).

use assets::pack::{ImageRef, PaletteRef};
use assets::AssetPack;
use rendering::{
    compose_frame_with_effects, AffineMatrix, BgLayer, BgSlot, Bgr555, BitDepth, ColorEffect,
    EffectsConfig, FrameEffects, Framebuffer, LayerTargets, OamEntry, Palette,
    PaletteColorTransform, PaletteFadeBlend, PaletteStage, ScreenEntry, SpriteLayer, Tilemap,
    Tileset, WindowConfig, WindowLayerEnable, WindowRect,
};

use super::sprites::{BALL_BANK, CIRCLE_BANK, MON_BANK};
use super::{StarterChoice, StarterChooserError};

const TILE_DIM: usize = BitDepth::TILE_DIM;
const MAP_WIDTH_TILES: usize = 32;
const BAG_HEIGHT_TILES: usize = 20;
const GRASS_HEIGHT_TILES: usize = 32;
const BYTES_PER_ENTRY: usize = 2;

/// BG layer identities and priorities (`starter_choose.c:120-149`).
const GRASS_BG: u8 = 2;
const GRASS_PRIORITY: u8 = 3;
const BAG_BG: u8 = 3;
const BAG_PRIORITY: u8 = 1;

/// `BLDY` while the label window is open (`starter_choose.c:437-440`).
const WINDOW_DARKEN: u8 = 7;
const WINDOW_NONE: u8 = 0;

/// Chooser graphics decoded out of the pack: the two BG tilemaps and every
/// starter's OBJ tiles and palette.
#[derive(Debug)]
pub(super) struct Graphics {
    bg_tiles: Tileset,
    bg_palette: Palette,
    bag: Tilemap,
    grass: Tilemap,
    obj: [ObjGraphics; 3],
}

/// One starter's OBJ tile sheet and palette: balls, circle, then the picture.
#[derive(Debug)]
struct ObjGraphics {
    tiles: Tileset,
    palette: [Bgr555; Palette::LEN],
}

impl Graphics {
    pub(super) fn from_pack(pack: &AssetPack) -> Result<Self, StarterChooserError> {
        let chooser = pack.starter_chooser()?;
        let bg_tiles = Tileset::decode(BitDepth::Bpp4, &packed_tiles(chooser.tiles.image))?;
        let mut bg_palette = [Bgr555::default(); Palette::LEN];
        copy_bank_colors(&mut bg_palette, 0, chooser.tiles.palette);

        let ball_tiles = packed_tiles(chooser.pokeball_selection.image);
        let circle_tiles = packed_tiles(chooser.starter_circle.image);

        let mut obj = Vec::with_capacity(StarterChoice::ALL.len());
        for choice in StarterChoice::ALL {
            let preview = pack.starter_preview(choice.starter_species())?;
            let tiles = [
                ball_tiles.as_slice(),
                circle_tiles.as_slice(),
                packed_tiles(preview.front.image).as_slice(),
            ]
            .concat();
            let mut palette = [Bgr555::default(); Palette::LEN];
            copy_bank_colors(&mut palette, BALL_BANK, chooser.pokeball_selection.palette);
            copy_bank_colors(&mut palette, CIRCLE_BANK, chooser.starter_circle.palette);
            copy_bank_colors(&mut palette, MON_BANK, preview.front.palette);
            obj.push(ObjGraphics {
                tiles: Tileset::decode(BitDepth::Bpp4, &tiles)?,
                palette,
            });
        }
        let Ok(obj) = <[ObjGraphics; 3]>::try_from(obj) else {
            unreachable!("one OBJ sheet was built per starter")
        };

        Ok(Self {
            bg_tiles,
            bg_palette: Palette::new(bg_palette),
            bag: tilemap(chooser.bag_tilemap, BAG_HEIGHT_TILES)?,
            grass: tilemap(chooser.grass_tilemap, GRASS_HEIGHT_TILES)?,
            obj,
        })
    }

    /// Composes BG1-3 and the sprites with the label window's darkening.
    ///
    /// `bg_fade` and `obj_fade` are the palette fade's two halves;
    /// `unfaded_mon_palette` leaves the Pokémon's bank raw, as a palette loaded
    /// between fade updates is (`palette.c:91-95`).
    pub(super) fn compose(&self, scene: &SceneFrame<'_>) -> Framebuffer {
        let obj = &self.obj[scene.preview_species.ordinal()];
        let obj_palette = Palette::new(faded_obj_colors(
            &obj.palette,
            scene.obj_fade,
            scene.unfaded_mon_palette,
        ));
        let grass = BgSlot::new(
            BgLayer::new(&self.bg_tiles, &self.bg_palette, &self.grass),
            GRASS_BG,
            GRASS_PRIORITY,
            0,
            0,
            true,
        );
        let bag = BgSlot::new(
            BgLayer::new(&self.bg_tiles, &self.bg_palette, &self.bag),
            BAG_BG,
            BAG_PRIORITY,
            0,
            0,
            true,
        );
        let sprites = SpriteLayer::new(scene.oam, &obj.tiles, &obj.tiles, &obj_palette)
            .with_affine_matrices(scene.matrices);

        let bg_stage = PaletteStage {
            bg: Some(&scene.bg_fade),
            obj: None,
        };
        let effects = FrameEffects {
            windows: label_windows(scene.label_window),
            color: EffectsConfig {
                effect: ColorEffect::Darken,
                target1: LayerTargets {
                    bg: [false, true, true, true],
                    obj: true,
                    backdrop: true,
                },
                evy: WINDOW_DARKEN,
                ..EffectsConfig::default()
            },
            backdrop: scene
                .bg_fade
                .transform(self.bg_palette.color(0).to_rgb888()),
            palette: bg_stage,
            ..FrameEffects::default()
        };
        compose_frame_with_effects(&sprites, &[grass, bag], &effects)
    }
}

/// What one composition shows.
pub(super) struct SceneFrame<'a> {
    pub(super) oam: &'a [OamEntry],
    pub(super) matrices: &'a [AffineMatrix],
    /// The starter whose picture sheet the OBJ tiles and bank 14 carry.
    pub(super) preview_species: StarterChoice,
    pub(super) label_window: Option<WindowRect>,
    pub(super) bg_fade: PaletteFadeBlend,
    pub(super) obj_fade: PaletteFadeBlend,
    pub(super) unfaded_mon_palette: bool,
}

/// `WIN0` encloses the label with colour effects; outside it every layer
/// shows without them (`starter_choose.c:424-440`).
fn label_windows(label: Option<WindowRect>) -> WindowConfig {
    let rect = label.unwrap_or_else(|| {
        WindowRect::new(
            rendering::WindowRange::new(WINDOW_NONE, WINDOW_NONE),
            rendering::WindowRange::new(WINDOW_NONE, WINDOW_NONE),
        )
    });
    WindowConfig {
        win0: Some((rect, WindowLayerEnable::ALL)),
        winout: WindowLayerEnable {
            effects: false,
            ..WindowLayerEnable::ALL
        },
        ..WindowConfig::default()
    }
}

/// Applies the OBJ fade to every bank the chooser uses, except the
/// Pokémon's bank while it is freshly loaded.
fn faded_obj_colors(
    base: &[Bgr555; Palette::LEN],
    fade: PaletteFadeBlend,
    unfaded_mon_palette: bool,
) -> [Bgr555; Palette::LEN] {
    let mut colors = *base;
    let banks = [BALL_BANK, CIRCLE_BANK, MON_BANK];
    for bank in banks {
        if bank == MON_BANK && unfaded_mon_palette {
            continue;
        }
        let start = usize::from(bank) * Palette::BANK_LEN;
        for color in &mut colors[start..start + Palette::BANK_LEN] {
            let faded = fade.transform(color.to_rgb888());
            *color = Bgr555::from_channels(faded.r >> 3, faded.g >> 3, faded.b >> 3);
        }
    }
    colors
}

/// Copies an indexed image's tiles in row-major tile order as packed 4bpp
/// bytes, left pixel in the low nibble: the layout 1D OBJ mapping reads.
/// The pack validates 4-bit indices and tile-aligned dimensions.
fn packed_tiles(image: ImageRef<'_>) -> Vec<u8> {
    let width = usize::try_from(image.width).unwrap_or(0);
    let height = usize::try_from(image.height).unwrap_or(0);
    let mut packed = Vec::with_capacity(width * height / 2);
    for tile_row in 0..height / TILE_DIM {
        for tile_col in 0..width / TILE_DIM {
            for y in 0..TILE_DIM {
                let start = (tile_row * TILE_DIM + y) * width + tile_col * TILE_DIM;
                for pair in image.pixels[start..start + TILE_DIM].chunks_exact(2) {
                    packed.push(pair[0] | (pair[1] << 4));
                }
            }
        }
    }
    packed
}

/// Writes a pack palette's colours into a 256-entry palette from `bank`.
fn copy_bank_colors(colors: &mut [Bgr555; Palette::LEN], bank: u8, source: PaletteRef<'_>) {
    let start = usize::from(bank) * Palette::BANK_LEN;
    for (offset, slot) in colors[start..].iter_mut().enumerate() {
        let Some(raw) = source.color(offset) else {
            break;
        };
        *slot = Bgr555::from_raw(raw);
    }
}

/// Decodes a little-endian screen-entry dump into a 32-wide map.
fn tilemap(raw: &[u8], height_tiles: usize) -> Result<Tilemap, rendering::RenderError> {
    let entries = raw
        .chunks_exact(BYTES_PER_ENTRY)
        .map(|pair| ScreenEntry::from_raw(u16::from_le_bytes([pair[0], pair[1]])))
        .collect();
    Tilemap::new(MAP_WIDTH_TILES, height_tiles, entries)
}
