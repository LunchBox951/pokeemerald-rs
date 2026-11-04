//! A synthetic catalog, pack and checkout for the battle-effect locator tests.

use std::path::PathBuf;

use assets::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationProgram, EffectSpriteId, ProgramId,
};
use pack_format::{image_entry, palette_entry, raw_entry, PackEntry, PackWriter};
use rom_import::fixture::RomFixture;

use crate::gen_rom_profile::pack_source::PackSource;
use crate::gen_rom_profile::search::{Lz77Search, PointerIndex, RawSearch};
use crate::gen_rom_profile::Context;

/// A catalog whose one program loads each of `sprites`, selected by every slot.
pub(in crate::gen_rom_profile) fn catalog(sprites: &[u16]) -> BattleAnimationCatalog {
    let mut commands: Vec<AnimCommand> = sprites
        .iter()
        .map(|&sprite| AnimCommand::LoadSpriteGfx(EffectSpriteId::new(sprite).expect("in range")))
        .collect();
    commands.push(AnimCommand::End);
    let program =
        BattleAnimationProgram::new(AnimSymbol::new("Move_FIXTURE").expect("a symbol"), commands)
            .expect("a program");
    let mut selectors = Vec::new();
    for table in AnimationTable::ALL {
        for index in 0..table.len() {
            selectors.push((
                AnimationSelector::new(table, index).expect("a slot"),
                ProgramId::new(0),
            ));
        }
    }
    BattleAnimationCatalog::new(vec![program], selectors).expect("a catalog")
}

/// The sheet's raster: 8x16 (two tiles) of indices that differ per `salt`.
pub(in crate::gen_rom_profile) fn pixels(salt: u8) -> Vec<u8> {
    (0..8 * 16u16)
        .map(|n| u8::try_from((n * 7 + u16::from(salt)) % 16).expect("a nibble"))
        .collect()
}

/// The palette's colours: one 16-colour bank that differs per `salt`.
pub(in crate::gen_rom_profile) fn colors(salt: u16) -> Vec<u16> {
    (0..16u16).map(|n| n * 0x0421 + salt).collect()
}

/// The sheet's tiles as the ROM stores them.
pub(in crate::gen_rom_profile) fn tiles(salt: u8) -> Vec<u8> {
    pack_format::tiles_from_image(&pixels(salt), 4, 8, 16, Some((1, 1))).expect("tiles")
}

pub(in crate::gen_rom_profile) fn color_bytes(salt: u16) -> Vec<u8> {
    colors(salt)
        .iter()
        .flat_map(|color| color.to_le_bytes())
        .collect()
}

/// The pack entries of one sprite: its sheet, then its palette.
pub(in crate::gen_rom_profile) fn pair(sprite: u16, salt: u8) -> Vec<PackEntry> {
    vec![
        image_entry(
            format!("battle-effect/{sprite:03}/image"),
            8,
            16,
            4,
            pixels(salt),
        )
        .expect("an image"),
        palette_entry(
            format!("battle-effect/{sprite:03}/palette"),
            &colors(u16::from(salt)),
        )
        .expect("a palette"),
    ]
}

/// The catalog entry and one pair per sprite, each pair salted by its sprite.
pub(in crate::gen_rom_profile) fn entries(sprites: &[u16]) -> Vec<PackEntry> {
    let mut entries = vec![raw_entry(
        rom_import::BATTLE_ANIM_PACK_ID.to_owned(),
        catalog(sprites).encode(),
    )];
    for &sprite in sprites {
        entries.extend(pair(sprite, u8::try_from(sprite).expect("a small sprite")));
    }
    entries
}

/// The checkout's two files, registering `rows` sprites (0..rows) and giving
/// each sheet row `sheet_bytes(index)`.
pub(in crate::gen_rom_profile) fn checkout_files(
    rows: u16,
    sheet_bytes: impl Fn(u16) -> u16,
) -> [(&'static str, String); 2] {
    let mut constants = String::from("#define ANIM_SPRITES_START 10000\n\n");
    let mut sheets =
        String::from("const struct CompressedSpriteSheet gBattleAnimPicTable[] =\n{\n");
    let mut palettes =
        String::from("const struct CompressedSpritePalette gBattleAnimPaletteTable[] =\n{\n");
    for index in 0..rows {
        constants.push_str(&format!(
            "#define ANIM_TAG_T{index} (ANIM_SPRITES_START + {index})\n"
        ));
        sheets.push_str(&format!(
            "    {{gBattleAnimSpriteGfx_T{index}, {:#06x}, ANIM_TAG_T{index}}},\n",
            sheet_bytes(index)
        ));
        palettes.push_str(&format!(
            "    {{gBattleAnimSpritePal_T{index}, ANIM_TAG_T{index}}},\n"
        ));
    }
    sheets.push_str("};\n\n");
    palettes.push_str("};\n");
    [
        ("include/constants/battle_anim.h", constants),
        ("src/data/battle_anim.h", sheets + &palettes),
    ]
}

/// Run `body` over `rom`, a pack of `entries`, and a checkout of `files`.
pub(in crate::gen_rom_profile) fn with_checkout<T>(
    name: &str,
    rom: &RomFixture,
    entries: Vec<PackEntry>,
    files: &[(&str, String)],
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "pokeemerald-rs-gen-rom-profile-battle-effect-{name}-{}",
        std::process::id()
    ));
    let upstream = dir.join("checkout");
    for (relative, text) in files {
        let path = upstream.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("checkout directory");
        std::fs::write(path, text).expect("a checkout file");
    }
    let pack_path = dir.join("test.pack");
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    std::fs::write(&pack_path, writer.finish().expect("pack")).expect("write pack");
    let pack = PackSource::load(&pack_path).expect("load pack");
    let rom = rom.clone().finish();
    let ctx = Context {
        rom: &rom,
        pack: &pack,
        raw: RawSearch::new(&rom),
        lz77: Lz77Search::new(&rom),
        pointers: PointerIndex::build(&rom),
        upstream,
        bag_ids: Vec::new(),
    };
    let out = body(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    out
}
