//! Singles health-box frame, enemy shadow, and ball palette locator tests over
//! a synthetic ROM and pack; the fixture they extend is in the sibling `tests`
//! module.

use pack_format::{image_entry_from_tiles, PackEntry};
use rom_import::Encoding;

use super::locate;
use super::tests::{fixture, palette_bytes, with_spare_copy};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

pub(super) const PLAYER_ID: &str = "battle-interface/image/healthbox_singles_player";
pub(super) const OPPONENT_ID: &str = "battle-interface/image/healthbox_singles_opponent";
pub(super) const SHADOW_ID: &str = "battle-interface/image/enemy_mon_shadow";
pub(super) const BALL_STATUS_ID: &str = "battle-interface/palette/ball_status_bar";
pub(super) const BALL_DISPLAY_ID: &str = "battle-interface/palette/ball_display";

/// One singles frame: its pack id, ROM address, raster size in pixels, the
/// `-mwidth`/`-mheight` upstream cuts it with (`src/graphics.c:628-629`), and
/// the symbol it is declared under.
pub(super) struct Frame {
    pub(super) id: &'static str,
    pub(super) at: usize,
    pub(super) size: (u32, u32),
    pub(super) metatile: (u32, u32),
    pub(super) symbol: &'static str,
}

/// The player frame is two 8x8-tile metatiles across, the opponent frame two
/// 8x4-tile ones.
pub(super) const PLAYER: Frame = Frame {
    id: PLAYER_ID,
    at: PLAYER_AT,
    size: (128, 64),
    metatile: (8, 8),
    symbol: "gHealthboxSinglesPlayerGfx",
};
pub(super) const OPPONENT: Frame = Frame {
    id: OPPONENT_ID,
    at: OPPONENT_AT,
    size: (128, 32),
    metatile: (8, 4),
    symbol: "gHealthboxSinglesOpponentGfx",
};

impl Frame {
    pub(super) fn tile_count(&self) -> usize {
        (self.size.0 / 8 * (self.size.1 / 8)) as usize
    }

    /// The ROM tile bytes: distinct per sheet and per tile, so a wrong
    /// metatile walk permutes them into something the ROM does not hold.
    pub(super) fn tiles(&self) -> Vec<u8> {
        distinctive_tiles(self.tile_count())
            .into_iter()
            .map(|byte| byte ^ u8::try_from(self.at >> 16).expect("small"))
            .collect()
    }

    pub(super) fn entry(&self) -> PackEntry {
        let (width, height) = self.size;
        image_entry_from_tiles(
            self.id.to_owned(),
            &self.tiles(),
            4,
            width,
            height,
            Some(self.metatile),
        )
        .expect("frame")
    }
}

/// `enemy_mon_shadow.png`: 32x8, four tiles in one row, no metatile cut.
pub(super) fn shadow_tiles() -> Vec<u8> {
    distinctive_tiles(4)
        .into_iter()
        .map(|byte| byte ^ 0x3C)
        .collect()
}

pub(super) const PLAYER_AT: usize = 0x18_0000;
pub(super) const OPPONENT_AT: usize = 0x19_0000;
pub(super) const SHADOW_AT: usize = 0x1A_0000;
pub(super) const BALL_STATUS_AT: usize = 0x1B_0000;
pub(super) const BALL_DISPLAY_AT: usize = 0x1C_0000;

#[test]
fn the_two_frames_keep_their_own_metatile_shapes() {
    let (rom, entries) = fixture();
    with_context("battle-interface-frames", &rom, entries, |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        for frame in [&PLAYER, &OPPONENT] {
            let image = plan
                .images
                .iter()
                .find(|image| image.id == frame.id)
                .expect("planned");
            assert_eq!(
                (
                    image.addr,
                    image.encoding,
                    (image.rom_bit_depth, image.pack_bit_depth),
                    (image.width, image.height),
                    (image.metatile_width, image.metatile_height),
                    image.tile_count,
                ),
                (
                    0x0800_0000 + u32::try_from(frame.at).expect("small"),
                    Encoding::Lz77,
                    (4, 4),
                    frame.size,
                    frame.metatile,
                    u32::try_from(frame.tile_count()).expect("small"),
                ),
                "{}",
                frame.id
            );
        }
        // The shadow is one row of tiles: every metatile width reads the same
        // bytes, so the generator reports the largest, as for any one-metatile
        // grid, and its stored tiles are what matter.
        let shadow = plan
            .images
            .iter()
            .find(|image| image.id == SHADOW_ID)
            .expect("planned");
        assert_eq!(
            (shadow.addr, shadow.encoding, shadow.tile_count),
            (0x081A_0000, Encoding::Lz77, 4)
        );
        assert_eq!((shadow.width, shadow.height), (32, 8));
    });
}

#[test]
fn a_frames_recovered_shape_is_the_cut_its_rom_tiles_were_written_with() {
    // The search reads the shape off the ROM: the same pack raster with the
    // tiles stored as 8x4 metatiles is located as 8x4, not assumed 8x8. The
    // importer then unpacks it under the recorded shape, so the raster is
    // rebuilt either way; only a shape the profile misrecords permutes it.
    let (mut rom, entries) = fixture();
    let (width, height) = PLAYER.size;
    let raster = image_entry_from_tiles(
        PLAYER_ID.to_owned(),
        &PLAYER.tiles(),
        4,
        width,
        height,
        Some(PLAYER.metatile),
    )
    .expect("raster");
    let recut = pack_format::tiles_from_image(&raster.payload, 4, width, height, Some((8, 4)))
        .expect("recut");
    assert_ne!(recut, PLAYER.tiles());
    let stream = lz77_literals(&PLAYER.tiles());
    rom[PLAYER_AT..PLAYER_AT + stream.len()].fill(0);
    let stream = lz77_literals(&recut);
    rom[PLAYER_AT..PLAYER_AT + stream.len()].copy_from_slice(&stream);
    with_context("battle-interface-frame-shape", &rom, entries, |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        let player = plan
            .images
            .iter()
            .find(|image| image.id == PLAYER_ID)
            .expect("planned");
        assert_eq!((player.metatile_width, player.metatile_height), (8, 4));
        assert_eq!(player.addr, 0x0818_0000);
        let opponent = plan
            .images
            .iter()
            .find(|image| image.id == OPPONENT_ID)
            .expect("planned");
        assert_eq!(
            (opponent.metatile_width, opponent.metatile_height),
            OPPONENT.metatile
        );
    });
}

#[test]
fn a_ball_palette_present_twice_is_ambiguous() {
    for (id, seed) in [(BALL_STATUS_ID, 0x4000), (BALL_DISPLAY_ID, 0x5000)] {
        let (_, bytes) = palette_bytes(seed, 16);
        let (rom, entries) = fixture();
        let rom = with_spare_copy(rom, &bytes);
        with_context("battle-interface-ball-twice", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::Ambiguous { id: named, .. } if named == id),
                "{id}: {err}"
            );
        });
    }
}
