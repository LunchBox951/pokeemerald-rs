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
//! [`staged_cry_ids`] names the one domain under a complete prefix whose ROM
//! producer has not landed: the species-cry table and exactly the
//! [`CRY_TABLE_LEN`] distinct samples it names, read from the checkout's own
//! table. Only those ids are not required of the ROM backend; any other
//! `audio/` id, including a stray one under the cry prefixes, stays required.
//! The staging fails as soon as the ROM backend writes any staged id, so the
//! producer's slice must remove it. Every other `audio/` id stays compared.
//!
//! [`REVIEWED_DIFFERENCES`] is the escape hatch for an id the two backends
//! genuinely cannot agree on. It is empty, and the intent is that it stays
//! empty: `title/palette/pokemon_logo` was the one candidate, and it was
//! resolved by making both backends honour upstream's own `-num_colors`
//! cut rather than by writing the difference down here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use assets::audio::{CRY_TABLE_LEN, CRY_TABLE_PACK_ID};
use assets::{CryTable, SampleId, Song, VoiceEntry, VoiceGroup};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, OUTPUT_RELATIVE_PATH};
use rom_import::FIELD_MESSAGE_PACK_ID;

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
    "audio/",
    "battle-message/",
    "field-message/",
    "battle-anim/",
    "bag/",
    "starter-chooser/",
    "pokemon/",
    "opening-scene/",
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

/// The checkout ids under a [`COMPLETE_PREFIXES`] entry whose ROM producer
/// (#1853) has not landed, read from the checkout's `audio/cry/table`
/// payload: the table itself and exactly the samples it names.
///
/// This is staging, not a reviewed difference. The set is pinned to
/// `1 + CRY_TABLE_LEN` ids, so a table that repeats or drops a sample fails
/// instead of widening or narrowing the exemption, and the main test fails
/// once the ROM backend writes any of them, so the producer's slice removes
/// the staging and the cry ids are then required like every other `audio/`
/// id.
///
/// # Errors
///
/// A description of why the payload cannot define the staged set.
fn staged_cry_ids(table_payload: &[u8]) -> Result<BTreeSet<String>, String> {
    let table = CryTable::decode(table_payload)
        .map_err(|err| format!("{CRY_TABLE_PACK_ID} does not decode: {err}"))?;
    let mut staged: BTreeSet<String> = table
        .samples()
        .iter()
        .map(|SampleId(id)| id.clone())
        .collect();
    if staged.len() != CRY_TABLE_LEN {
        return Err(format!(
            "{CRY_TABLE_PACK_ID} names {} distinct samples, not {CRY_TABLE_LEN}",
            staged.len()
        ));
    }
    staged.insert(CRY_TABLE_PACK_ID.to_owned());
    Ok(staged)
}

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

/// `field-message/table` as both backends must write it: schema header, then
/// `gText_PkmnFainted_FldPsn` (`{STR_VAR_1} fainted…\p\n`, `EOS`) spelled
/// through `charmap.txt`.
const FIELD_MESSAGE_TABLE: [u8; 28] = [
    b'F', b'M', b'S', b'G', 1, 0, 1, 0, 0, 0, 14, 0, 0, 0, 0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD,
    0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF,
];

/// Ids the two backends are known and accepted to disagree on, each with
/// the reason it was signed off.
///
/// Empty. Every entry here is a promise not kept, so adding one needs a
/// reason that survives review, not a convenient way to make this test
/// pass `(test-ratchet)`.
const REVIEWED_DIFFERENCES: &[(&str, &str)] = &[];

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

    let table = checkout
        .get(CRY_TABLE_PACK_ID)
        .unwrap_or_else(|| panic!("the checkout pack has no {CRY_TABLE_PACK_ID}"));
    let staged = staged_cry_ids(&checkout_bytes[table.offset..table.offset + table.length])
        .unwrap_or_else(|err| panic!("the checkout pack: {err}"));
    for id in &staged {
        if !checkout.contains_key(id) {
            differences.push(format!("{id}: the checkout pack wrote no such entry"));
        }
        if imported.contains_key(id) {
            differences.push(format!(
                "{id}: the ROM backend now writes this staged cry id; remove the staging (#1853)"
            ));
        }
    }

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
        let claimed = COMPLETE_PREFIXES
            .iter()
            .any(|prefix| id.starts_with(prefix));
        if claimed
            && !staged.contains(id)
            && !imported.contains_key(id)
            && !reviewed.contains_key(id.as_str())
        {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    // `battle-interface/` is not a complete prefix, so the textbox, element,
    // battle-bar, frame, shadow, and ball-palette entries are pinned by id.
    for id in BATTLE_INTERFACE_IDS {
        if !imported.contains_key(id) {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    // Named by id so an entry both backends drop together still fails.
    differences.extend(missing_from_either(
        battler_ids().into_iter().chain(checkpoint_payload_ids()),
        (&imported, &checkout),
    ));

    differences.extend(field_message_differences(
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

/// A cry table payload naming `samples` in order.
fn cry_table_payload(samples: Vec<String>) -> Vec<u8> {
    CryTable::new(samples.into_iter().map(SampleId).collect())
        .expect("a valid cry table")
        .encode()
}

/// The staged set is the table and exactly the samples it names; no other id
/// under a cry prefix, and no instrument sample, is staged.
#[test]
fn only_the_cry_table_and_its_samples_are_staged() {
    let samples: Vec<String> = (0..CRY_TABLE_LEN)
        .map(|position| format!("audio/sample/direct-sound/cry/c{position}"))
        .collect();
    let staged = staged_cry_ids(&cry_table_payload(samples)).expect("388 distinct samples");
    assert_eq!(staged.len(), CRY_TABLE_LEN + 1);
    for id in ["audio/cry/table", "audio/sample/direct-sound/cry/c0"] {
        assert!(staged.contains(id), "{id}");
    }
    for id in [
        "audio/cry/table_reverse",
        "audio/sample/direct-sound/cry/unnamed",
        "audio/sample/direct-sound/sc88pro_flute",
        "audio/sample/direct-sound/cryptic",
        "audio/sample/programmable-wave/01",
        "audio/voicegroup/title",
        "audio/song/mus_title",
        "title/image/pokemon_logo",
    ] {
        assert!(!staged.contains(id), "{id}");
    }
}

/// A table that repeats a sample cannot shrink the pinned staged count.
#[test]
fn a_cry_table_with_a_repeated_sample_does_not_define_the_staged_set() {
    let mut samples: Vec<String> = (0..CRY_TABLE_LEN)
        .map(|position| format!("audio/sample/direct-sound/cry/c{position}"))
        .collect();
    samples[1] = samples[0].clone();
    let err = staged_cry_ids(&cry_table_payload(samples)).unwrap_err();
    assert!(err.contains("387 distinct samples"), "{err}");
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

/// One difference per `ids` entry that `imported` or `checkout` lacks.
fn missing_from_either(
    ids: impl IntoIterator<Item = String>,
    (imported, checkout): (
        &BTreeMap<String, DirectoryEntry>,
        &BTreeMap<String, DirectoryEntry>,
    ),
) -> Vec<String> {
    let mut differences = Vec::new();
    for id in ids {
        for (what, entries) in [
            ("the ROM backend", imported),
            ("the checkout pack", checkout),
        ] {
            if !entries.contains_key(&id) {
                differences.push(format!("{id}: {what} wrote no such entry"));
            }
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
