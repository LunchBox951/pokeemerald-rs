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
//! [`REVIEWED_DIFFERENCES`] is the escape hatch for an id the two backends
//! genuinely cannot agree on. It is empty, and the intent is that it stays
//! empty: `title/palette/pokemon_logo` was the one candidate, and it was
//! resolved by making both backends honour upstream's own `-num_colors`
//! cut rather than by writing the difference down here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use assets::{AssetPack, Song, VoiceEntry, VoiceGroup};
use pack_format::{parse_directory, DirectoryEntry, EntryKind, OUTPUT_RELATIVE_PATH};

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
    "battle-anim/",
    "bag/",
    "starter-chooser/",
    "opening-scene/",
    "evolution-scene/",
];

/// The starters' static battler entries: 12 ids the ROM backend must write
/// although `pokemon/` is not yet in [`COMPLETE_PREFIXES`].
const STARTER_BATTLER_IDS: [&str; 12] = [
    "pokemon/treecko/front",
    "pokemon/treecko/back",
    "pokemon/treecko/palette/normal",
    "pokemon/treecko/palette/shiny",
    "pokemon/torchic/front",
    "pokemon/torchic/back",
    "pokemon/torchic/palette/normal",
    "pokemon/torchic/palette/shiny",
    "pokemon/mudkip/front",
    "pokemon/mudkip/back",
    "pokemon/mudkip/palette/normal",
    "pokemon/mudkip/palette/shiny",
];

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
        if claimed && !imported.contains_key(id) && !reviewed.contains_key(id.as_str()) {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    // `pokemon/` is not a complete prefix while only the starters exist, so
    // their entries are pinned by id: a dropped root fails here.
    for id in STARTER_BATTLER_IDS {
        if !imported.contains_key(id) {
            differences.push(format!("{id}: the ROM backend wrote no such entry"));
        }
    }

    // Named by id so a payload both backends drop together still fails.
    for id in checkpoint_payload_ids()
        .into_iter()
        .chain(EVOLUTION_SCENE_IDS.map(str::to_owned))
    {
        for (what, ids) in [
            ("the ROM backend", &imported),
            ("the checkout pack", &checkout),
        ] {
            if !ids.contains_key(&id) {
                differences.push(format!("{id}: {what} wrote no such entry"));
            }
        }
    }

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
