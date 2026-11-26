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
use assets::SightTrainerIntroSpeechTable;
use assets::{
    AssetPack, CryTable, SampleId, Song, TrainerClass, TrainerClassNameTable,
    TrainerDefeatSpeechContext, TrainerDefeatSpeechTable, VoiceEntry, VoiceGroup,
};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, OUTPUT_RELATIVE_PATH};
use rom_import::SIGHT_TRAINER_INTRO_SPEECH_PACK_ID;
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
    "sight-trainer-intro-speech/",
    "trainer-class-name/",
    "battle-anim/",
    "bag/",
    "starter-chooser/",
    "pokemon/",
    "opening-scene/",
    "evolution-scene/",
    "poke-ball/",
    "naming-screen/",
    "field-effect/",
    "party-menu/",
];

/// The text printer's page-prompt arrows (light, then dark), by id:
/// `interface/` is a complete prefix, but a sheet both backends drop together
/// would pass it.
const DOWN_ARROW_IDS: [&str; 2] = [
    "interface/image/down_arrow",
    "interface/image/down_arrow_alt",
];

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

/// The tall-grass field effect's two pack ids. A prefix cannot catch both
/// backends dropping the same constituent, so each is named.
const FIELD_EFFECT_IDS: [&str; 2] = [
    "field-effect/image/tall_grass",
    "field-effect/palette/general_1",
];

/// The party menu's fourteen pack ids, named independently of the checkout
/// extractor and of the profile: three sheets, three palettes, the
/// background screen, five slot panels, and two buttons. `party-menu/` is a
/// complete prefix, but a record both backends drop together would pass it.
const PARTY_MENU_IDS: [&str; 14] = [
    "party-menu/image/bg",
    "party-menu/image/pokeball",
    "party-menu/image/hold_icons",
    "party-menu/palette/bg",
    "party-menu/palette/pokeball",
    "party-menu/palette/hold_icons",
    "party-menu/raw/bg",
    "party-menu/raw/slot_main",
    "party-menu/raw/slot_main_no_hp",
    "party-menu/raw/slot_wide",
    "party-menu/raw/slot_wide_no_hp",
    "party-menu/raw/slot_wide_empty",
    "party-menu/raw/confirm_button",
    "party-menu/raw/cancel_button",
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

/// The `Route103_Text_*Defeated` labels in `TrainerDefeatSpeechContext::ALL`
/// order. Labels only: the expected strings are read at test time from the
/// contributor's own checkout, so no game text is committed here.
const TRAINER_DEFEAT_SPEECH_LABELS: [&str; 14] = [
    "Route103_Text_DaisyDefeated",
    "Route103_Text_AmyDefeated",
    "Route103_Text_LivDefeated",
    "Route103_Text_AmyRematchDefeated",
    "Route103_Text_LivRematchDefeated",
    "Route103_Text_AndrewDefeated",
    "Route103_Text_MiguelDefeated",
    "Route103_Text_MiguelRematchDefeated",
    "Route103_Text_PeteDefeated",
    "Route103_Text_IsabelleDefeated",
    "Route103_Text_RhettDefeated",
    "Route103_Text_MarcosDefeated",
    "Route103_Text_MayDefeated",
    "Route103_Text_BrendanDefeated",
];

/// The upstream files the reference speeches are read from, relative to the
/// checkout: the shared trainer text, then Route 103's script-local text.
const TRAINER_DEFEAT_SPEECH_SOURCES: [&str; 2] =
    ["data/text/trainers.inc", "data/maps/Route103/scripts.inc"];

/// Route 103's nine sight-trainer object-event scripts in the table's record
/// order. Names only: each script's intro text is read at test time from the
/// contributor's own checkout, so no game text is committed here.
const SIGHT_TRAINER_SCRIPTS: [&str; 9] = [
    "Route103_EventScript_Amy",
    "Route103_EventScript_Andrew",
    "Route103_EventScript_Daisy",
    "Route103_EventScript_Isabelle",
    "Route103_EventScript_Liv",
    "Route103_EventScript_Marcos",
    "Route103_EventScript_Miguel",
    "Route103_EventScript_Pete",
    "Route103_EventScript_Rhett",
];

/// Class indices that share one display name upstream: the four
/// `{PKMN}`-glyph classes, the two `COOLTRAINER` classes, and the two `TUBER`
/// classes. Every other class has a name of its own.
const SHARED_CLASS_NAMES: [&[u16]; 3] = [&[0, 1, 50, 65], &[5, 48], &[18, 19]];

/// Synthetic class names honouring [`SHARED_CLASS_NAMES`]; not game text.
fn synthetic_class_names() -> Vec<String> {
    (0u16..66)
        .map(|index| match index {
            0 | 1 | 50 | 65 => "{PKMN} QZ".to_owned(),
            48 => "ZAF".to_owned(),
            19 => "ZAS".to_owned(),
            _ => {
                let letter = |n: u16| char::from(b'A' + u8::try_from(n).expect("a letter"));
                format!("Z{}{}", letter(index / 26), letter(index % 26))
            }
        })
        .collect()
}

/// A synthetic class-name payload, spelled through [`spell`]; not game text.
fn synthetic_class_name_table() -> Vec<u8> {
    let streams: Vec<Vec<u8>> = synthetic_class_names()
        .iter()
        .map(|name| {
            let mut stream = spell(name).expect("synthetic names spell");
            stream.push(0xFF);
            stream
        })
        .collect();
    record_table(*b"TCLN", &streams)
}

/// A synthetic speech payload: distinct made-up lines, with `{PLAYER}` and
/// `{KUN}` where May's and Brendan's speeches carry them; not game text.
fn synthetic_speech_table() -> Vec<u8> {
    let streams: Vec<Vec<u8>> = (0..14)
        .map(|index| {
            let text = match index {
                12 => "Zz!\\n{PLAYER}{KUN}, zz.$".to_owned(),
                13 => "Zz, {PLAYER}, zz.$".to_owned(),
                _ => format!("Zz {}…$", "z".repeat(index + 1)),
            };
            spell(&text).expect("synthetic speeches spell")
        })
        .collect();
    record_table(*b"TDFS", &streams)
}

/// A synthetic intro-speech payload: distinct made-up lines, with `{PLAYER}`
/// in one; not game text.
fn synthetic_sight_trainer_table() -> Vec<u8> {
    let streams: Vec<Vec<u8>> = (0..9)
        .map(|index| {
            let text = match index {
                4 => "Zz!\\n{PLAYER}, zz.$".to_owned(),
                _ => format!("Zz {}…$", "z".repeat(index + 1)),
            };
            spell(&text).expect("synthetic speeches spell")
        })
        .collect();
    keyed_record_table(&streams)
}

#[test]
fn the_hand_charmap_spells_glyphs_controls_and_placeholders() {
    assert_eq!(spell("Az"), Ok(vec![0xBB, 0xEE]));
    assert_eq!(
        spell("{PKMN} é♂♀"),
        Ok(vec![0x53, 0x54, 0x00, 0x1B, 0xB5, 0xB6])
    );
    assert_eq!(
        spell("!?.-…'\",:"),
        Ok(vec![0xAB, 0xAC, 0xAD, 0xAE, 0xB0, 0xB4, 0xB2, 0xB8, 0xF0])
    );
    assert_eq!(spell("0 9"), Ok(vec![0xA1, 0x00, 0xAA]));
    assert_eq!(
        spell("\\n\\p\\l{PLAYER}{KUN}$"),
        Ok(vec![0xFE, 0xFB, 0xFA, 0xFD, 0x01, 0xFD, 0x05, 0xFF])
    );
    assert!(spell("{RIVAL}").is_err());
    assert!(spell("~").is_err());
}

#[test]
fn reference_class_names_are_read_by_define_not_by_row_order() {
    let constants = "\
#define TRAINER_CLASS_ZERO 0x0  // Unused
#define TRAINER_CLASS_ONE  1
#define TRAINER_ENCOUNTER_MUSIC_MALE 0
";
    let names = "\
const u8 gTrainerClassNames[][13] = {
    [TRAINER_CLASS_ONE] = _(\"Zb\"),
    [TRAINER_CLASS_ZERO] = _(\"{PKMN} Za\"),
};
";
    let streams = reference_class_name_streams(constants, names, 2).unwrap();
    assert_eq!(
        streams,
        [
            vec![0x53, 0x54, 0x00, 0xD4, 0xD5, 0xFF],
            vec![0xD4, 0xD6, 0xFF]
        ]
    );
    // A missing row, an unknown designator, and an unspellable name all fail.
    assert!(reference_class_name_streams(constants, names, 3).is_err());
    let unknown = names.replace("[TRAINER_CLASS_ONE]", "[TRAINER_CLASS_TWO]");
    assert!(reference_class_name_streams(constants, &unknown, 2).is_err());
    let unspellable = names.replace("Zb", "Z~");
    assert!(reference_class_name_streams(constants, &unspellable, 2).is_err());
}

#[test]
fn reference_speeches_are_read_by_label_across_continued_strings() {
    let first = "\
Other_Text:
\t.string \"Zq$\"

Want_A:
\t.string \"Za!\\n\"
\t.string \"{PLAYER}…$\"
";
    let second = "Want_B::\n\t.string \"Zb.$\"\n";
    assert_eq!(
        reference_speech(&[first, second], "Want_A"),
        Ok(vec![0xD4, 0xD5, 0xAB, 0xFE, 0xFD, 0x01, 0xB0, 0xFF])
    );
    assert_eq!(
        reference_speech(&[first, second], "Want_B"),
        Ok(vec![0xD4, 0xD6, 0xAD, 0xFF])
    );
    // A label defined nowhere, or twice, and a string never closed all fail.
    assert!(reference_speech(&[first, second], "Want_C").is_err());
    assert!(reference_speech(&[first, first], "Want_A").is_err());
    let open = "Want_A:\n\t.string \"Za\"\n";
    assert!(reference_speech(&[open], "Want_A").is_err());
}

#[test]
fn the_structural_checks_accept_the_synthetic_tables_and_catch_merges() {
    assert_eq!(class_name_structure(&synthetic_class_name_table()), Ok(()));
    assert_eq!(speech_structure(&synthetic_speech_table()), Ok(()));
    // Two classes that upstream keeps apart given one name.
    let mut names = synthetic_class_names();
    names[3] = names[2].clone();
    let streams: Vec<Vec<u8>> = names
        .iter()
        .map(|name| {
            let mut stream = spell(name).unwrap();
            stream.push(0xFF);
            stream
        })
        .collect();
    assert!(class_name_structure(&record_table(*b"TCLN", &streams)).is_err());
    assert!(class_name_structure(b"TCLN").is_err());
    assert!(speech_structure(&synthetic_class_name_table()).is_err());
}

#[test]
fn the_intro_reference_reads_each_scripts_own_operand_and_catches_merges() {
    let scripts = "\
Route103_EventScript_Amy::
\ttrainerbattle_double TRAINER_X, Want_A, Lost_A, Few_A, Callback
\tend

Route103_EventScript_Liv::

\ttrainerbattle_single TRAINER_X, Want_B, Lost_B
\tend

Route103_EventScript_Andrew::
\tmsgbox Want_A, MSGBOX_NPC
";
    assert_eq!(
        reference_intro_label(scripts, "Route103_EventScript_Amy"),
        Ok("Want_A".to_owned())
    );
    assert_eq!(
        reference_intro_label(scripts, "Route103_EventScript_Liv"),
        Ok("Want_B".to_owned())
    );
    // Not a trainerbattle, or defined nowhere.
    assert!(reference_intro_label(scripts, "Route103_EventScript_Andrew").is_err());
    assert!(reference_intro_label(scripts, "Route103_EventScript_Pete").is_err());

    assert_eq!(
        sight_trainer_intro_structure(&synthetic_sight_trainer_table()),
        Ok(())
    );
    // Two scripts given one string, and a table of another domain.
    let mut streams: Vec<Vec<u8>> = (0..9)
        .map(|index| spell(&format!("Zz {}…$", "z".repeat(index + 1))).unwrap())
        .collect();
    streams[1] = streams[0].clone();
    assert!(sight_trainer_intro_structure(&keyed_record_table(&streams)).is_err());
    assert!(sight_trainer_intro_structure(&synthetic_speech_table()).is_err());
}

/// A pack holding just `id` with `payload`, or none.
fn single_entry_pack(
    id: &str,
    payload: Option<Vec<u8>>,
) -> (Vec<u8>, BTreeMap<String, DirectoryEntry>) {
    let mut writer = pack_format::PackWriter::new();
    if let Some(payload) = payload {
        writer.push(pack_format::raw_entry(id.to_owned(), payload));
    }
    let bytes = writer.finish().expect("a valid pack");
    let entries = index(&bytes, "the fixture pack");
    (bytes, entries)
}

type PackDifferences = fn(
    (&[u8], &BTreeMap<String, DirectoryEntry>),
    (&[u8], &BTreeMap<String, DirectoryEntry>),
    &Result<Vec<u8>, String>,
) -> Vec<String>;

#[test]
fn the_reference_checks_catch_an_absent_corrupt_or_identically_wrong_entry() {
    let cases: [(&str, Vec<u8>, PackDifferences); 3] = [
        (
            TRAINER_CLASS_NAME_PACK_ID,
            synthetic_class_name_table(),
            trainer_class_name_differences,
        ),
        (
            TRAINER_DEFEAT_SPEECH_PACK_ID,
            synthetic_speech_table(),
            trainer_defeat_speech_differences,
        ),
        (
            SIGHT_TRAINER_INTRO_SPEECH_PACK_ID,
            synthetic_sight_trainer_table(),
            sight_trainer_intro_speech_differences,
        ),
    ];
    for (id, good, differences) in cases {
        let reference = Ok(good.clone());
        let (good_bytes, good_index) = single_entry_pack(id, Some(good.clone()));
        let (none_bytes, none_index) = single_entry_pack(id, None);
        let mut wrong = good.clone();
        // A glyph both backends could misencode together: the last record's
        // last glyph, still a well-formed stream.
        let last = wrong.len() - 2;
        wrong[last] ^= 0x01;
        let (wrong_bytes, wrong_index) = single_entry_pack(id, Some(wrong));

        let same = differences(
            (&good_bytes, &good_index),
            (&good_bytes, &good_index),
            &reference,
        );
        assert!(same.is_empty(), "{id}: {same:?}");
        let one_absent = differences(
            (&none_bytes, &none_index),
            (&good_bytes, &good_index),
            &reference,
        );
        assert_eq!(one_absent.len(), 1, "{id}: {one_absent:?}");
        assert!(one_absent[0].contains("the ROM backend wrote no such entry"));
        let both_absent = differences(
            (&none_bytes, &none_index),
            (&none_bytes, &none_index),
            &reference,
        );
        assert_eq!(both_absent.len(), 2, "{id}: {both_absent:?}");
        let one_corrupt = differences(
            (&good_bytes, &good_index),
            (&wrong_bytes, &wrong_index),
            &reference,
        );
        assert_eq!(one_corrupt.len(), 1, "{id}: {one_corrupt:?}");
        assert!(one_corrupt[0].contains("the checkout pack"));
        let rom_corrupt = differences(
            (&wrong_bytes, &wrong_index),
            (&good_bytes, &good_index),
            &reference,
        );
        assert_eq!(rom_corrupt.len(), 1, "{id}: {rom_corrupt:?}");
        assert!(rom_corrupt[0].contains("the ROM backend"));
        // Equal packs are not enough: both wrong the same way still fails twice.
        let both_wrong = differences(
            (&wrong_bytes, &wrong_index),
            (&wrong_bytes, &wrong_index),
            &reference,
        );
        assert_eq!(both_wrong.len(), 2, "{id}: {both_wrong:?}");
        // No reference checkout fails closed rather than skipping the check.
        let unread = differences(
            (&good_bytes, &good_index),
            (&good_bytes, &good_index),
            &Err("no checkout".to_owned()),
        );
        assert_eq!(unread.len(), 1, "{id}: {unread:?}");
        assert!(unread[0].contains("no checkout"));
    }
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
        .chain(FIELD_EFFECT_IDS.into_iter().map(str::to_owned))
        .chain(TRAINER_PORTRAIT_IDS.into_iter().map(str::to_owned))
        .chain(TRAINER_BATTLER_IDS.into_iter().map(str::to_owned))
        .chain(DOWN_ARROW_IDS.into_iter().map(str::to_owned))
        .chain(PARTY_MENU_IDS.into_iter().map(str::to_owned))
        .collect();
    differences.extend(missing_from_either(&pinned, &imported, &checkout));

    differences.extend(field_message_differences(
        (&rom_bytes, &imported),
        (&checkout_bytes, &checkout),
    ));

    differences.extend(reference_table_differences(
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

/// Every party-menu graphic resolves through the typed accessor from both
/// packs and is equal. The accessor validates each shape, so a ROM import that
/// wrote a different raster, palette length, or map width fails here as well
/// as in the byte comparison.
#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_party_menu_handle_is_equal_from_the_rom_and_the_checkout() {
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
        "pokeemerald-rs-equivalence-party-menu-{}.pack",
        std::process::id()
    ));
    std::fs::write(&imported_path, rom_bytes).expect("write the imported pack");
    let imported = assets::AssetPack::load(&imported_path);
    let _ = std::fs::remove_file(&imported_path);
    let imported = imported.expect("the imported pack loads");
    let checkout = assets::AssetPack::load(&checkout_path).expect("the checkout pack loads");
    let ours = imported.party_menu().expect("the imported party menu");
    let theirs = checkout.party_menu().expect("the checkout party menu");

    let (a, b) = (&ours.background.sheet, &theirs.background.sheet);
    assert_eq!((a.width, a.height, a.bit_depth), (8, 62 * 8, 4));
    assert_eq!(
        (a.width, a.height, a.bit_depth, a.pixels),
        (b.width, b.height, b.bit_depth, b.pixels)
    );
    assert_eq!(ours.background.palette.color_count, 11 * 16);
    assert!(ours
        .background
        .palette
        .colors()
        .eq(theirs.background.palette.colors()));
    assert_eq!(ours.background.tilemap, theirs.background.tilemap);
    for (name, ours, theirs, size) in [
        ("Poké Ball", &ours.poke_ball, &theirs.poke_ball, (32, 64)),
        ("held item", &ours.held_item, &theirs.held_item, (8, 16)),
    ] {
        let (a, b) = (&ours.image, &theirs.image);
        assert_eq!(
            (a.width, a.height, a.bit_depth),
            (size.0, size.1, 4),
            "{name}"
        );
        assert_eq!(
            (a.width, a.height, a.bit_depth, a.pixels),
            (b.width, b.height, b.bit_depth, b.pixels),
            "{name}"
        );
        assert_eq!(ours.palette.color_count, 16, "{name}");
        assert!(ours.palette.colors().eq(theirs.palette.colors()), "{name}");
    }
    assert_eq!(ours.slots, theirs.slots);
    assert_eq!(ours.confirm_button, theirs.confirm_button);
    assert_eq!(ours.cancel_button, theirs.cancel_button);
    // The five panels and two buttons are different maps, not one repeated.
    for (index, slot) in ours.slots.iter().enumerate() {
        assert!(
            ours.slots[index + 1..].iter().all(|other| other != slot),
            "slot {index} repeats"
        );
    }
    assert_ne!(ours.confirm_button, ours.cancel_button);
    eprintln!("party-menu record equal through both backends");
}

/// Both down arrows resolve through their typed accessors from both packs,
/// and all four frames of each are equal. The accessor validates the sheet's shape, so a
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
    let arrows = [
        (
            "light",
            imported.text_printer_down_arrow(),
            checkout.text_printer_down_arrow(),
        ),
        (
            "dark",
            imported.text_printer_dark_down_arrow(),
            checkout.text_printer_dark_down_arrow(),
        ),
    ];
    let _ = std::fs::remove_file(imported_path);

    for (name, from_rom, from_checkout) in arrows {
        let from_rom = from_rom.unwrap_or_else(|err| panic!("the imported {name} arrow: {err}"));
        let from_checkout =
            from_checkout.unwrap_or_else(|err| panic!("the checkout {name} arrow: {err}"));
        for index in 0..4 {
            let (rom_frame, checkout_frame) = (from_rom.frame(index), from_checkout.frame(index));
            assert_eq!(
                (rom_frame.width, rom_frame.height),
                (8, 16),
                "{name} frame {index}"
            );
            assert!(
                rom_frame.pixels.iter().any(|&pixel| pixel != 0),
                "{name} frame {index} is blank"
            );
            assert_eq!(
                rom_frame.pixels, checkout_frame.pixels,
                "{name} frame {index}"
            );
        }
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

/// The profile carries the tall-grass sheet as a five-frame, 2x2-metatile
/// image and its palette as a separate field-effect root, so the complete
/// `field-effect/` prefix cannot pass by both backends dropping the same entry.
#[test]
fn the_field_effect_profile_roots_are_the_tall_grass_ids() {
    let sprites = &rom_import::EMERALD_US_REV0.roots.sprites;
    let sheet = sprites
        .sheets
        .iter()
        .find(|sheet| sheet.id == FIELD_EFFECT_IDS[0])
        .expect("the tall-grass sheet is a profile root");
    assert_eq!(
        (
            sheet.encoding,
            sheet.rom_bit_depth,
            sheet.pack_bit_depth,
            (sheet.width, sheet.height),
            (sheet.metatile_width, sheet.metatile_height),
            sheet.tile_count
        ),
        (rom_import::Encoding::Raw, 4, 4, (80, 16), (2, 2), 20)
    );

    let palettes: Vec<_> = sprites
        .field_effect_palettes
        .iter()
        .map(|palette| (palette.id, palette.color_count, palette.encoding))
        .collect();
    assert_eq!(
        palettes,
        [(FIELD_EFFECT_IDS[1], 16, rom_import::Encoding::Raw)]
    );
    assert!(
        sprites
            .palettes
            .iter()
            .all(|palette| palette.id != FIELD_EFFECT_IDS[1]),
        "the object-event palette table does not name this bank"
    );

    for id in FIELD_EFFECT_IDS {
        assert!(is_claimed_complete(id), "{id}");
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

/// The profile's party-menu roots are exactly the 14 pinned ids, with the
/// encodings and shapes upstream stores, so the complete `party-menu/` prefix
/// cannot pass by both backends dropping the same entry.
#[test]
fn the_party_menu_profile_roots_are_the_pinned_ids() {
    use rom_import::Encoding::{Lz77, Raw};
    let roots = &rom_import::EMERALD_US_REV0.roots.party_menu;
    let mut profiled: Vec<&str> = roots
        .images
        .iter()
        .map(|root| root.id)
        .chain(roots.palettes.iter().map(|root| root.id))
        .chain(roots.tilemaps.iter().map(|root| root.id))
        .collect();
    profiled.sort_unstable();
    let mut expected = PARTY_MENU_IDS;
    expected.sort_unstable();
    assert_eq!(profiled, expected);

    let image = |id: &str| {
        let root = roots.images.iter().find(|root| root.id == id).unwrap();
        (
            root.encoding,
            root.width,
            root.height,
            root.tile_count,
            root.rom_bit_depth,
            root.pack_bit_depth,
        )
    };
    // The background is cut to its first 62 tiles by `-num_tiles 62`.
    assert_eq!(image("party-menu/image/bg"), (Lz77, 8, 62 * 8, 62, 4, 4));
    assert_eq!(image("party-menu/image/pokeball"), (Lz77, 32, 64, 32, 4, 4));
    assert_eq!(image("party-menu/image/hold_icons"), (Raw, 8, 16, 2, 4, 4));
    let palette = |id: &str| {
        let root = roots.palettes.iter().find(|root| root.id == id).unwrap();
        (root.encoding, root.color_count)
    };
    assert_eq!(palette("party-menu/palette/bg"), (Lz77, 11 * 16));
    assert_eq!(palette("party-menu/palette/pokeball"), (Lz77, 16));
    assert_eq!(palette("party-menu/palette/hold_icons"), (Raw, 16));
    let map = |id: &str| {
        let root = roots.tilemaps.iter().find(|root| root.id == id).unwrap();
        (root.encoding, root.len)
    };
    assert_eq!(map("party-menu/raw/bg"), (Lz77, 32 * 32 * 2));
    for (id, len) in [
        ("slot_main", 10 * 7),
        ("slot_main_no_hp", 10 * 7),
        ("slot_wide", 18 * 3),
        ("slot_wide_no_hp", 18 * 3),
        ("slot_wide_empty", 18 * 3),
        ("confirm_button", 7 * 2 * 2),
        ("cancel_button", 7 * 2 * 2),
    ] {
        assert_eq!(map(&format!("party-menu/raw/{id}")), (Raw, len), "{id}");
    }
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

/// The text-table payloads of both backends against strings read from the
/// local checkout, so a record both backends drop or misencode together still
/// fails.
fn reference_table_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
) -> Vec<String> {
    let reference = reference_checkout();
    let mut differences = trainer_defeat_speech_differences(
        imported,
        checkout,
        &reference_trainer_defeat_speech_table(&reference),
    );
    differences.extend(sight_trainer_intro_speech_differences(
        imported,
        checkout,
        &reference_sight_trainer_intro_table(&reference),
    ));
    differences.extend(trainer_class_name_differences(
        imported,
        checkout,
        &reference_trainer_class_name_table(&reference),
    ));
    differences
}

/// The `pokeemerald/` checkout `cargo xtask extract` reads, at the repo root.
fn reference_checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/rom-import is always two levels under the repo root")
        .join("pokeemerald")
}

fn read_reference(checkout: &Path, relative: &str) -> Result<String, String> {
    let path = checkout.join(relative);
    std::fs::read_to_string(&path)
        .map_err(|err| format!("reading {} failed: {err}", path.display()))
}

/// Upstream text through the charmap, spelled here so no code under test
/// vouches for it. Covers what the class names and Route 103 defeat speeches
/// use: space `00`, `A`..`Z` from `BB`, `a`..`z` from `D5`, digits from `A1`,
/// the punctuation below, `é` `1B`, `♂` `B5`, `♀` `B6`, `{PKMN}` `53 54`,
/// `{PLAYER}` `FD 01`, `{KUN}` `FD 05`, `\n` `FE`, `\p` `FB`, `\l` `FA`, and
/// `$` (`EOS`) `FF`.
fn spell(text: &str) -> Result<Vec<u8>, String> {
    const MULTI: [(&str, &[u8]); 6] = [
        ("{PKMN}", &[0x53, 0x54]),
        ("{PLAYER}", &[0xFD, 0x01]),
        ("{KUN}", &[0xFD, 0x05]),
        ("\\n", &[0xFE]),
        ("\\p", &[0xFB]),
        ("\\l", &[0xFA]),
    ];
    let mut out = Vec::new();
    let mut rest = text;
    'outer: while let Some(c) = rest.chars().next() {
        for (token, bytes) in MULTI {
            if let Some(after) = rest.strip_prefix(token) {
                out.extend_from_slice(bytes);
                rest = after;
                continue 'outer;
            }
        }
        let offset = |base: char| u8::try_from(u32::from(c) - u32::from(base)).expect("in range");
        out.push(match c {
            ' ' => 0x00,
            'é' => 0x1B,
            '0'..='9' => 0xA1 + offset('0'),
            '!' => 0xAB,
            '?' => 0xAC,
            '.' => 0xAD,
            '-' => 0xAE,
            '…' => 0xB0,
            '"' => 0xB2,
            '\'' => 0xB4,
            '♂' => 0xB5,
            '♀' => 0xB6,
            ',' => 0xB8,
            'A'..='Z' => 0xBB + offset('A'),
            'a'..='z' => 0xD5 + offset('a'),
            ':' => 0xF0,
            '$' => 0xFF,
            other => return Err(format!("`{other}` in {text:?} has no hand spelling")),
        });
        rest = &rest[c.len_utf8()..];
    }
    Ok(out)
}

/// `count` class-name streams (`EOS` appended) in index order, from the
/// `TRAINER_CLASS_*` defines and the `[TRAINER_CLASS_*] = _("…")` rows, read
/// line by line rather than by the extractor's tokenizer.
fn reference_class_name_streams(
    constants: &str,
    names: &str,
    count: usize,
) -> Result<Vec<Vec<u8>>, String> {
    let mut indices = BTreeMap::new();
    for line in constants.lines() {
        let mut words = line.split_whitespace();
        if words.next() != Some("#define") {
            continue;
        }
        let (Some(symbol), Some(value)) = (words.next(), words.next()) else {
            continue;
        };
        if !symbol.starts_with("TRAINER_CLASS_") {
            continue;
        }
        let value = match value.strip_prefix("0x") {
            Some(hex) => usize::from_str_radix(hex, 16),
            None => value.parse(),
        }
        .map_err(|_| format!("`{symbol}` has no literal index"))?;
        indices.insert(symbol.to_owned(), value);
    }
    let mut streams = vec![None; count];
    for line in names.lines() {
        let Some((designator, value)) = line.trim().split_once("] = _(\"") else {
            continue;
        };
        let symbol = designator.trim_start_matches('[');
        let name = value
            .strip_suffix("\"),")
            .or_else(|| value.strip_suffix("\")"))
            .ok_or_else(|| format!("`{symbol}` row is not one literal"))?;
        let index = *indices
            .get(symbol)
            .ok_or_else(|| format!("`{symbol}` has no define"))?;
        let slot = streams
            .get_mut(index)
            .ok_or_else(|| format!("`{symbol}` is {index}, outside {count} classes"))?;
        let mut stream = spell(name)?;
        stream.push(0xFF);
        if slot.replace(stream).is_some() {
            return Err(format!("class {index} has two rows"));
        }
    }
    streams
        .into_iter()
        .enumerate()
        .map(|(index, stream)| stream.ok_or_else(|| format!("class {index} has no row")))
        .collect()
}

/// The text of the one `label:` (or `label::`) across `sources`: its
/// consecutive `.string` lines through the one ending in `$`, spelled.
fn reference_speech(sources: &[&str], label: &str) -> Result<Vec<u8>, String> {
    let mut found = None;
    for source in sources {
        let mut lines = source.lines();
        while let Some(line) = lines.next() {
            let line = line.trim_end();
            if line != format!("{label}:") && line != format!("{label}::") {
                continue;
            }
            if found.is_some() {
                return Err(format!("`{label}` is defined twice"));
            }
            let mut text = String::new();
            for line in lines.by_ref() {
                let Some(literal) = line
                    .trim()
                    .strip_prefix(".string \"")
                    .and_then(|rest| rest.strip_suffix('"'))
                else {
                    break;
                };
                text.push_str(literal);
                if literal.ends_with('$') {
                    break;
                }
            }
            if !text.ends_with('$') {
                return Err(format!("`{label}` has no `$`-terminated string"));
            }
            found = Some(spell(&text)?);
        }
    }
    found.ok_or_else(|| format!("`{label}` is defined in no source"))
}

/// A `magic`, schema 1, record-count header, then one `id`, `byte_len`,
/// `bytes` record per stream in index order: the layout both the `TCLN` and
/// `TDFS` codecs document, built here so a shared encoder cannot vouch for
/// itself.
fn record_table(magic: [u8; 4], streams: &[Vec<u8>]) -> Vec<u8> {
    let mut table = magic.to_vec();
    table.extend_from_slice(&1u16.to_le_bytes());
    let count = u16::try_from(streams.len()).expect("a u16 record count");
    table.extend_from_slice(&count.to_le_bytes());
    for (id, stream) in (0u16..).zip(streams) {
        table.extend_from_slice(&id.to_le_bytes());
        let len = u32::try_from(stream.len()).expect("a stream fits in u32");
        table.extend_from_slice(&len.to_le_bytes());
        table.extend_from_slice(stream);
    }
    table
}

/// The first operand after the trainer of `script`'s first command, which
/// must be a `trainerbattle_single` or `trainerbattle_double`: its intro text.
fn reference_intro_label(scripts: &str, script: &str) -> Result<String, String> {
    let mut lines = scripts.lines();
    if !lines.any(|line| line.trim_end() == format!("{script}::")) {
        return Err(format!("`{script}` is defined in no source"));
    }
    let command = lines
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| format!("`{script}` has no command"))?;
    let operands = ["trainerbattle_single ", "trainerbattle_double "]
        .iter()
        .find_map(|name| command.strip_prefix(name))
        .ok_or_else(|| format!("`{script}` does not start with a trainerbattle command"))?;
    operands
        .split(',')
        .nth(1)
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty())
        .ok_or_else(|| format!("`{script}` names no intro text"))
}

/// A `STIS` payload: header, then one `key_len`, key, `byte_len`, `bytes`
/// record per stream in [`SIGHT_TRAINER_SCRIPTS`] order; built here so a
/// shared encoder cannot vouch for itself.
fn keyed_record_table(streams: &[Vec<u8>]) -> Vec<u8> {
    let mut table = b"STIS".to_vec();
    table.extend_from_slice(&1u16.to_le_bytes());
    let count = u16::try_from(streams.len()).expect("a u16 record count");
    table.extend_from_slice(&count.to_le_bytes());
    for (script, stream) in SIGHT_TRAINER_SCRIPTS.iter().zip(streams) {
        let key_len = u16::try_from(script.len()).expect("a u16 key length");
        table.extend_from_slice(&key_len.to_le_bytes());
        table.extend_from_slice(script.as_bytes());
        let len = u32::try_from(stream.len()).expect("a stream fits in u32");
        table.extend_from_slice(&len.to_le_bytes());
        table.extend_from_slice(stream);
    }
    table
}

/// The `sight-trainer-intro-speech/table` payload derived from the local
/// checkout: each script's own `trainerbattle` intro operand, through the
/// trainer text, spelled by hand.
fn reference_sight_trainer_intro_table(checkout: &Path) -> Result<Vec<u8>, String> {
    let scripts = read_reference(checkout, "data/maps/Route103/scripts.inc")?;
    let trainers = read_reference(checkout, "data/text/trainers.inc")?;
    let streams = SIGHT_TRAINER_SCRIPTS
        .iter()
        .map(|script| {
            let label = reference_intro_label(&scripts, script)?;
            reference_speech(&[trainers.as_str()], &label)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(keyed_record_table(&streams))
}

/// The `trainer-class-name/table` payload derived from the local checkout.
fn reference_trainer_class_name_table(checkout: &Path) -> Result<Vec<u8>, String> {
    let constants = read_reference(checkout, "include/constants/trainers.h")?;
    let names = read_reference(checkout, "src/data/text/trainer_class_names.h")?;
    let streams = reference_class_name_streams(&constants, &names, 66)?;
    Ok(record_table(*b"TCLN", &streams))
}

/// The `trainer-defeat-speech/table` payload derived from the local checkout.
fn reference_trainer_defeat_speech_table(checkout: &Path) -> Result<Vec<u8>, String> {
    let sources = TRAINER_DEFEAT_SPEECH_SOURCES
        .iter()
        .map(|relative| read_reference(checkout, relative))
        .collect::<Result<Vec<_>, _>>()?;
    let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
    let streams = TRAINER_DEFEAT_SPEECH_LABELS
        .iter()
        .map(|label| reference_speech(&sources, label))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(record_table(*b"TDFS", &streams))
}

/// Invariants of the class-name payload that carry no game text: it decodes,
/// covers every class, keeps [`SHARED_CLASS_NAMES`] equal and every other
/// class distinct, and the shared `{PKMN}` name begins with the two glyphs.
fn class_name_structure(payload: &[u8]) -> Result<(), String> {
    let table = TrainerClassNameTable::decode(payload).map_err(|err| err.to_string())?;
    let names: Vec<&[u8]> = TrainerClass::all()
        .map(|class| {
            table
                .name(class)
                .ok_or(format!("class {} has no name", class.0))
        })
        .collect::<Result<_, _>>()?;
    if names.len() != 66 {
        return Err(format!("{} classes, not 66", names.len()));
    }
    for group in SHARED_CLASS_NAMES {
        let first = names[usize::from(group[0])];
        if group
            .iter()
            .any(|&index| names[usize::from(index)] != first)
        {
            return Err(format!("classes {group:?} do not share one name"));
        }
    }
    if !names[0].starts_with(&[0x53, 0x54]) {
        return Err("class 0 does not begin with the `PK` and `MN` glyphs".to_owned());
    }
    let distinct: BTreeSet<&[u8]> = names.iter().copied().collect();
    let shared: usize = SHARED_CLASS_NAMES.iter().map(|group| group.len() - 1).sum();
    if distinct.len() != names.len() - shared {
        return Err(format!(
            "{} distinct names, expected {}",
            distinct.len(),
            names.len() - shared
        ));
    }
    Ok(())
}

/// Invariants of the speech payload that carry no game text: it decodes, the
/// four `TRAINER_AMY_AND_LIV_1` contexts are four different strings, and the
/// rivals keep `{PLAYER}` (`FD 01`), May also `{KUN}` (`FD 05`), as tokens.
fn speech_structure(payload: &[u8]) -> Result<(), String> {
    let table = TrainerDefeatSpeechTable::decode(payload).map_err(|err| err.to_string())?;
    let speeches: Vec<&[u8]> = TrainerDefeatSpeechContext::ALL
        .iter()
        .map(|&context| table.speech(context))
        .collect();
    let shared: BTreeSet<&[u8]> = speeches[1..=4].iter().copied().collect();
    if shared.len() != 4 {
        return Err("the Amy and Liv speeches are not four strings".to_owned());
    }
    let has = |speech: &[u8], token: [u8; 2]| speech.windows(2).any(|pair| pair == token);
    if !has(speeches[12], [0xFD, 0x01]) || !has(speeches[12], [0xFD, 0x05]) {
        return Err("May's speech lost `{PLAYER}` or `{KUN}`".to_owned());
    }
    if !has(speeches[13], [0xFD, 0x01]) {
        return Err("Brendan's speech lost `{PLAYER}`".to_owned());
    }
    Ok(())
}

/// Invariants of the intro payload that carry no game text: it decodes, and
/// the nine scripts hold nine different strings, Amy's and Liv's included.
fn sight_trainer_intro_structure(payload: &[u8]) -> Result<(), String> {
    let table = SightTrainerIntroSpeechTable::decode(payload).map_err(|err| err.to_string())?;
    let speeches: BTreeSet<&[u8]> = SIGHT_TRAINER_SCRIPTS
        .iter()
        .map(|script| {
            table
                .speech(script)
                .ok_or(format!("{script} has no speech"))
        })
        .collect::<Result<_, _>>()?;
    if speeches.len() != SIGHT_TRAINER_SCRIPTS.len() {
        return Err("the nine intro speeches are not nine different strings".to_owned());
    }
    Ok(())
}

/// Each backend's `id` payload checked against `reference`, derived from the
/// local checkout, and against `structure`: named by id and bytes so a
/// record both backends drop, merge, reorder, or misencode together still
/// fails. An unreadable reference fails closed.
fn reference_differences(
    id: &str,
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
    reference: &Result<Vec<u8>, String>,
    structure: fn(&[u8]) -> Result<(), String>,
) -> Vec<String> {
    let reference = match reference {
        Ok(reference) => reference,
        Err(problem) => {
            return vec![format!(
                "{id}: no reference from the local checkout ({problem})"
            )]
        }
    };
    let mut differences = Vec::new();
    for (what, (bytes, entries)) in [
        ("the ROM backend", imported),
        ("the checkout pack", checkout),
    ] {
        let Some(entry) = entries.get(id) else {
            differences.push(format!("{id}: {what} wrote no such entry"));
            continue;
        };
        let payload = &bytes[entry.offset..entry.offset + entry.length];
        if let Err(problem) = structure(payload) {
            differences.push(format!("{id}: {what} wrote a malformed table: {problem}"));
        } else if payload != &reference[..] {
            differences.push(format!(
                "{id}: {what} wrote a payload other than the local checkout's"
            ));
        }
    }
    differences
}

fn trainer_class_name_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
    reference: &Result<Vec<u8>, String>,
) -> Vec<String> {
    reference_differences(
        TRAINER_CLASS_NAME_PACK_ID,
        imported,
        checkout,
        reference,
        class_name_structure,
    )
}

fn trainer_defeat_speech_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
    reference: &Result<Vec<u8>, String>,
) -> Vec<String> {
    reference_differences(
        TRAINER_DEFEAT_SPEECH_PACK_ID,
        imported,
        checkout,
        reference,
        speech_structure,
    )
}

fn sight_trainer_intro_speech_differences(
    imported: (&[u8], &BTreeMap<String, DirectoryEntry>),
    checkout: (&[u8], &BTreeMap<String, DirectoryEntry>),
    reference: &Result<Vec<u8>, String>,
) -> Vec<String> {
    reference_differences(
        SIGHT_TRAINER_INTRO_SPEECH_PACK_ID,
        imported,
        checkout,
        reference,
        sight_trainer_intro_structure,
    )
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
