use std::path::PathBuf;

use assets::pack::BattleEffectRecord;
use assets::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, AssetPack, BattleAnimationCatalog,
    BattleAnimationProgram, EffectSpriteId, ProgramId,
};
use pack_format::PackWriter;

use super::super::png::tests::indexed_png_from_raw_with_plte;
use super::super::png::IndexedImage;
use super::super::{extract_to, repo_root, upstream_present};
use super::{check_pixels, extract_battle_effects, parse_tags, ExtractError};

const RED: [u8; 3] = [248, 0, 0];
const GREEN: [u8; 3] = [0, 248, 0];
const BLUE: [u8; 3] = [0, 0, 248];
const RED_555: u16 = 0x001F;
const GREEN_555: u16 = 0x03E0;
const SPRITES: &str = "graphics/battle_anims/sprites";
/// A 4bpp 8x8 sheet.
const ONE_TILE: &str = "0x0020";

fn sprite(index: u16) -> EffectSpriteId {
    EffectSpriteId::new(index).unwrap()
}

/// A catalog whose only program loads `sprites`; every selector runs it.
fn catalog(sprites: &[u16]) -> BattleAnimationCatalog {
    let mut commands: Vec<AnimCommand> = sprites
        .iter()
        .map(|&index| AnimCommand::LoadSpriteGfx(sprite(index)))
        .collect();
    commands.push(AnimCommand::End);
    let program =
        BattleAnimationProgram::new(AnimSymbol::new("Move_TEST").unwrap(), commands).unwrap();
    let selectors = AnimationTable::ALL.into_iter().flat_map(|table| {
        (0..table.len()).map(move |index| {
            (
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(0),
            )
        })
    });
    BattleAnimationCatalog::new(vec![program], selectors).unwrap()
}

/// Packed PNG scanlines (filter byte 0), the first pixel in the high bits.
fn raw_rows(depth: u32, width: u32, height: u32, pixel: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    let per_byte = 8 / depth;
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0);
        for x0 in (0..width).step_by(per_byte as usize) {
            let mut byte = 0u8;
            for i in 0..per_byte {
                let x = x0 + i;
                let value = if x < width { pixel(x, y) } else { 0 };
                byte |= value << (8 - depth * (i + 1));
            }
            raw.push(byte);
        }
    }
    raw
}

fn plte(colors: usize, rgb: [u8; 3]) -> Vec<u8> {
    rgb.repeat(colors)
}

/// An indexed PNG whose pixel at `(x, y)` is `pixel(x, y)`.
fn png(
    depth: u32,
    width: u32,
    height: u32,
    colors: usize,
    rgb: [u8; 3],
    pixel: impl Fn(u32, u32) -> u8,
) -> Vec<u8> {
    indexed_png_from_raw_with_plte(
        u8::try_from(depth).unwrap(),
        width,
        height,
        &raw_rows(depth, width, height, pixel),
        &plte(colors, rgb),
    )
}

/// An 8x8 4-bit sheet with a 16-colour palette of `rgb`.
fn tile(rgb: [u8; 3], fill: u8) -> Vec<u8> {
    png(4, 8, 8, 16, rgb, |_, _| fill)
}

fn jasc(colors: usize, rgb: [u8; 3]) -> String {
    format!(
        "JASC-PAL\r\n0100\r\n{colors}\r\n{}",
        format!("{} {} {}\r\n", rgb[0], rgb[1], rgb[2]).repeat(colors)
    )
}

struct Fixture {
    dir: PathBuf,
    constants: String,
    pictures: Vec<String>,
    palettes: Vec<String>,
    declarations: Vec<String>,
}

fn picture_row(symbol: &str, size: &str, tag: &str) -> String {
    format!("    {{gBattleAnimSpriteGfx_{symbol}, {size}, ANIM_TAG_{tag}}},")
}

fn palette_row(symbol: &str, tag: &str) -> String {
    format!("    {{gBattleAnimSpritePal_{symbol}, ANIM_TAG_{tag}}},")
}

fn gfx_declaration(symbol: &str, path: &str, suffix: &str) -> String {
    format!("const u32 gBattleAnimSpriteGfx_{symbol}[] = INCGFX_U32(\"{path}\", \"{suffix}\");")
}

fn pal_declaration(symbol: &str, path: &str) -> String {
    format!("const u32 gBattleAnimSpritePal_{symbol}[] = INCGFX_U32(\"{path}\", \".gbapal.lz\");")
}

impl Fixture {
    /// Tags `A` (3) and `B` (11) joined to `alpha.png` (red palette) and
    /// `beta.png` (green palette); `DEAD` (40) is registered but has no sources.
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-battle-effect-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(SPRITES)).unwrap();
        std::fs::create_dir_all(dir.join("include/constants")).unwrap();
        std::fs::create_dir_all(dir.join("src/data")).unwrap();
        let fixture = Self {
            dir,
            constants: "// Particle ids.\n#define ANIM_SPRITES_START 10000\n\n\
                #define ANIM_TAG_A (ANIM_SPRITES_START + 3)\n\
                #define ANIM_TAG_B                (ANIM_SPRITES_START + 0xB) // unused\n\
                #define ANIM_TAG_DEAD (ANIM_SPRITES_START + 40)\n"
                .to_owned(),
            pictures: vec![
                picture_row("Beta", ONE_TILE, "B"),
                picture_row("Dead", ONE_TILE, "DEAD"),
                picture_row("Alpha", ONE_TILE, "A"),
            ],
            palettes: vec![
                palette_row("BetaPal", "B"),
                palette_row("DeadPal", "DEAD"),
                palette_row("AlphaPal", "A"),
            ],
            declarations: vec![
                gfx_declaration("Alpha", &format!("{SPRITES}/alpha.png"), ".4bpp.lz"),
                pal_declaration("AlphaPal", &format!("{SPRITES}/alpha.png")),
                gfx_declaration("Beta", &format!("{SPRITES}/beta.png"), ".4bpp.lz"),
                pal_declaration("BetaPal", &format!("{SPRITES}/beta.png")),
                gfx_declaration("Unrelated", &format!("{SPRITES}/missing.png"), ".4bpp.lz"),
            ],
        };
        fixture.write("alpha.png", &tile(RED, 1));
        fixture.write("beta.png", &tile(GREEN, 2));
        fixture
    }

    /// Writes a file under `graphics/battle_anims/sprites`.
    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.dir.join(SPRITES).join(name), bytes).unwrap();
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(SPRITES).join(name)
    }

    /// Writes the three text sources from the current rows.
    fn flush(&self) {
        let table = |kind: &str, array: &str, rows: &[String]| {
            format!(
                "const struct OamData gOamData_Unrelated = {{ 0 }};\n\n\
                 const struct {kind} {array}[] =\n{{\n{}\n}};\n\n\
                 const struct Other gOther[] =\n{{\n}};\n",
                rows.join("\n")
            )
        };
        let tables = format!(
            "{}\n{}",
            table(
                "CompressedSpriteSheet",
                "gBattleAnimPicTable",
                &self.pictures
            ),
            table(
                "CompressedSpritePalette",
                "gBattleAnimPaletteTable",
                &self.palettes
            )
        );
        std::fs::write(self.dir.join("src/data/battle_anim.h"), tables).unwrap();
        std::fs::write(
            self.dir.join("src/graphics.c"),
            self.declarations.join("\n") + "\n",
        )
        .unwrap();
        std::fs::write(
            self.dir.join("include/constants/battle_anim.h"),
            &self.constants,
        )
        .unwrap();
    }

    /// Runs the extractor for `sprites` into a writer holding one sentinel.
    fn extract(&self, sprites: &[u16]) -> (Result<(), ExtractError>, PackWriter) {
        self.flush();
        let mut writer = PackWriter::new();
        writer.push(pack_format::raw_entry("sentinel".to_owned(), vec![1]));
        let result = extract_battle_effects(&self.dir, &catalog(sprites), &mut writer);
        (result, writer)
    }

    /// Extracts and expects success, loading the finished pack.
    fn extracted(&self, sprites: &[u16]) -> AssetPack {
        let (result, writer) = self.extract(sprites);
        result.unwrap();
        assert_eq!(writer.len(), 1 + 2 * sprites.len());
        let path = self.dir.join("test.pack");
        std::fs::write(&path, writer.finish().unwrap()).unwrap();
        AssetPack::load(&path).unwrap()
    }

    /// Extracts and expects failure with only the sentinel queued.
    fn rejected(&self, sprites: &[u16]) -> ExtractError {
        let (result, writer) = self.extract(sprites);
        assert_eq!(writer.len(), 1, "a failure queues no battle-effect entry");
        result.expect_err("the checkout is malformed")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn source_detail(error: ExtractError) -> (PathBuf, usize, String) {
    match error {
        ExtractError::BattleEffectSource { path, line, detail } => (path, line, detail),
        other => panic!("expected a source fault, got {other:?}"),
    }
}

fn mismatch_detail(error: ExtractError) -> (PathBuf, String) {
    match error {
        ExtractError::BattleEffectAssetMismatch { path, detail } => (path, detail),
        other => panic!("expected an asset mismatch, got {other:?}"),
    }
}

#[test]
fn a_valid_join_emits_each_tags_sheet_and_selected_palette() {
    let mut fixture = Fixture::new("valid");
    // The palette table selects beta's palette for A, not A's own sheet palette.
    fixture.palettes[2] = palette_row("BetaPal", "A");
    fixture.write(
        "alpha.png",
        &png(4, 8, 8, 16, RED, |x, y| u8::try_from((x + y) % 16).unwrap()),
    );
    let pack = fixture.extracted(&[11, 3]);

    let a = pack.battle_effect(sprite(3)).unwrap().art;
    assert_eq!(
        (a.image.width, a.image.height, a.image.bit_depth),
        (8, 8, 4)
    );
    let expected: Vec<u8> = (0..8u32)
        .flat_map(|y| (0..8u32).map(move |x| u8::try_from((x + y) % 16).unwrap()))
        .collect();
    assert_eq!(a.image.pixels, expected);
    assert_eq!(
        (a.palette.color_count, a.palette.color(0)),
        (16, Some(GREEN_555))
    );
    let b = pack.battle_effect(sprite(11)).unwrap().art;
    assert_eq!(b.image.pixels, [2; 64]);
    assert_eq!(b.palette.color(15), Some(GREEN_555));
}

#[test]
fn a_jasc_palette_keeps_every_bank() {
    let mut fixture = Fixture::new("jasc");
    fixture.declarations[1] = pal_declaration("AlphaPal", &format!("{SPRITES}/alpha.pal"));
    fixture.declarations[3] = pal_declaration("BetaPal", &format!("{SPRITES}/beta.pal"));
    std::fs::write(fixture.path("alpha.pal"), jasc(16, BLUE)).unwrap();
    std::fs::write(fixture.path("beta.pal"), jasc(80, RED)).unwrap();
    let pack = fixture.extracted(&[3, 11]);

    let a = pack.battle_effect(sprite(3)).unwrap().art.palette;
    assert_eq!((a.color_count, a.color(0)), (16, Some(0x7C00)));
    let b = pack.battle_effect(sprite(11)).unwrap().art.palette;
    assert_eq!((b.color_count, b.color(79)), (80, Some(RED_555)));
}

#[test]
fn a_shared_payload_is_emitted_once_per_referencing_tag() {
    let mut fixture = Fixture::new("shared");
    // B reuses A's sheet but selects its own palette.
    fixture.pictures[0] = picture_row("Alpha", ONE_TILE, "B");
    let pack = fixture.extracted(&[3, 11]);

    let a = pack.battle_effect(sprite(3)).unwrap().art;
    let b = pack.battle_effect(sprite(11)).unwrap().art;
    assert_eq!(a.image.pixels, b.image.pixels);
    assert_eq!(a.image.pixels, [1; 64]);
    assert_eq!(a.palette.color(0), Some(RED_555));
    assert_eq!(b.palette.color(0), Some(GREEN_555));
    for id in [
        "battle-effect/003/image",
        "battle-effect/011/image",
        "battle-effect/003/palette",
        "battle-effect/011/palette",
    ] {
        assert!(pack.entries().any(|entry| entry.id == id), "{id}");
    }
}

#[test]
fn unreachable_registrations_are_never_read() {
    let mut fixture = Fixture::new("unreachable");
    // DEAD has no declarations or files, and its palette is registered twice.
    fixture.palettes.push(palette_row("DeadPal", "DEAD"));
    fixture.extracted(&[3]);
}

#[test]
fn a_tag_missing_from_either_table_is_rejected() {
    let mut fixture = Fixture::new("missing-row");
    fixture.pictures.remove(2);
    let (path, _, detail) = source_detail(fixture.rejected(&[3, 11]));
    assert!(path.ends_with("src/data/battle_anim.h"));
    assert!(detail.contains("gBattleAnimPicTable") && detail.contains("sprite 3"));

    let mut fixture = Fixture::new("missing-palette-row");
    fixture.palettes.remove(2);
    let (_, _, detail) = source_detail(fixture.rejected(&[3, 11]));
    assert!(detail.contains("gBattleAnimPaletteTable") && detail.contains("sprite 3"));
}

#[test]
fn a_selected_tag_registered_twice_is_rejected_even_when_identical() {
    let mut fixture = Fixture::new("duplicate-row");
    fixture.palettes.push(palette_row("AlphaPal", "A"));
    let (_, line, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("twice") && line > 0, "{line}: {detail}");

    let mut fixture = Fixture::new("duplicate-pic-row");
    fixture.pictures.push(picture_row("Alpha", ONE_TILE, "A"));
    let (_, _, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("gBattleAnimPicTable"), "{detail}");
}

#[test]
fn rows_join_by_tag_not_by_position_or_symbol_name() {
    let mut fixture = Fixture::new("order");
    fixture.pictures.reverse();
    fixture.palettes.reverse();
    let pack = fixture.extracted(&[3, 11]);
    assert_eq!(
        pack.battle_effect(sprite(3)).unwrap().art.image.pixels,
        [1; 64]
    );
    assert_eq!(
        pack.battle_effect(sprite(11)).unwrap().art.image.pixels,
        [2; 64]
    );
}

#[test]
fn a_missing_or_repeated_declaration_is_rejected() {
    for index in [0, 1] {
        let mut fixture = Fixture::new("missing-declaration");
        let removed = fixture.declarations.remove(index);
        let symbol = removed.split('[').next().unwrap().replace("const u32 ", "");
        let (path, _, detail) = source_detail(fixture.rejected(&[3]));
        assert!(path.ends_with("src/graphics.c"));
        assert!(detail.contains(&symbol), "{detail}");

        let mut fixture = Fixture::new("repeated-declaration");
        fixture.declarations.push(removed.clone());
        fixture.declarations.push(removed);
        let (_, line, detail) = source_detail(fixture.rejected(&[3]));
        assert!(detail.contains("twice") && line > 0, "{detail}");
    }
}

#[test]
fn a_missing_source_file_is_rejected() {
    for name in ["alpha.png", "beta.png"] {
        let fixture = Fixture::new("missing-file");
        std::fs::remove_file(fixture.path(name)).unwrap();
        match fixture.rejected(&[3, 11]) {
            ExtractError::ReadFailed(path, _) => assert_eq!(path, fixture.path(name)),
            other => panic!("wrong error {other:?}"),
        }
    }
}

#[test]
fn a_missing_palette_file_is_rejected() {
    let mut fixture = Fixture::new("missing-palette");
    fixture.declarations[1] = pal_declaration("AlphaPal", &format!("{SPRITES}/alpha.pal"));
    match fixture.rejected(&[3]) {
        ExtractError::ReadFailed(path, _) => assert_eq!(path, fixture.path("alpha.pal")),
        other => panic!("wrong error {other:?}"),
    }
}

#[test]
fn undecodable_sources_are_rejected() {
    let fixture = Fixture::new("undecodable-png");
    fixture.write("alpha.png", b"not a png");
    assert!(
        matches!(fixture.rejected(&[3]), ExtractError::Png(path, _) if path == fixture.path("alpha.png"))
    );

    let mut fixture = Fixture::new("undecodable-pal");
    fixture.declarations[1] = pal_declaration("AlphaPal", &format!("{SPRITES}/alpha.pal"));
    std::fs::write(fixture.path("alpha.pal"), "JASC-PAL\r\n0100\r\n0\r\n").unwrap();
    assert!(
        matches!(fixture.rejected(&[3]), ExtractError::Pal(path, _) if path == fixture.path("alpha.pal"))
    );
}

#[test]
fn a_sheet_of_the_wrong_shape_is_rejected_without_a_partial_family() {
    // Alpha is valid and queued first; beta's failure still empties the family.
    let cases: [(Vec<u8>, &str); 3] = [
        (png(4, 12, 8, 16, RED, |_, _| 0), "12x8"),
        (png(2, 8, 8, 4, RED, |_, _| 0), "2-bit"),
        (png(8, 8, 8, 16, RED, |_, _| 0), "8-bit"),
    ];
    for (bytes, expected) in cases {
        let fixture = Fixture::new("shape");
        fixture.write("beta.png", &bytes);
        let (path, detail) = mismatch_detail(fixture.rejected(&[3, 11]));
        assert_eq!(path, fixture.path("beta.png"));
        assert!(detail.contains(expected), "{detail}");
    }
}

#[test]
fn a_palette_that_is_not_whole_banks_is_rejected() {
    for colors in [3, 17] {
        let fixture = Fixture::new("bank-png");
        fixture.write("beta.png", &png(4, 8, 8, colors, GREEN, |_, _| 2));
        let (_, detail) = mismatch_detail(fixture.rejected(&[3, 11]));
        assert!(detail.contains(&format!("{colors} colours")), "{detail}");
    }
    let mut fixture = Fixture::new("bank-jasc");
    fixture.declarations[1] = pal_declaration("AlphaPal", &format!("{SPRITES}/alpha.pal"));
    std::fs::write(fixture.path("alpha.pal"), jasc(17, RED)).unwrap();
    let (path, _) = mismatch_detail(fixture.rejected(&[3]));
    assert_eq!(path, fixture.path("alpha.pal"));
}

#[test]
fn a_palette_only_png_may_be_8bit_with_many_banks() {
    let mut fixture = Fixture::new("palette-only-8bit");
    fixture.write("wide.png", &png(8, 8, 8, 32, BLUE, |_, _| 2));
    fixture.declarations[1] = pal_declaration("AlphaPal", &format!("{SPRITES}/wide.png"));
    let pack = fixture.extracted(&[3]);
    let art = pack.battle_effect(sprite(3)).unwrap().art;
    assert_eq!((art.palette.color_count, art.image.bit_depth), (32, 4));
}

#[test]
fn a_pixel_outside_the_palette_or_the_bit_depth_is_rejected() {
    let image = |pixels: Vec<u8>| IndexedImage {
        width: 8,
        height: 8,
        bit_depth: 4,
        pixels,
        palette: Vec::new(),
    };
    let path = std::path::Path::new("sheet.png");
    let mut pixels = vec![0; 64];
    assert!(check_pixels(path, &image(pixels.clone()), 16).is_ok());
    pixels[5] = 16;
    assert!(check_pixels(path, &image(pixels.clone()), 80).is_err());
    pixels[5] = 15;
    assert!(check_pixels(path, &image(pixels), 8).is_err());
}

#[test]
fn an_unsupported_declaration_or_path_is_rejected() {
    let mut fixture = Fixture::new("declaration-kind");
    fixture.declarations[0] = gfx_declaration("Alpha", &format!("{SPRITES}/alpha.png"), ".lz");
    let (_, line, detail) = source_detail(fixture.rejected(&[3]));
    assert_eq!(line, 1, "{detail}");

    let mut fixture = Fixture::new("declaration-escape");
    fixture.declarations[0] =
        gfx_declaration("Alpha", "graphics/battle_anims/../alpha.png", ".4bpp.lz");
    let (_, _, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("not a path under"), "{detail}");

    let mut fixture = Fixture::new("declaration-unknown-stream");
    fixture.declarations[0] = gfx_declaration("Alpha", &format!("{SPRITES}/other.4bpp"), ".lz");
    let (_, _, detail) = source_detail(fixture.rejected(&[3]));
    assert!(
        detail.contains("not a known concatenated sheet"),
        "{detail}"
    );

    let mut fixture = Fixture::new("declaration-garbled");
    fixture.declarations[0] = "const u32 gBattleAnimSpriteGfx_Alpha[] = INCBIN_U32(\"x\");".into();
    let (_, line, _) = source_detail(fixture.rejected(&[3]));
    assert_eq!(line, 1);
}

#[test]
fn a_malformed_table_or_constant_is_rejected() {
    let mut fixture = Fixture::new("unknown-tag");
    fixture.pictures[1] = picture_row("Dead", ONE_TILE, "NOWHERE");
    let (_, line, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("ANIM_TAG_NOWHERE") && line > 0, "{detail}");

    let mut fixture = Fixture::new("short-row");
    fixture.palettes[1] = "    {gBattleAnimSpritePal_DeadPal},".to_owned();
    let (_, _, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("expected 2"), "{detail}");

    let mut fixture = Fixture::new("size");
    fixture.pictures[1] = picture_row("Dead", "BIG", "DEAD");
    let (_, _, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("not an integer"), "{detail}");

    let mut fixture = Fixture::new("constant");
    fixture.constants += "#define ANIM_TAG_BAD (ANIM_SPRITES_START - 2)\n";
    let (path, line, detail) = source_detail(fixture.rejected(&[3]));
    assert!(
        path.ends_with("include/constants/battle_anim.h") && line == 7,
        "{line}"
    );
    assert!(detail.contains("ANIM_TAG_BAD"));

    let mut fixture = Fixture::new("out-of-range");
    fixture.constants += "#define ANIM_TAG_BIG (ANIM_SPRITES_START + 9999)\n";
    let (_, _, detail) = source_detail(fixture.rejected(&[3]));
    assert!(detail.contains("effect sprite range"), "{detail}");

    let mut fixture = Fixture::new("no-base");
    fixture.constants = fixture
        .constants
        .replace("#define ANIM_SPRITES_START 10000", "");
    fixture.flush();
    let (_, line, detail) = source_detail(parse_tags(&fixture.dir).unwrap_err());
    assert_eq!(line, 0);
    assert!(detail.contains("ANIM_SPRITES_START"));

    let fixture = Fixture::new("unclosed");
    fixture.flush();
    let tables = fixture.dir.join("src/data/battle_anim.h");
    std::fs::write(
        &tables,
        "const struct CompressedSpriteSheet gBattleAnimPicTable[] =\n{\n",
    )
    .unwrap();
    let mut writer = PackWriter::new();
    let error = extract_battle_effects(&fixture.dir, &catalog(&[3]), &mut writer).unwrap_err();
    assert!(source_detail(error).2.contains("not closed"));
    assert_eq!(writer.len(), 0);
}

/// Registers `spark.4bpp` for tag A with a table size of `size`.
fn composite(name: &str, size: &str) -> Fixture {
    let mut fixture = Fixture::new(name);
    fixture.pictures[2] = picture_row("Spark", size, "A");
    fixture.declarations[0] = gfx_declaration("Spark", &format!("{SPRITES}/spark.4bpp"), ".lz");
    // Part 0 is 16x16 with tiles 1, 2 / 3, 4 in raster order; part 1 is one tile.
    fixture.write(
        "spark_0.png",
        &png(4, 16, 16, 16, RED, |x, y| match (x / 8, y / 8) {
            (0, 0) => 1,
            (1, 0) => 2,
            (0, 1) => 3,
            _ => 4,
        }),
    );
    fixture.write("spark_1.png", &png(4, 8, 8, 16, RED, |_, _| 5));
    // `spark_2.png` is the Spark2 sheet, not a part of Spark.
    fixture.write("spark_2.png", b"unrelated and undecodable");
    fixture
}

#[test]
fn a_concatenated_sheet_is_the_tile_stream_of_its_rule_parts() {
    let fixture = composite("composite", "0x00A0");
    let pack = fixture.extracted(&[3]);
    let image = pack.battle_effect(sprite(3)).unwrap().art.image;
    assert_eq!((image.width, image.height), (8, 40));
    let blocks: Vec<u8> = image.pixels.chunks(64).map(|block| block[0]).collect();
    assert_eq!(blocks, [1, 2, 3, 4, 5]);
    assert!(image
        .pixels
        .chunks(64)
        .all(|b| b.iter().all(|&p| p == b[0])));
}

#[test]
fn a_concatenated_sheet_must_match_its_registered_size() {
    let fixture = composite("composite-size", "0x0080");
    let (path, detail) = mismatch_detail(fixture.rejected(&[3]));
    assert_eq!(path, fixture.path("spark.4bpp"));
    assert!(
        detail.contains("160 bytes") && detail.contains("128"),
        "{detail}"
    );
}

#[test]
fn a_single_png_keeps_its_dimensions_whatever_the_registered_size() {
    let mut fixture = Fixture::new("exception");
    fixture.write("alpha.png", &png(4, 16, 16, 16, RED, |_, _| 7));
    fixture.pictures[2] = picture_row("Alpha", "0x0100", "A");
    let image = fixture
        .extracted(&[3])
        .battle_effect(sprite(3))
        .unwrap()
        .art
        .image
        .pixels
        .len();
    assert_eq!(image, 256);
}

#[test]
fn an_empty_inventory_emits_nothing() {
    let fixture = Fixture::new("empty");
    fixture.extracted(&[]);
}

#[test]
fn error_messages_name_the_file_line_and_detail() {
    let source = ExtractError::BattleEffectSource {
        path: PathBuf::from("src/graphics.c"),
        line: 12,
        detail: "bad".to_owned(),
    };
    assert_eq!(
        source.to_string(),
        "battle-effect source `src/graphics.c` line 12: bad"
    );
    let asset = ExtractError::BattleEffectAssetMismatch {
        path: PathBuf::from("a.png"),
        detail: "is 12x8".to_owned(),
    };
    assert_eq!(asset.to_string(), "battle-effect asset `a.png` is 12x8");
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_yields_every_inventory_pair() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-battle-effect-{}.pack",
        std::process::id()
    ));
    extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let catalog = pack.battle_animation_catalog().unwrap();
    let records: Vec<BattleEffectRecord> = BattleEffectRecord::inventory(&catalog).collect();
    // 192 distinct `loadspritegfx` operands plus `ANIM_TAG_POKEBLOCK`.
    assert_eq!(records.len(), 193);
    let ids: Vec<String> = pack
        .entries()
        .filter(|entry| entry.id.starts_with("battle-effect/"))
        .map(|entry| entry.id.clone())
        .collect();
    assert_eq!(ids.len(), 2 * records.len());
    for record in &records {
        assert!(ids.contains(&record.image_id()) && ids.contains(&record.palette_id()));
        pack.battle_effect(record.sprite())
            .unwrap_or_else(|e| panic!("{}: {e}", record.image_id()));
    }
    let upstream = repo_root().join("pokeemerald/graphics/battle_anims/sprites");
    let art = |index| pack.battle_effect(sprite(index)).unwrap().art;
    let decoded =
        |name: &str| super::png::decode(&std::fs::read(upstream.join(name)).unwrap()).unwrap();

    // A single PNG keeps its shape and pixels, even where the table registers
    // another size (`bent_spoon`, `eye_sparkle`, `sparkle_6`).
    for (index, name) in [
        (97, "bent_spoon.png"),
        (218, "eye_sparkle.png"),
        (271, "sparkle_6.png"),
    ] {
        let source = decoded(name);
        let image = art(index).image;
        assert_eq!(
            (image.width, image.height),
            (source.width, source.height),
            "{name}"
        );
        assert_eq!(image.pixels, source.pixels.as_slice(), "{name}");
    }

    // The five concatenated sheets are one tile wide, in stream order.
    for (index, height) in [(10, 1152), (141, 112), (74, 40), (159, 40), (1, 192)] {
        let image = art(index).image;
        assert_eq!((image.width, image.height), (8, height), "sprite {index}");
    }
    // The last tile of the stream is the last tile of `ice_cube_3.png` (32x32).
    let part = decoded("ice_cube_3.png");
    let last_tile: Vec<u8> = (24..32)
        .flat_map(|y| part.pixels[y * 32 + 24..y * 32 + 32].to_vec())
        .collect();
    assert!(art(10).image.pixels.ends_with(&last_tile));

    // One sheet backs three tags, each with its own palette.
    let (gray, blue, red) = (art(235), art(236), art(237));
    assert_eq!(gray.image.pixels, blue.image.pixels);
    assert_eq!(gray.image.pixels, red.image.pixels);
    assert_ne!(gray.palette.color(1), blue.palette.color(1));
    assert_ne!(blue.palette.color(1), red.palette.color(1));

    // `MusicNotes2` keeps its five banks.
    assert_eq!(art(206).palette.color_count, 80);
    let _ = std::fs::remove_file(path);
}
