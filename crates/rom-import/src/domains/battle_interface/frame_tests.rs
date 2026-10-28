//! Singles health-box frame, enemy shadow, and ball palette reader tests over
//! a synthetic ROM.
//!
//! The two compressed frame sheets are cut as 8x8- and 8x4-tile metatiles, the
//! compressed 32x8 shadow has no cut, and the two ball palettes are raw
//! 16-colour runs, all under their real `battle-interface/` ids. Each lands in
//! the same `images` and `palettes` slices as the textbox records.

use pack_format::{parse_directory, DirectoryEntry, EntryKind, PackWriter};

use super::tests::{at, bank, lz77_literals, payload};
use super::write;
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::rom::Rom;
use crate::roots::{BattleInterfaceRoots, Encoding, ImageRoot, PaletteRoot, Roots};

const PLAYER_AT: usize = 0x8_0000;
const OPPONENT_AT: usize = 0x9_0000;
const SHADOW_AT: usize = 0xA_0000;
const BALL_STATUS_AT: usize = 0xA_1000;
const BALL_DISPLAY_AT: usize = 0xA_2000;

/// One singles frame or the shadow: its id, ROM address, raster size in
/// pixels, and the metatile cut it is stored under.
struct Sheet {
    id: &'static str,
    at: usize,
    size: (u32, u32),
    metatile: (u32, u32),
}

const PLAYER: Sheet = Sheet {
    id: "battle-interface/image/healthbox_singles_player",
    at: PLAYER_AT,
    size: (128, 64),
    metatile: (8, 8),
};
const OPPONENT: Sheet = Sheet {
    id: "battle-interface/image/healthbox_singles_opponent",
    at: OPPONENT_AT,
    size: (128, 32),
    metatile: (8, 4),
};
const SHADOW: Sheet = Sheet {
    id: "battle-interface/image/enemy_mon_shadow",
    at: SHADOW_AT,
    size: (32, 8),
    metatile: (1, 1),
};

impl Sheet {
    fn tiles_across(&self) -> usize {
        usize::try_from(self.size.0 / 8).expect("small")
    }

    fn tile_count(&self) -> usize {
        self.tiles_across() * usize::try_from(self.size.1 / 8).expect("small")
    }

    /// 4bpp tiles where ROM tile `n` is uniformly colour `n % 16`.
    fn tiles(&self) -> Vec<u8> {
        (0..self.tile_count())
            .flat_map(|tile| {
                let colour = u8::try_from(tile % 16).expect("a nibble");
                [colour | colour << 4; 32]
            })
            .collect()
    }

    /// The ROM tile index the metatile walk puts at raster tile `(tx, ty)`.
    fn rom_tile(&self, tx: usize, ty: usize) -> usize {
        let (mw, mh) = (
            usize::try_from(self.metatile.0).expect("small"),
            usize::try_from(self.metatile.1).expect("small"),
        );
        (tx / mw) * (mw * mh)
            + (ty % mh) * mw
            + tx % mw
            + (ty / mh) * (self.tiles_across() / mw) * (mw * mh)
    }

    fn root(&self) -> ImageRoot {
        ImageRoot {
            id: self.id,
            addr: at(self.at),
            encoding: Encoding::Lz77,
            rom_bit_depth: 4,
            pack_bit_depth: 4,
            width: self.size.0,
            height: self.size.1,
            metatile_width: self.metatile.0,
            metatile_height: self.metatile.1,
            tile_count: u32::try_from(self.tile_count()).expect("small"),
        }
    }
}

const BALL_STATUS_ROOT: PaletteRoot = PaletteRoot {
    id: "battle-interface/palette/ball_status_bar",
    addr: at(BALL_STATUS_AT),
    encoding: Encoding::Raw,
    color_count: 16,
};
const BALL_DISPLAY_ROOT: PaletteRoot = PaletteRoot {
    id: "battle-interface/palette/ball_display",
    addr: at(BALL_DISPLAY_AT),
    encoding: Encoding::Raw,
    color_count: 16,
};

fn rom() -> Rom {
    let bytes = RomFixture::new()
        .emerald_header()
        .write(PLAYER_AT, &lz77_literals(&PLAYER.tiles()))
        .write(OPPONENT_AT, &lz77_literals(&OPPONENT.tiles()))
        .write(SHADOW_AT, &lz77_literals(&SHADOW.tiles()))
        .write(BALL_STATUS_AT, &bank(0x0400))
        .write(BALL_DISPLAY_AT, &bank(0x0500))
        .finish();
    Rom::from_bytes(bytes).expect("the fixture header is valid")
}

/// The frames, the shadow, and the ball palettes as one slice each, the way
/// the generated profile holds them beside the textbox records.
fn with_frames() -> Roots {
    Roots {
        battle_interface: BattleInterfaceRoots {
            images: Vec::leak(vec![SHADOW.root(), OPPONENT.root(), PLAYER.root()]),
            palettes: Vec::leak(vec![BALL_DISPLAY_ROOT, BALL_STATUS_ROOT]),
            ..BattleInterfaceRoots::NONE
        },
        ..Roots::NONE
    }
}

fn written(roots: &Roots) -> Result<(Vec<u8>, Vec<DirectoryEntry>), ImportError> {
    let mut writer = PackWriter::new();
    write(&rom(), roots, &mut writer)?;
    let bytes = writer.finish().expect("no duplicate ids");
    let entries = parse_directory(&bytes).expect("a valid pack");
    Ok((bytes, entries))
}

#[test]
fn the_frames_and_shadow_are_validated_four_bit_images_not_raw_tiles() {
    let (_, entries) = written(&with_frames()).expect("every root is well-formed");
    let kind = |id: &str| entries.iter().find(|e| e.id == id).expect("written").kind;
    for sheet in [&PLAYER, &OPPONENT, &SHADOW] {
        assert_eq!(
            kind(sheet.id),
            EntryKind::Image {
                width: sheet.size.0,
                height: sheet.size.1,
                bit_depth: 4
            },
            "{}",
            sheet.id
        );
    }
    for id in [
        "battle-interface/palette/ball_status_bar",
        "battle-interface/palette/ball_display",
    ] {
        assert_eq!(kind(id), EntryKind::Palette { color_count: 16 }, "{id}");
    }
    assert_eq!(entries.len(), 5);
}

#[test]
fn each_frame_lands_in_the_raster_its_own_metatile_cut_implies() {
    let (bytes, entries) = written(&with_frames()).expect("every root is well-formed");
    for sheet in [&PLAYER, &OPPONENT, &SHADOW] {
        let pixels = payload(&bytes, &entries, sheet.id);
        let (width, height) = (
            usize::try_from(sheet.size.0).expect("small"),
            usize::try_from(sheet.size.1).expect("small"),
        );
        assert_eq!(pixels.len(), width * height, "{}", sheet.id);
        for (index, &pixel) in pixels.iter().enumerate() {
            let (x, y) = (index % width, index / width);
            let tile = sheet.rom_tile(x / 8, y / 8);
            assert_eq!(
                usize::from(pixel),
                tile % 16,
                "{} pixel ({x}, {y})",
                sheet.id
            );
        }
    }
    // The two frames are cut differently: swapping the cuts changes the raster.
    let swapped = Sheet {
        metatile: (8, 4),
        ..PLAYER
    };
    assert_ne!(
        (0..8).map(|ty| PLAYER.rom_tile(9, ty)).collect::<Vec<_>>(),
        (0..8).map(|ty| swapped.rom_tile(9, ty)).collect::<Vec<_>>()
    );
}

#[test]
fn the_ball_palettes_land_as_their_raw_bytes() {
    let (bytes, entries) = written(&with_frames()).expect("every root is well-formed");
    assert_eq!(
        payload(&bytes, &entries, "battle-interface/palette/ball_status_bar"),
        bank(0x0400)
    );
    assert_eq!(
        payload(&bytes, &entries, "battle-interface/palette/ball_display"),
        bank(0x0500)
    );
}

#[test]
fn the_whole_root_iterators_enumerate_the_frames_shadow_and_ball_palettes() {
    let roots = with_frames();
    let images: Vec<&str> = roots.images().map(|root| root.id).collect();
    let palettes: Vec<&str> = roots.palettes().map(|root| root.id).collect();
    for id in [PLAYER.id, OPPONENT.id, SHADOW.id] {
        assert!(images.contains(&id), "{id}");
    }
    for id in [
        "battle-interface/palette/ball_status_bar",
        "battle-interface/palette/ball_display",
    ] {
        assert!(palettes.contains(&id), "{id}");
    }
    assert_eq!((images.len(), palettes.len()), (3, 2));
}
