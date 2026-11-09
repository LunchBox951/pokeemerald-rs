//! The equivalence gate: the ROM backend's pack against the checkout's.
//!
//! Discussion #71's policy A and policy C promise the same pack from the
//! same game. Nothing else in the tree can check that promise: the two
//! backends read completely different inputs, and only a real ROM and a
//! real `cargo xtask extract` pack together prove they agree.
//!
//! So this test is `#[ignore]`d and needs both:
//!
//! ```text
//! POKEEMERALD_ROM=/path/to/pokeemerald.gba cargo test -p rom-import -- --ignored
//! ```
//!
//! The checkout side is read from `cargo xtask extract`'s own fixed
//! destination, `<repo root>/assets-pack/pokeemerald.pack`, and never
//! through runtime pack resolution — see [`checkout_pack_path`].
//!
//! It skips with a printed reason when either is missing, rather than
//! failing: a contributor without a ROM must still be able to run
//! `--ignored` `(gated-by-default)`.
//!
//! # What it compares
//!
//! Whole directories, not a hand-listed set of ids. The ROM backend's pack
//! is *partial* until every domain lands, so the comparison is scoped two
//! ways:
//!
//! - Every id the ROM backend wrote must exist in the checkout pack, with
//!   the same kind, the same metadata, and the same payload bytes.
//! - Every checkout id under a prefix in [`COMPLETE_PREFIXES`] must exist in
//!   the ROM backend's pack, so a domain that silently drops a root fails
//!   here instead of shipping a pack with a hole in it. A domain slice adds
//!   its prefix when it lands.
//!
//! The species cries are under the complete `audio/` prefix like every other
//! sample: the table and the samples it names are required of the ROM backend,
//! and [`the_cries_are_named_by_id`] pins them by the table's own contents so
//! a cry both backends drop together still fails.
//! Every `audio/voicegroup/` and `audio/sample/` id is compared; the songs are
//! pinned by [`ROM_IMPORTED_SONG_IDS`], not claimed by prefix.
//!
//! [`REVIEWED_DIFFERENCES`] is the escape hatch for an id the two backends
//! genuinely cannot agree on. It is empty, and the intent is that it stays
//! empty: `title/palette/pokemon_logo` was the one candidate, and it was
//! resolved by making both backends honour upstream's own `-num_colors`
//! cut rather than by writing the difference down here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use assets::audio::{CRY_TABLE_LEN, CRY_TABLE_PACK_ID};
use assets::{AssetPack, CryTable, SampleId, Song, VoiceEntry, VoiceGroup};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, OUTPUT_RELATIVE_PATH};
use rom_import::{
    FIELD_MESSAGE_PACK_ID, TRAINER_CLASS_NAME_PACK_ID, TRAINER_DEFEAT_SPEECH_PACK_ID,
};

/// The environment variable naming the ROM to import.
const ROM_ENV: &str = "POKEEMERALD_ROM";

/// Id prefixes the ROM backend claims to cover completely.
///
/// A checkout id under one of these with no ROM counterpart is a failure.
/// Anything outside them is a domain that has not landed yet, and is
/// counted but not compared.
const COMPLETE_PREFIXES: &[&str] = &[
    "title/",
    "interface/",
    "tileset/",
    "layout/",
    "sprite/",
    "font/",
    "text-window/",
    "audio/voicegroup/",
    "audio/sample/",
    "trainer-pic/",
    "trainer/",
    "battle-message/",
    "field-message/",
    "trainer-defeat-speech/",
    "trainer-class-name/",
    "battle-anim/",
    "bag/",
    "starter-chooser/",
    "pokemon/",
    "opening-scene/",
    "evolution-scene/",
    "poke-ball/",
    "naming-screen/",
];

/// The text printer's page-prompt arrow, by id: `interface/` is a complete
/// prefix, but a sheet both backends drop together would pass it.
const DOWN_ARROW_IDS: [&str; 1] = ["interface/image/down_arrow"];

/// The species whose static battler art both backends must carry: the
/// starters, then the checkpoint's other eleven Pokémon.
const BATTLER_SPECIES: [&str; 14] = [
    "treecko",
    "torchic",
    "mudkip",
    "poochyena",
    "mightyena",
    "wurmple",
    "silcoon",
    "beautifly",
    "cascoon",
    "dustox",
    "zigzagoon",
    "linoone",
    "wingull",
    "pelipper",
];

/// The only song the ROM backend imports, by id: its song root is the profile's lone
/// `mus_title` entry. The checkout pack also carries every other checkpoint song, so
/// `audio/song/` is not in [`COMPLETE_PREFIXES`]; a dropped title still fails by this id.
const ROM_IMPORTED_SONG_IDS: [&str; 1] = ["audio/song/mus_title"];

/// Whether the ROM backend claims every checkout entry with this id.
fn is_claimed_complete(id: &str) -> bool {
    COMPLETE_PREFIXES
        .iter()
        .any(|prefix| id.starts_with(prefix))
}

/// The battle textbox and menu text entries, the thirteen health-box elements
/// in `gHealthboxElementsGfxTable` order, the battle bar, the singles
/// health-box frames, the enemy shadow, and the two ball palettes: 24 ids the
/// ROM backend must write although `battle-interface/` is not in
/// [`COMPLETE_PREFIXES`]. They are every `battle-interface/` entry the checkout
/// extracts, so a narrow pin of these ids stands in for the prefix.
const BATTLE_INTERFACE_IDS: [&str; 24] = [
    "battle-interface/image/textbox",
    "battle-interface/palette/textbox",
    "battle-interface/palette/window_text",
    "battle-interface/palette/pp_text",
    "battle-interface/raw/textbox_map",
    "battle-interface/raw/healthbox_element/hpbar",
    "battle-interface/raw/healthbox_element/expbar",
    "battle-interface/raw/healthbox_element/status",
    "battle-interface/raw/healthbox_element/misc",
    "battle-interface/raw/healthbox_element/hpbar_anim",
    "battle-interface/raw/healthbox_element/misc_frameend",
    "battle-interface/raw/healthbox_element/ball_display",
    "battle-interface/raw/healthbox_element/ball_caught_indicator",
    "battle-interface/raw/healthbox_element/status2",
    "battle-interface/raw/healthbox_element/status3",
    "battle-interface/raw/healthbox_element/status4",
    "battle-interface/raw/healthbox_element/healthbox_doubles_frameend",
    "battle-interface/raw/healthbox_element/healthbox_doubles_frameend_bar",
    "battle-interface/raw/battle_bar",
    "battle-interface/image/healthbox_singles_player",
    "battle-interface/image/healthbox_singles_opponent",
    "battle-interface/image/enemy_mon_shadow",
    "battle-interface/palette/ball_status_bar",
    "battle-interface/palette/ball_display",
];

/// Every battler entry, named by id so entries both backends drop together
/// still fail: front, back, then normal and shiny palette per species.
fn battler_ids() -> Vec<String> {
    BATTLER_SPECIES
        .iter()
        .flat_map(|name| {
            [
                format!("pokemon/{name}/front"),
                format!("pokemon/{name}/back"),
                format!("pokemon/{name}/palette/normal"),
                format!("pokemon/{name}/palette/shiny"),
            ]
        })
        .collect()
}

/// The layout names the checkpoint maps added beyond the Littleroot, Route 101,
/// Oldale, and Route 103 grids: `MAP_INSIDE_OF_TRUCK` and the Oldale interiors.
const CHECKPOINT_LAYOUTS: [&str; 6] = [
    "inside_of_truck",
    "house1",
    "house2",
    "mart",
    "pokemon_center_1f",
    "pokemon_center_2f",
];

/// The tilesets those layouts and their neighbours draw with.
const CHECKPOINT_TILESETS: [&str; 4] = [
    "generic_building",
    "pokemon_center",
    "shop",
    "inside_of_truck",
];

/// The non-people object-event pictures and palettes the checkpoint maps'
/// fixed objects render.
const CHECKPOINT_OBJECT_IDS: [&str; 17] = [
    "sprite/berry_trees/cheri",
    "sprite/berry_trees/dirt_pile",
    "sprite/berry_trees/leppa",
    "sprite/berry_trees/sprout",
    "sprite/dolls/pichu_doll",
    "sprite/dolls/swablu_doll",
    "sprite/misc/birchs_bag",
    "sprite/misc/cuttable_tree",
    "sprite/misc/item_ball",
    "sprite/misc/moving_box",
    "sprite/misc/truck",
    "sprite/pokemon/enemy_zigzagoon",
    "sprite/pokemon/vigoroth",
    "sprite/palette/enemy_zigzagoon",
    "sprite/palette/moving_box",
    "sprite/palette/truck",
    "sprite/palette/vigoroth",
];

/// Every pack id the checkpoint maps' payloads add: each new layout's grid and
/// border, each new tileset's tiles, metatile tables, and sixteen palette
/// banks, and the object pictures and palettes.
fn checkpoint_payload_ids() -> Vec<String> {
    let layouts = CHECKPOINT_LAYOUTS.iter().flat_map(|name| {
        [
            format!("layout/{name}/map"),
            format!("layout/{name}/border"),
        ]
    });
    let tilesets = CHECKPOINT_TILESETS.iter().flat_map(|name| {
        ["tiles", "metatiles", "metatile-attributes"]
            .map(|part| format!("tileset/{name}/{part}"))
            .into_iter()
            .chain((0..16).map(move |bank| format!("tileset/{name}/palette/{bank:02}")))
    });
    layouts
        .chain(tilesets)
        .chain(CHECKPOINT_OBJECT_IDS.map(str::to_owned))
        .collect()
}

/// The evolution scene's seven pack ids. A prefix cannot catch both backends
/// dropping the same constituent, so each is named.
const EVOLUTION_SCENE_IDS: [&str; 7] = [
    "evolution-scene/image/bg",
    "evolution-scene/image/evo_sparkle",
    "evolution-scene/palette/bg_anim_intro",
    "evolution-scene/palette/bg_anim",
    "evolution-scene/palette/evo_sparkle",
    "evolution-scene/raw/bg_inner",
    "evolution-scene/raw/bg_outer",
];

/// The Birch-speech portraits: the front image and palette of Brendan and of
/// May. Named by id so portraits both backends drop together still fail.
const TRAINER_PORTRAIT_IDS: [&str; 4] = [
    "trainer-pic/brendan/front",
    "trainer-pic/brendan/palette",
    "trainer-pic/may/front",
    "trainer-pic/may/palette",
];

/// The Brendan and May trainer battlers: the front image, back sheet and
/// shared palette of each. Named by id so entries both backends drop together
/// still fail.
const TRAINER_BATTLER_IDS: [&str; 6] = [
    "trainer/brendan/front",
    "trainer/brendan/back",
    "trainer/brendan/palette",
    "trainer/may/front",
    "trainer/may/back",
    "trainer/may/palette",
];

/// `field-message/table` as both backends must write it: schema header, then
/// `gText_PkmnFainted_FldPsn` (`{STR_VAR_1} fainted…\p\n`, `EOS`) spelled
/// through `charmap.txt`.
const FIELD_MESSAGE_TABLE: [u8; 28] = [
    b'F', b'M', b'S', b'G', 1, 0, 1, 0, 0, 0, 14, 0, 0, 0, 0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD,
    0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF,
];

/// Each `Route103_Text_*Defeated` string as both backends must write it, in
/// `TrainerDefeatSpeechContext::ALL` order, spelled by hand through
/// `charmap.txt`: `…` is `B0`, `\n` is `FE`, `{PLAYER}` is `FD 01`, `{KUN}` is
/// `FD 05`, and the `$` is `EOS` (`FF`). The Amy, Liv, and rematch speeches are
/// four distinct strings of the one `TRAINER_AMY_AND_LIV_1`.
const TRAINER_DEFEAT_SPEECHES: [&[u8]; 14] = [
    // Route103_Text_DaisyDefeated
    &[
        0xD3, 0xE3, 0xE9, 0x00, 0xEB, 0xD9, 0xE6, 0xD9, 0xE2, 0xB4, 0xE8, 0x00, 0xE0, 0xD9, 0xD8,
        0x00, 0xD5, 0xE7, 0xE8, 0xE6, 0xD5, 0xED, 0x00, 0xD6, 0xED, 0x00, 0xE3, 0xE9, 0xE6, 0x00,
        0xD5, 0xE6, 0xE3, 0xE1, 0xD5, 0xB0, 0xFF,
    ],
    // Route103_Text_AmyDefeated
    &[
        0xBB, 0xC7, 0xD3, 0xF0, 0x00, 0xCF, 0xDC, 0xAE, 0xE3, 0xDC, 0xB8, 0x00, 0xEB, 0xD9, 0x00,
        0xE0, 0xE3, 0xE7, 0xE8, 0xAD, 0xFF,
    ],
    // Route103_Text_LivDefeated
    &[
        0xC6, 0xC3, 0xD0, 0xF0, 0x00, 0xC9, 0xDC, 0xB8, 0x00, 0xEB, 0xD9, 0x00, 0xE0, 0xE3, 0xE7,
        0xE8, 0xB8, 0x00, 0xD6, 0xDD, 0xDB, 0x00, 0xE7, 0xDD, 0xE7, 0xE8, 0xD9, 0xE6, 0xB0, 0xFF,
    ],
    // Route103_Text_AmyRematchDefeated
    &[
        0xBB, 0xC7, 0xD3, 0xF0, 0x00, 0xBB, 0xEB, 0xEB, 0xB8, 0x00, 0xD6, 0xE3, 0xE3, 0xAB, 0xFE,
        0xD1, 0xD9, 0x00, 0xD7, 0xE3, 0xE9, 0xE0, 0xD8, 0xE2, 0xB4, 0xE8, 0x00, 0xEB, 0xDD, 0xE2,
        0x00, 0xD5, 0xDB, 0xD5, 0xDD, 0xE2, 0xB0, 0xFF,
    ],
    // Route103_Text_LivRematchDefeated
    &[
        0xC6, 0xC3, 0xD0, 0xF0, 0x00, 0xBB, 0xEB, 0xEB, 0xEB, 0xB8, 0x00, 0xEB, 0xD9, 0x00, 0xE0,
        0xE3, 0xE7, 0xE8, 0x00, 0xD5, 0xDB, 0xD5, 0xDD, 0xE2, 0xB0, 0xFE, 0xBC, 0xDD, 0xDB, 0x00,
        0xE7, 0xDD, 0xE7, 0xE8, 0xD9, 0xE6, 0xB0, 0xFF,
    ],
    // Route103_Text_AndrewDefeated
    &[
        0xC1, 0xD5, 0xDC, 0xAB, 0x00, 0xC6, 0xE3, 0xE7, 0xE8, 0x00, 0xDD, 0xE8, 0xAB, 0xFE, 0xC3,
        0xB4, 0xE1, 0x00, 0xD9, 0xEA, 0xD9, 0xE2, 0x00, 0xE1, 0xE3, 0xE6, 0xD9, 0x00, 0xD5, 0xE2,
        0xE2, 0xE3, 0xED, 0xD9, 0xD8, 0x00, 0xE2, 0xE3, 0xEB, 0xAB, 0xFF,
    ],
    // Route103_Text_MiguelDefeated
    &[
        0xC9, 0xDC, 0xB8, 0x00, 0xE1, 0xED, 0x00, 0xDB, 0xE3, 0xE7, 0xDC, 0xAB, 0xFE, 0xC7, 0xED,
        0x00, 0xD8, 0xD5, 0xE6, 0xE0, 0xDD, 0xE2, 0xDB, 0x00, 0xCA, 0xC9, 0xC5, 0x1B, 0xC7, 0xC9,
        0xC8, 0xAB, 0xFF,
    ],
    // Route103_Text_MiguelRematchDefeated
    &[
        0xC9, 0xDC, 0xAB, 0xFE, 0xC7, 0xED, 0x00, 0xD8, 0xD5, 0xE6, 0xE0, 0xDD, 0xE2, 0xDB, 0x00,
        0xCA, 0xC9, 0xC5, 0x1B, 0xC7, 0xC9, 0xC8, 0xAB, 0xFF,
    ],
    // Route103_Text_PeteDefeated
    &[
        0xC9, 0xDC, 0xB8, 0x00, 0xE8, 0xDC, 0xD5, 0xE8, 0xB4, 0xE7, 0x00, 0xDB, 0xE3, 0xE3, 0xD8,
        0x00, 0xDB, 0xE3, 0xDD, 0xE2, 0xDB, 0xAB, 0xFF,
    ],
    // Route103_Text_IsabelleDefeated
    &[0xC1, 0xE6, 0xE3, 0xD5, 0xE2, 0xB0, 0xFF],
    // Route103_Text_RhettDefeated
    &[
        0xD1, 0xDC, 0xE3, 0xD5, 0xAB, 0xFE, 0xCE, 0xDC, 0xD9, 0x00, 0xDF, 0xDD, 0xD8, 0x00, 0xD7,
        0xD5, 0xE2, 0x00, 0xE6, 0xE3, 0xD7, 0xDF, 0xAB, 0xFF,
    ],
    // Route103_Text_MarcosDefeated
    &[
        0xC7, 0xED, 0x00, 0xE3, 0xE2, 0xD9, 0xAE, 0xE1, 0xD5, 0xE2, 0x00, 0xE7, 0xDC, 0xE3, 0xEB,
        0x00, 0xDD, 0xE7, 0x00, 0xE6, 0xE9, 0xDD, 0xE2, 0xD9, 0xD8, 0xB0, 0xFF,
    ],
    // Route103_Text_MayDefeated
    &[
        0xD1, 0xE3, 0xEB, 0xAB, 0x00, 0xCE, 0xDC, 0xD5, 0xE8, 0xB4, 0xE7, 0x00, 0xDB, 0xE6, 0xD9,
        0xD5, 0xE8, 0xAB, 0xFE, 0xFD, 0x01, 0xFD, 0x05, 0xB8, 0x00, 0xED, 0xE3, 0xE9, 0xB4, 0xE6,
        0xD9, 0x00, 0xE4, 0xE6, 0xD9, 0xE8, 0xE8, 0xED, 0x00, 0xDB, 0xE3, 0xE3, 0xD8, 0xAB, 0xFF,
    ],
    // Route103_Text_BrendanDefeated
    &[
        0xC2, 0xE9, 0xDC, 0xB8, 0x00, 0xFD, 0x01, 0xB8, 0x00, 0xED, 0xE3, 0xE9, 0xB4, 0xE6, 0xD9,
        0x00, 0xE2, 0xE3, 0xE8, 0x00, 0xE8, 0xE3, 0xE3, 0x00, 0xE7, 0xDC, 0xD5, 0xD6, 0xD6, 0xED,
        0xAD, 0xFF,
    ],
];

#[test]
fn the_class_name_pin_spells_the_upstream_names_through_the_charmap() {
    let table = expected_trainer_class_name_table();
    // Header, 66 records of 6 bytes, and 674 bytes of streams.
    assert_eq!(table.len(), 8 + 66 * 6 + 674);
    let rival = [
        0x53, 0x54, 0x00, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
    ];
    assert_eq!(class_name_stream("{PKMN} TRAINER"), rival);
    assert_eq!(
        class_name_stream("COOLTRAINER"),
        [0xBD, 0xC9, 0xC9, 0xC6, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF]
    );
    assert_eq!(
        class_name_stream("HIKER"),
        [0xC2, 0xC3, 0xC5, 0xBF, 0xCC, 0xFF]
    );
    assert_eq!(class_name_stream("POKéFAN")[3], 0x1B);
    assert_eq!(class_name_stream("SWIMMER♂")[7..], [0xB5, 0xFF]);
    assert_eq!(class_name_stream("SWIMMER♀")[7..], [0xB6, 0xFF]);
    // Twelve glyphs and the `EOS` fill the 13-byte row exactly.
    assert_eq!(class_name_stream("PARASOL LADY").len(), 13);
    for index in [0, 1, 50, 65] {
        assert_eq!(class_name_stream(TRAINER_CLASS_NAMES[index]), rival);
    }
    assert_eq!(TRAINER_CLASS_NAMES[5], TRAINER_CLASS_NAMES[48]);
    assert_eq!(TRAINER_CLASS_NAMES[18], TRAINER_CLASS_NAMES[19]);
    assert!(TRAINER_CLASS_NAMES
        .iter()
        .all(|name| class_name_stream(name).len() <= 13));
}

/// A pack holding just the class-name entry with `payload`, or none.
fn class_name_pack(payload: Option<Vec<u8>>) -> (Vec<u8>, BTreeMap<String, DirectoryEntry>) {
    let mut writer = pack_format::PackWriter::new();
    if let Some(payload) = payload {
        writer.push(pack_format::raw_entry(
            TRAINER_CLASS_NAME_PACK_ID.to_owned(),
            payload,
        ));
    }
    let bytes = writer.finish().expect("a valid pack");
    let entries = index(&bytes, "the fixture pack");
    (bytes, entries)
}

#[test]
fn the_class_name_pin_catches_an_absent_corrupt_or_identically_wrong_entry() {
    let good = expected_trainer_class_name_table();
    let (good_bytes, good_index) = class_name_pack(Some(good.clone()));
    let (none_bytes, none_index) = class_name_pack(None);
    let mut wrong = good.clone();
    // A class name both backends could drop together: the last glyph of record 66.
    let last = wrong.len() - 2;
    wrong[last] ^= 0x01;
    let (wrong_bytes, wrong_index) = class_name_pack(Some(wrong));

    assert!(
        trainer_class_name_differences((&good_bytes, &good_index), (&good_bytes, &good_index))
            .is_empty()
    );
    let one_absent =
        trainer_class_name_differences((&none_bytes, &none_index), (&good_bytes, &good_index));
    assert_eq!(one_absent.len(), 1, "{one_absent:?}");
    assert!(one_absent[0].contains("the ROM backend wrote no such entry"));
    let both_absent =
        trainer_class_name_differences((&none_bytes, &none_index), (&none_bytes, &none_index));
    assert_eq!(both_absent.len(), 2, "{both_absent:?}");
    let one_corrupt =
        trainer_class_name_differences((&good_bytes, &good_index), (&wrong_bytes, &wrong_index));
    assert_eq!(one_corrupt.len(), 1, "{one_corrupt:?}");
    assert!(one_corrupt[0].contains("the checkout pack"));
    // Equal packs are not enough: both wrong the same way still fails twice.
    let both_wrong =
        trainer_class_name_differences((&wrong_bytes, &wrong_index), (&wrong_bytes, &wrong_index));
    assert_eq!(both_wrong.len(), 2, "{both_wrong:?}");
}

/// Ids the two backends are known and accepted to disagree on, each with
/// the reason it was signed off.
///
/// Empty. Every entry here is a promise not kept, so adding one needs a
/// reason that survives review, not a convenient way to make this test
/// pass `(test-ratchet)`.
const REVIEWED_DIFFERENCES: &[(&str, &str)] = &[];

/// A difference line for each of `ids` the ROM backend did not write.
fn missing_imported<V>(ids: &[&str], imported: &BTreeMap<String, V>) -> Vec<String> {
    ids.iter()
        .filter(|id| !imported.contains_key(**id))
        .map(|id| format!("{id}: the ROM backend wrote no such entry"))
        .collect()
}

#[test]
fn the_rom_backend_imports_only_the_title_song_so_no_other_song_is_claimed() {
    assert!(is_claimed_complete("audio/voicegroup/title"));
    assert!(is_claimed_complete(
        "audio/sample/direct-sound/sc88pro_trumpet_60"
    ));
    for id in ROM_IMPORTED_SONG_IDS {
        assert!(
            !is_claimed_complete(id),
            "{id} is pinned by id, not by prefix"
        );
    }
    assert!(!is_claimed_complete("audio/song/se_select"));
    assert!(!is_claimed_complete("audio/song/mus_vs_wild"));
}

#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_rom_backend_matches_the_checkout_pack() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }

    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing {} failed: {err}", rom_path.display()));
    let checkout_bytes = std::fs::read(&checkout_path)
        .unwrap_or_else(|err| panic!("reading {} failed: {err}", checkout_path.display()));

    let imported = index(&rom_bytes, "the imported pack");
    let checkout = index(&checkout_bytes, "the checkout pack");

    // Equal packs are not enough: both backends once dropped the same note.
    assert_title_key_33_plays_the_trumpet(&rom_bytes, &imported, "the imported pack");
    assert_title_key_33_plays_the_trumpet(&checkout_bytes, &checkout, "the checkout pack");

    let reviewed: BTreeMap<&str, &str> = REVIEWED_DIFFERENCES.iter().copied().collect();
    let mut differences = Vec::new();

    for (id, entry) in &imported {
        if let Some(reason) = reviewed.get(id.as_str()) {
            eprintln!("reviewed difference: {id}: {reason}");
            continue;
        }
        let Some(theirs) = checkout.get(id) else {
            differences.push(format!("{id}: the checkout pack has no such entry"));
            continue;
        };
        if let Some(difference) = compare(id, &rom_bytes, entry, &checkout_bytes, theirs) {
            differences.push(difference);
        }
    }

    for id in checkout.keys() {
        if is_claimed_complete(id)
            && !imported.contains_key(id)
            && !reviewed.contains_key(id.as_str())
        {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    for (what, bytes, ids) in [
        ("the ROM backend", &rom_bytes, &imported),
        ("the checkout pack", &checkout_bytes, &checkout),
    ] {
        if let Err(problem) = the_cries_are_named_by_id(bytes, ids) {
            differences.push(format!("{what}: {problem}"));
        }
    }

    // `audio/song/` is not a complete prefix, so the imported songs are
    // pinned by id too (`ROM_IMPORTED_SONG_IDS`).
    differences.extend(missing_imported(&ROM_IMPORTED_SONG_IDS, &imported));

    // `battle-interface/` is not a complete prefix, so the textbox, element,
    // battle-bar, frame, shadow, and ball-palette entries are pinned by id.
    for id in BATTLE_INTERFACE_IDS {
        if !imported.contains_key(id) {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    // Named by id so a payload both backends drop together still fails.
    let pinned: Vec<String> = battler_ids()
        .into_iter()
        .chain(checkpoint_payload_ids())
        .chain(EVOLUTION_SCENE_IDS.into_iter().map(str::to_owned))
        .chain(TRAINER_PORTRAIT_IDS.into_iter().map(str::to_owned))
        .chain(TRAINER_BATTLER_IDS.into_iter().map(str::to_owned))
        .chain(DOWN_ARROW_IDS.into_iter().map(str::to_owned))
        .collect();
    differences.extend(missing_from_either(&pinned, &imported, &checkout));

    differences.extend(field_message_differences(
        (&rom_bytes, &imported),
        (&checkout_bytes, &checkout),
    ));

    differences.extend(trainer_defeat_speech_differences(
        (&rom_bytes, &imported),
        (&checkout_bytes, &checkout),
    ));

    differences.extend(trainer_class_name_differences(
        (&rom_bytes, &imported),
        (&checkout_bytes, &checkout),
    ));

    for difference in &differences {
        eprintln!("difference: {difference}");
    }
    eprintln!(
        "compared {} imported entries against {} checkout entries; {} difference(s)",
        imported.len(),
        checkout.len(),
        differences.len()
    );
    assert!(
        differences.is_empty(),
        "{} entry difference(s) between the ROM backend and the checkout pack",
        differences.len()
    );
}

#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_evolution_scene_record_is_equal_through_both_backends() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }
    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing {} failed: {err}", rom_path.display()));
    let imported_path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-evolution-equivalence-{}.pack",
        std::process::id()
    ));
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&imported_path)
            .unwrap_or_else(|err| panic!("creating {} failed: {err}", imported_path.display()));
        file.write_all(&rom_bytes)
            .expect("writing the imported pack");
    }
    let imported = AssetPack::load(&imported_path);
    let _ = std::fs::remove_file(&imported_path);
    let imported = imported.expect("the imported pack loads");
    let checkout = AssetPack::load(&checkout_path).expect("the checkout pack loads");

    let ours = imported.evolution_scene().expect("the imported record");
    let theirs = checkout.evolution_scene().expect("the checkout record");
    let (a, b) = (&ours.background, &theirs.background);
    assert_eq!((a.width, a.height, a.bit_depth), (128, 112, 4));
    assert_eq!(
        (a.width, a.height, a.bit_depth, a.pixels),
        (b.width, b.height, b.bit_depth, b.pixels)
    );
    assert_eq!(ours.inner_tilemap.len(), 2048);
    assert_eq!(ours.inner_tilemap, theirs.inner_tilemap);
    assert_eq!(ours.outer_tilemap, theirs.outer_tilemap);
    assert_ne!(ours.inner_tilemap, ours.outer_tilemap);
    for (ours, theirs, colors) in [
        (ours.intro_palette, theirs.intro_palette, 256),
        (ours.animation_palette, theirs.animation_palette, 32),
        (ours.sparkle.palette, theirs.sparkle.palette, 16),
    ] {
        assert_eq!(ours.color_count, colors);
        assert_eq!(
            ours.colors().collect::<Vec<_>>(),
            theirs.colors().collect::<Vec<_>>()
        );
    }
    let (a, b) = (&ours.sparkle.image, &theirs.sparkle.image);
    assert_eq!((a.width, a.height, a.bit_depth), (8, 8, 4));
    assert_eq!(
        (a.width, a.height, a.bit_depth, a.pixels),
        (b.width, b.height, b.bit_depth, b.pixels)
    );
    eprintln!("evolution-scene record equal through both backends");
}

/// The Poké Ball resolves through the typed handle from both packs, and the two
/// handles are equal. The handle requires each of its three constituents, so
/// a constituent both backends drop together still fails here.
#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_poke_ball_handle_is_equal_from_the_rom_and_the_checkout() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }
    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing the ROM failed: {err}"));
    let imported_path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-equivalence-poke-ball-{}.pack",
        std::process::id()
    ));
    std::fs::write(&imported_path, rom_bytes).expect("write the imported pack");

    let imported = assets::AssetPack::load(&imported_path).expect("the imported pack loads");
    let checkout = assets::AssetPack::load(&checkout_path).expect("the checkout pack loads");
    let from_rom = imported.poke_ball().expect("the imported pack's Poké Ball");
    let from_checkout = checkout.poke_ball().expect("the checkout pack's Poké Ball");
    let _ = std::fs::remove_file(imported_path);

    assert_eq!(
        (from_rom.sheet.image.width, from_rom.sheet.image.height),
        (16, 48)
    );
    assert_eq!(from_rom.sheet.palette.color_count, 16);
    assert_eq!(
        (from_rom.open_sheet.width, from_rom.open_sheet.height),
        (16, 16)
    );
    assert_eq!(
        from_rom.sheet.image.pixels,
        from_checkout.sheet.image.pixels
    );
    assert!(from_rom
        .sheet
        .palette
        .colors()
        .eq(from_checkout.sheet.palette.colors()));
    assert_eq!(from_rom.open_sheet.pixels, from_checkout.open_sheet.pixels);
}

/// The down arrow resolves through the typed accessor from both packs, and
/// all four frames are equal. The accessor validates the sheet's shape, so a
/// ROM import that wrote the wrong raster fails here as well as in the byte
/// comparison.
#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_down_arrow_frames_are_equal_from_the_rom_and_the_checkout() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }
    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing the ROM failed: {err}"));
    let imported_path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-equivalence-down-arrow-{}.pack",
        std::process::id()
    ));
    std::fs::write(&imported_path, rom_bytes).expect("write the imported pack");

    let imported = AssetPack::load(&imported_path).expect("the imported pack loads");
    let checkout = AssetPack::load(&checkout_path).expect("the checkout pack loads");
    let from_rom = imported
        .text_printer_down_arrow()
        .expect("the imported pack's down arrow");
    let from_checkout = checkout
        .text_printer_down_arrow()
        .expect("the checkout pack's down arrow");
    let _ = std::fs::remove_file(imported_path);

    for index in 0..4 {
        let (rom_frame, checkout_frame) = (from_rom.frame(index), from_checkout.frame(index));
        assert_eq!(
            (rom_frame.width, rom_frame.height),
            (8, 16),
            "frame {index}"
        );
        assert!(
            rom_frame.pixels.iter().any(|&pixel| pixel != 0),
            "frame {index} is blank"
        );
        assert_eq!(rom_frame.pixels, checkout_frame.pixels, "frame {index}");
    }
}

/// The cries, named by id: the `audio/cry/table` entry, exactly
/// [`CRY_TABLE_LEN`] distinct samples in the table, and an entry for each.
///
/// # Errors
///
/// A description of what the pack is missing.
fn the_cries_are_named_by_id(
    pack: &[u8],
    entries: &BTreeMap<String, DirectoryEntry>,
) -> Result<(), String> {
    let table = entries
        .get(CRY_TABLE_PACK_ID)
        .ok_or_else(|| format!("no {CRY_TABLE_PACK_ID} entry"))?;
    let table = CryTable::decode(&pack[table.offset..table.offset + table.length])
        .map_err(|err| format!("{CRY_TABLE_PACK_ID} does not decode: {err}"))?;
    let named: BTreeSet<&SampleId> = table.samples().iter().collect();
    if named.len() != CRY_TABLE_LEN {
        return Err(format!(
            "{CRY_TABLE_PACK_ID} names {} distinct samples, not {CRY_TABLE_LEN}",
            named.len()
        ));
    }
    match named.iter().find(|SampleId(id)| !entries.contains_key(id)) {
        Some(SampleId(id)) => Err(format!("no {id} entry for a sample the table names")),
        None => Ok(()),
    }
}

/// The pinned battle-interface ids are exactly the profile's roots, with the
/// thirteen elements in table order and each starting where the last ends, the
/// two frames keeping their own metatile shapes, and no complete prefix
/// claiming any of them.
#[test]
fn the_battle_interface_ids_are_pinned_not_claimed_by_prefix() {
    let roots = &rom_import::EMERALD_US_REV0.roots.battle_interface;
    let mut profiled: Vec<&str> = roots
        .images
        .iter()
        .map(|root| root.id)
        .chain(roots.palettes.iter().map(|root| root.id))
        .chain(roots.tilemaps.iter().map(|root| root.id))
        .chain(roots.healthbox_elements.iter().map(|root| root.id))
        .chain(roots.battle_bar.iter().map(|root| root.id))
        .collect();
    profiled.sort_unstable();
    let mut pinned = BATTLE_INTERFACE_IDS;
    pinned.sort_unstable();
    assert_eq!(profiled, pinned);

    // The table is one run: the pinned order, each root's length, and each
    // address the previous root's end.
    let elements: Vec<&str> = roots.healthbox_elements.iter().map(|r| r.id).collect();
    assert_eq!(elements, BATTLE_INTERFACE_IDS[5..18]);
    for pair in roots.healthbox_elements.windows(2) {
        assert_eq!(
            pair[0].addr.raw() + pair[0].len,
            pair[1].addr.raw(),
            "{} ends where {} starts",
            pair[0].id,
            pair[1].id
        );
    }
    assert_eq!(
        roots.healthbox_elements.iter().map(|r| r.len).sum::<u32>(),
        118 * 32
    );
    assert_eq!(
        roots.battle_bar.map(|bar| bar.id),
        Some(BATTLE_INTERFACE_IDS[18])
    );

    // The singles frames keep the metatile cut each is built with
    // (`src/graphics.c:628-629`), the shadow and ball palettes their roots.
    let image = |id: &str| {
        roots
            .images
            .iter()
            .find(|root| root.id == id)
            .unwrap_or_else(|| panic!("{id} is not an image root"))
    };
    let player = image("battle-interface/image/healthbox_singles_player");
    let opponent = image("battle-interface/image/healthbox_singles_opponent");
    assert_eq!(
        (
            player.width,
            player.height,
            player.metatile(),
            player.tile_count
        ),
        (128, 64, (8, 8), 128)
    );
    assert_eq!(
        (
            opponent.width,
            opponent.height,
            opponent.metatile(),
            opponent.tile_count
        ),
        (128, 32, (8, 4), 64)
    );
    let shadow = image("battle-interface/image/enemy_mon_shadow");
    assert_eq!((shadow.width, shadow.height, shadow.tile_count), (32, 8, 4));
    for root in [player, opponent, shadow] {
        assert_eq!(
            (root.encoding, root.rom_bit_depth, root.pack_bit_depth),
            (rom_import::Encoding::Lz77, 4, 4),
            "{}",
            root.id
        );
    }
    for id in [
        "battle-interface/palette/ball_status_bar",
        "battle-interface/palette/ball_display",
    ] {
        let root = roots
            .palettes
            .iter()
            .find(|root| root.id == id)
            .unwrap_or_else(|| panic!("{id} is not a palette root"));
        assert_eq!(
            (root.color_count, root.encoding),
            (16, rom_import::Encoding::Raw),
            "{id}"
        );
    }

    for id in [
        "battle-interface/raw/battle_bar",
        "battle-interface/raw/healthbox_element/hpbar",
        "battle-interface/palette/ball_display",
        "battle-interface/image/healthbox_singles_player",
        "battle-interface/image/enemy_mon_shadow",
    ] {
        assert!(
            !COMPLETE_PREFIXES
                .iter()
                .any(|prefix| id.starts_with(prefix)),
            "{id} must stay pinned by id, not claimed by a prefix"
        );
    }
}

/// The profile's naming-screen roots are exactly the checkout manifest's 26
/// ids, so the complete `naming-screen/` prefix cannot pass by both backends
/// dropping the same entry.
#[test]
fn the_naming_screen_profile_roots_are_the_manifest_ids() {
    const SHEETS: [&str; 15] = [
        "menu",
        "page_swap_frame",
        "back_button",
        "ok_button",
        "page_swap_upper",
        "page_swap_lower",
        "page_swap_others",
        "cursor",
        "cursor_squished",
        "cursor_filled",
        "page_swap_button",
        "input_arrow",
        "underscore",
        "pc_icon_off",
        "pc_icon_on",
    ];
    const PALETTES: [&str; 7] = [
        "menu",
        "page_swap_upper",
        "page_swap_lower",
        "page_swap_others",
        "buttons",
        "cursor",
        "keyboard",
    ];
    const TILEMAPS: [&str; 4] = [
        "background",
        "keyboard_upper",
        "keyboard_lower",
        "keyboard_symbols",
    ];
    let mut expected: Vec<String> = SHEETS
        .iter()
        .map(|stem| format!("naming-screen/image/{stem}"))
        .chain(
            PALETTES
                .iter()
                .map(|stem| format!("naming-screen/palette/{stem}")),
        )
        .chain(
            TILEMAPS
                .iter()
                .map(|stem| format!("naming-screen/raw/{stem}")),
        )
        .collect();
    expected.sort_unstable();
    let roots = &rom_import::EMERALD_US_REV0.roots.naming_screen;
    let mut profiled: Vec<String> = roots
        .images
        .iter()
        .map(|root| root.id)
        .chain(roots.palettes.iter().map(|root| root.id))
        .chain(roots.tilemaps.iter().map(|root| root.id))
        .map(str::to_owned)
        .collect();
    profiled.sort_unstable();
    assert_eq!(profiled, expected);
}

/// `mus_title.mid` plays key 33 on program 48 (channel 2, tick 2160). That is
/// the strings key split, whose label sits 36 bytes before its data
/// (`pokeemerald/asm/macros/m4a.inc:25-32`), so key 33 reads the final byte of
/// piano's table, child 3 (`pokeemerald/sound/keysplit_tables.inc:13-22`). The
/// strings group declares three records, and the next linked record is
/// trumpet's first (`pokeemerald/sound/voice_groups.inc:11-12`).
/// Asserted per pack, against upstream's own reads, so two packs that agree
/// on the same omission still fail.
fn assert_title_key_33_plays_the_trumpet(
    pack_bytes: &[u8],
    entries: &BTreeMap<String, DirectoryEntry>,
    what: &str,
) {
    const STRINGS_PROGRAM: usize = 48;
    const PLAYED_KEY: usize = 33;
    const EXPECTED_CHILD: u8 = 3;
    let voicegroup = |id: &str| {
        let entry = entries
            .get(id)
            .unwrap_or_else(|| panic!("{what} has no {id}"));
        VoiceGroup::decode(&pack_bytes[entry.offset..entry.offset + entry.length])
            .unwrap_or_else(|err| panic!("{what}'s {id} does not decode: {err}"))
    };
    let title = voicegroup("audio/voicegroup/title");
    let Some(VoiceEntry::KeySplit(split)) = title.slot(STRINGS_PROGRAM) else {
        panic!("{what}: title program {STRINGS_PROGRAM} is not a key split");
    };
    let offset = PLAYED_KEY
        .checked_sub(usize::from(split.starting_note))
        .unwrap_or_else(|| panic!("{what}: the strings split does not reach key {PLAYED_KEY}"));
    assert_eq!(
        split.table().get(offset).copied(),
        Some(EXPECTED_CHILD),
        "{what}: key {PLAYED_KEY} must select child {EXPECTED_CHILD}"
    );
    let strings = voicegroup(&split.children.0);
    let Some(VoiceEntry::DirectSound(trumpet)) = strings.slot(usize::from(EXPECTED_CHILD)) else {
        panic!("{what}: strings child {EXPECTED_CHILD} is not a DirectSound voice");
    };
    assert_eq!(
        trumpet.sample.0,
        "audio/sample/direct-sound/sc88pro_trumpet_60"
    );
}

/// Where `cargo xtask extract` writes: `<repo root>/`
/// [`OUTPUT_RELATIVE_PATH`], the same fixed destination
/// `xtask::extract::run` computes from its own manifest directory.
///
/// Deliberately not [`pack_format::default_pack_path`]. That resolver
/// answers "where does a *running game* find its pack", and its first two
/// rungs — `$POKEEMERALD_PACK` and the OS user-data directory — are exactly
/// the two destinations `--import-rom` writes to. Resolving through it
/// would let this gate compare a fresh ROM import against an earlier ROM
/// import and pass without the checkout extractor being involved at all,
/// and a typo in the override would skip the gate even with a valid
/// checkout pack on disk `(test-ratchet)`.
fn checkout_pack_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/rom-import is always two levels under the repo root")
        .join(OUTPUT_RELATIVE_PATH)
}

/// The ROM to import, if the environment names one.
fn rom_path() -> Option<PathBuf> {
    let value = std::env::var_os(ROM_ENV)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// One difference per id either backend's pack lacks.
fn missing_from_either(
    ids: &[String],
    imported: &BTreeMap<String, DirectoryEntry>,
    checkout: &BTreeMap<String, DirectoryEntry>,
) -> Vec<String> {
    let mut differences = Vec::new();
    for id in ids {
        for (what, entries) in [
            ("the ROM backend", imported),
            ("the checkout pack", checkout),
        ] {
            if !entries.contains_key(id) {
                differences.push(format!("{id}: {what} wrote no such entry"));
            }
        }
    }
    differences
}

/// The `field-message/table` pin applied to each backend's pack, given as
/// `(bytes, index)` pairs: named by id and bytes so a message both backends
/// drop or misencode together still fails.
fn field_message_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
) -> Vec<String> {
    let mut differences = Vec::new();
    for (what, (bytes, entries)) in [
        ("the ROM backend", imported),
        ("the checkout pack", checkout),
    ] {
        let Some(entry) = entries.get(FIELD_MESSAGE_PACK_ID) else {
            differences.push(format!(
                "{FIELD_MESSAGE_PACK_ID}: {what} wrote no such entry"
            ));
            continue;
        };
        if bytes[entry.offset..entry.offset + entry.length] != FIELD_MESSAGE_TABLE {
            differences.push(format!(
                "{FIELD_MESSAGE_PACK_ID}: {what} wrote a payload other than gText_PkmnFainted_FldPsn"
            ));
        }
    }
    differences
}

/// Each `gTrainerClassNames` row as both backends must write it, in
/// `TrainerClass` index order, spelled from `src/data/text/trainer_class_names.h`.
/// `{PKMN}` is the two glyph tokens `PK` and `MN`; `é`, `♂`, and `♀` are glyphs of
/// their own. The four `{PKMN} TRAINER` classes, the two `COOLTRAINER`s, and the
/// two `TUBER`s are separate records with equal names.
const TRAINER_CLASS_NAMES: [&str; 66] = [
    "{PKMN} TRAINER",
    "{PKMN} TRAINER",
    "HIKER",
    "TEAM AQUA",
    "{PKMN} BREEDER",
    "COOLTRAINER",
    "BIRD KEEPER",
    "COLLECTOR",
    "SWIMMER♂",
    "TEAM MAGMA",
    "EXPERT",
    "AQUA ADMIN",
    "BLACK BELT",
    "AQUA LEADER",
    "HEX MANIAC",
    "AROMA LADY",
    "RUIN MANIAC",
    "INTERVIEWER",
    "TUBER",
    "TUBER",
    "LADY",
    "BEAUTY",
    "RICH BOY",
    "POKéMANIAC",
    "GUITARIST",
    "KINDLER",
    "CAMPER",
    "PICNICKER",
    "BUG MANIAC",
    "PSYCHIC",
    "GENTLEMAN",
    "ELITE FOUR",
    "LEADER",
    "SCHOOL KID",
    "SR. AND JR.",
    "WINSTRATE",
    "POKéFAN",
    "YOUNGSTER",
    "CHAMPION",
    "FISHERMAN",
    "TRIATHLETE",
    "DRAGON TAMER",
    "NINJA BOY",
    "BATTLE GIRL",
    "PARASOL LADY",
    "SWIMMER♀",
    "TWINS",
    "SAILOR",
    "COOLTRAINER",
    "MAGMA ADMIN",
    "{PKMN} TRAINER",
    "BUG CATCHER",
    "{PKMN} RANGER",
    "MAGMA LEADER",
    "LASS",
    "YOUNG COUPLE",
    "OLD COUPLE",
    "SIS AND BRO",
    "SALON MAIDEN",
    "DOME ACE",
    "PALACE MAVEN",
    "ARENA TYCOON",
    "FACTORY HEAD",
    "PIKE QUEEN",
    "PYRAMID KING",
    "{PKMN} TRAINER",
];

/// A class name through the charmap, spelled here so no code under test
/// vouches for it: space is `00`, `.` is `AD`, `A`..`Z` start at `BB`, `a`..`z`
/// at `D5`, `é` is `1B`, `♂` is `B5`, `♀` is `B6`, `{PKMN}` is `53 54`, and the
/// string ends in `EOS` (`FF`).
fn class_name_stream(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = name;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("{PKMN}") {
            out.extend_from_slice(&[0x53, 0x54]);
            rest = after;
            continue;
        }
        out.push(match c {
            ' ' => 0x00,
            '.' => 0xAD,
            'é' => 0x1B,
            '♂' => 0xB5,
            '♀' => 0xB6,
            'A'..='Z' => 0xBB + u8::try_from(u32::from(c) - u32::from('A')).expect("a letter"),
            'a'..='z' => 0xD5 + u8::try_from(u32::from(c) - u32::from('a')).expect("a letter"),
            other => panic!("`{other}` has no spelling in the class-name pin"),
        });
        rest = &rest[c.len_utf8()..];
    }
    out.push(0xFF);
    out
}

/// The `trainer-class-name/table` payload both backends must write: the
/// header, then one record per class in index order. Built here from
/// [`TRAINER_CLASS_NAMES`] rather than by the schema encoder, so an encoder
/// both backends share cannot vouch for itself.
fn expected_trainer_class_name_table() -> Vec<u8> {
    let count = u16::try_from(TRAINER_CLASS_NAMES.len()).expect("sixty-six records");
    let mut table = b"TCLN".to_vec();
    table.extend_from_slice(&1u16.to_le_bytes());
    table.extend_from_slice(&count.to_le_bytes());
    for (id, name) in (0u16..).zip(TRAINER_CLASS_NAMES) {
        table.extend_from_slice(&id.to_le_bytes());
        let stream = class_name_stream(name);
        let len = u32::try_from(stream.len()).expect("a stream fits in u32");
        table.extend_from_slice(&len.to_le_bytes());
        table.extend_from_slice(&stream);
    }
    table
}

/// The `trainer-class-name/table` pin applied to each backend's pack, given as
/// `(bytes, index)` pairs: named by id and bytes so a class both backends
/// drop, merge, reorder, or misencode together still fails.
fn trainer_class_name_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
) -> Vec<String> {
    let expected = expected_trainer_class_name_table();
    let mut differences = Vec::new();
    for (what, (bytes, entries)) in [
        ("the ROM backend", imported),
        ("the checkout pack", checkout),
    ] {
        let Some(entry) = entries.get(TRAINER_CLASS_NAME_PACK_ID) else {
            differences.push(format!(
                "{TRAINER_CLASS_NAME_PACK_ID}: {what} wrote no such entry"
            ));
            continue;
        };
        if bytes[entry.offset..entry.offset + entry.length] != expected[..] {
            differences.push(format!(
                "{TRAINER_CLASS_NAME_PACK_ID}: {what} wrote a payload other than the sixty-six trainer class names"
            ));
        }
    }
    differences
}

/// The `trainer-defeat-speech/table` payload both backends must write: the
/// header, then one record per speech in ID order. Built here from
/// [`TRAINER_DEFEAT_SPEECHES`] rather than by the schema encoder, so an encoder
/// both backends share cannot vouch for itself.
fn expected_trainer_defeat_speech_table() -> Vec<u8> {
    let count = u16::try_from(TRAINER_DEFEAT_SPEECHES.len()).expect("fourteen records");
    let mut table = b"TDFS".to_vec();
    table.extend_from_slice(&1u16.to_le_bytes());
    table.extend_from_slice(&count.to_le_bytes());
    for (id, stream) in (0u16..).zip(TRAINER_DEFEAT_SPEECHES) {
        table.extend_from_slice(&id.to_le_bytes());
        let len = u32::try_from(stream.len()).expect("a stream fits in u32");
        table.extend_from_slice(&len.to_le_bytes());
        table.extend_from_slice(stream);
    }
    table
}

/// The `trainer-defeat-speech/table` pin applied to each backend's pack, given
/// as `(bytes, index)` pairs: named by id and bytes so a speech both backends
/// drop, merge, or misencode together still fails.
fn trainer_defeat_speech_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
) -> Vec<String> {
    let expected = expected_trainer_defeat_speech_table();
    assert_eq!(
        expected.len(),
        8 + 14 * 6 + 420,
        "the pin spells all 14 speeches"
    );
    let mut differences = Vec::new();
    for (what, (bytes, entries)) in [
        ("the ROM backend", imported),
        ("the checkout pack", checkout),
    ] {
        let Some(entry) = entries.get(TRAINER_DEFEAT_SPEECH_PACK_ID) else {
            differences.push(format!(
                "{TRAINER_DEFEAT_SPEECH_PACK_ID}: {what} wrote no such entry"
            ));
            continue;
        };
        if bytes[entry.offset..entry.offset + entry.length] != expected[..] {
            differences.push(format!(
                "{TRAINER_DEFEAT_SPEECH_PACK_ID}: {what} wrote a payload other than the fourteen Route 103 defeat speeches"
            ));
        }
    }
    differences
}

/// Parse a pack's directory into an id-keyed map.
fn index(bytes: &[u8], what: &str) -> BTreeMap<String, DirectoryEntry> {
    parse_directory(bytes)
        .unwrap_or_else(|err| panic!("{what} is not a well-formed pack: {err}"))
        .into_iter()
        .map(|entry| (entry.id.clone(), entry))
        .collect()
}

/// Compare one entry across both packs, or `None` if they match.
fn compare(
    id: &str,
    ours_bytes: &[u8],
    ours: &DirectoryEntry,
    theirs_bytes: &[u8],
    theirs: &DirectoryEntry,
) -> Option<String> {
    if ours.kind != theirs.kind {
        return Some(format!(
            "{id}: {} from the ROM, {} from the checkout",
            describe(ours.kind),
            describe(theirs.kind)
        ));
    }
    let ours_payload = &ours_bytes[ours.offset..ours.offset + ours.length];
    let theirs_payload = &theirs_bytes[theirs.offset..theirs.offset + theirs.length];
    if ours_payload == theirs_payload {
        return None;
    }
    if id.starts_with("audio/song/") {
        if let Some(report) = compare_songs(id, ours_payload, theirs_payload) {
            return Some(report);
        }
    }
    let first = ours_payload
        .iter()
        .zip(theirs_payload)
        .position(|(a, b)| a != b);
    Some(match first {
        Some(at) => format!(
            "{id}: payloads differ at byte {at} ({:#04x} from the ROM, {:#04x} from the \
             checkout); lengths {} and {}",
            ours_payload[at],
            theirs_payload[at],
            ours_payload.len(),
            theirs_payload.len()
        ),
        None => format!(
            "{id}: one payload is a prefix of the other; lengths {} and {}",
            ours_payload.len(),
            theirs_payload.len()
        ),
    })
}

/// Name the first event the two backends disagree on, if both payloads
/// decode as songs. A byte offset into an event stream is hard to read; a
/// track and event index with the two events side by side is not.
fn compare_songs(id: &str, ours: &[u8], theirs: &[u8]) -> Option<String> {
    let (ours, theirs) = (Song::decode(ours).ok()?, Song::decode(theirs).ok()?);
    if ours.tracks().len() != theirs.tracks().len() {
        return Some(format!(
            "{id}: {} tracks from the ROM, {} from the checkout",
            ours.tracks().len(),
            theirs.tracks().len()
        ));
    }
    for (track, (a, b)) in ours.tracks().iter().zip(theirs.tracks()).enumerate() {
        let first = a.iter().zip(b).position(|(x, y)| x != y);
        let at = match first {
            Some(at) => at,
            None if a.len() == b.len() => continue,
            None => a.len().min(b.len()),
        };
        return Some(format!(
            "{id}: track {track} event {at}: {:?} from the ROM, {:?} from the checkout              (track lengths {} and {})",
            a.get(at),
            b.get(at),
            a.len(),
            b.len()
        ));
    }
    Some(format!("{id}: header metadata differs"))
}

/// One entry's kind and metadata, on one line.
fn describe(kind: EntryKind) -> String {
    match kind {
        EntryKind::Image {
            width,
            height,
            bit_depth,
        } => format!("image {width}x{height} at {bit_depth}bpp"),
        EntryKind::Palette { color_count } => format!("palette of {color_count}"),
        EntryKind::Raw => "raw".to_owned(),
    }
}
