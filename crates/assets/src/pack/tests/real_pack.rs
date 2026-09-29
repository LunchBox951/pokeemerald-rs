//! The ignored real-pack integration: loads the actual local pack
//! (`cargo xtask extract`'s output) and proves the extraction pipeline and
//! this reader agree byte-for-byte on the format, not just on the synthetic
//! fixtures the rest of this suite uses. Needs a local pack; run
//! `cargo xtask extract` first, then `cargo test -p assets -- --ignored`.

use super::super::AssetPack;
use crate::audio::{SampleId, SongEvent, VoiceGroupId};

/// Loads the *real* local pack (`cargo xtask extract`'s output) and
/// exercises every typed accessor against it -- proof the extraction
/// pipeline (`xtask::extract`, writing through `pack_format::PackWriter`)
/// and this reader agree byte-for-byte on the format, not just on the
/// synthetic fixtures above. Needs a local pack:
/// run `cargo xtask extract` first, then `cargo test -p assets -- --ignored`.
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end walk over every typed accessor; a split scatters one round-trip claim across ignored tests"
)]
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_loads_and_every_typed_accessor_works() {
    use crate::fonts::FontId;
    use crate::map_layouts::{BorderGrid, LayoutId, LayoutTable};

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");

    let general = pack
        .tileset("general")
        .expect("general tileset should be in the pack");
    assert!(general.tiles.width > 0 && general.tiles.height > 0);
    assert_eq!(
        general.tiles.pixels.len(),
        general.tiles.width as usize * general.tiles.height as usize
    );
    for palette in &general.palettes {
        assert_eq!(palette.color_count, 16);
        assert_eq!(palette.colors().count(), 16);
    }
    assert!(!general.metatiles.is_empty());
    assert!(!general.metatile_attributes.is_empty());

    for name in [
        "general",
        "building",
        "petalburg",
        "brendans_mays_house",
        "lab",
    ] {
        pack.tileset(name)
            .unwrap_or_else(|e| panic!("tileset `{name}` should load: {e}"));
    }

    let walking = pack
        .sprite("brendan/walking")
        .expect("brendan/walking sprite");
    assert!(walking.width > 0 && walking.height > 0);
    let brendan_palette = pack.sprite_palette("brendan").expect("brendan palette");
    assert_eq!(brendan_palette.color_count, 16);
    pack.sprite_palette("may").expect("may palette");

    let logo = pack
        .image("title/image/pokemon_logo")
        .expect("title logo image");
    assert!(logo.width > 0 && logo.height > 0);
    pack.palette("title/palette/pokemon_logo")
        .expect("title logo palette");
    pack.raw("title/raw/pokemon_logo")
        .expect("title logo raw tilemap");

    // `general`'s metatile_attributes should decode cleanly end to end
    // (every real upstream layer-type value is 0..=2).
    let attr_table = general.metatile_attribute_table();
    assert!(!attr_table.is_empty());
    for attr in attr_table.attributes() {
        attr.unwrap_or_else(|e| panic!("metatile attribute decode failed: {e}"));
    }

    // Every Littleroot Town layout's map/border bytes should be present and
    // decode through `crate::map_layouts`'s typed grid views, using the
    // hand-transcribed `LayoutTable` metadata for dimensions -- plus the
    // three connection targets outside that family this pipeline bundles:
    // `LAYOUT_ROUTE101` (issue #177), `LAYOUT_OLDALE_TOWN`/`LAYOUT_ROUTE103`
    // (issue #248), mirroring `crates/xtask/src/extract/mod.rs`'s own
    // `LAYOUTS` table exactly.
    let table = LayoutTable::new();
    for (layout_id, pack_name) in [
        ("LAYOUT_LITTLEROOT_TOWN", "littleroot_town"),
        (
            "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
            "littleroot_town_brendans_house_1f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
            "littleroot_town_brendans_house_2f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
            "littleroot_town_mays_house_1f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
            "littleroot_town_mays_house_2f",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
            "littleroot_town_professor_birchs_lab",
        ),
        (
            "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
            "littleroot_town_professor_birchs_lab_with_table",
        ),
        ("LAYOUT_ROUTE101", "route101"),
        ("LAYOUT_OLDALE_TOWN", "oldale_town"),
        ("LAYOUT_ROUTE103", "route103"),
    ] {
        let layout = table
            .layout(LayoutId(layout_id))
            .unwrap_or_else(|e| panic!("{layout_id} should be in LayoutTable: {e}"));
        let map_bytes = pack
            .layout_map(pack_name)
            .unwrap_or_else(|e| panic!("layout/{pack_name}/map should be in the pack: {e}"));
        let grid = layout
            .grid(map_bytes)
            .unwrap_or_else(|e| panic!("{layout_id}'s map.bin should decode: {e}"));
        assert_eq!(grid.cell_count(), layout.cell_count());

        let border_bytes = pack
            .layout_border(pack_name)
            .unwrap_or_else(|e| panic!("layout/{pack_name}/border should be in the pack: {e}"));
        let border = BorderGrid::new(border_bytes)
            .unwrap_or_else(|e| panic!("{layout_id}'s border.bin should decode: {e}"));
        assert_eq!(border.cells().count(), crate::map_layouts::BORDER_CELLS);
    }

    // Every Latin font's glyph sheet should be present and decode through
    // `crate::fonts::FontGlyphSheet`, glyph 0 (the space) included.
    for font in FontId::ALL {
        let image = pack
            .font(font)
            .unwrap_or_else(|e| panic!("font `{}` should be in the pack: {e}", font.pack_name()));
        assert_eq!(image.image().width, crate::fonts::SHEET_WIDTH);
        assert_eq!(image.image().height, crate::fonts::SHEET_HEIGHT);
        let sheet = crate::fonts::FontGlyphSheet::new(image)
            .unwrap_or_else(|e| panic!("font `{}` sheet shape: {e}", font.pack_name()));
        let glyph = sheet
            .glyph(0)
            .unwrap_or_else(|| panic!("font `{}` glyph 0 should decode", font.pack_name()));
        assert_eq!(glyph.advance_width, font.glyph_width(0).unwrap());
    }

    // Every numbered text-window border frame should bundle a tile bitmap
    // with a 16-colour palette read out of its own PNG `PLTE` chunk, the
    // default message-box frame likewise, and the four extra textbox
    // palettes should each be a 16-colour palette too.
    for frame_id in 0..20u8 {
        let frame = pack.text_window_frame(frame_id).unwrap_or_else(|e| {
            panic!("text-window frame id {frame_id} should be in the pack: {e}")
        });
        assert!(frame.tiles.width > 0 && frame.tiles.height > 0);
        assert_eq!(frame.palette.color_count, 16);
        assert_eq!(frame.palette.colors().count(), 16);
    }
    let message_box = pack
        .message_box()
        .expect("message-box frame should be in the pack");
    assert!(message_box.tiles.width > 0 && message_box.tiles.height > 0);
    assert_eq!(message_box.palette.color_count, 16);
    for n in 1..=4u8 {
        let palette = pack
            .text_window_extra_palette(n)
            .unwrap_or_else(|e| panic!("text-window extra palette {n} should be in the pack: {e}"));
        assert_eq!(palette.color_count, 16);
    }
}

/// The `DirectSound` sample basenames `xtask::extract::audio_samples`
/// writes for `mus_title`'s voicegroup, as
/// `audio/sample/direct-sound/<basename>` pack ids. Spelled out here rather
/// than imported: `crates/assets` deliberately does not depend on
/// `crates/xtask`, and that independence is exactly what makes the test
/// below a real producer/consumer pin rather than a self-consistency check.
const REAL_PACK_DIRECT_SOUND_SAMPLES: [&str; 33] = [
    "sc88pro_flute",
    "sc88pro_french_horn_60",
    "sc88pro_french_horn_72",
    "sc88pro_glockenspiel",
    "sc88pro_harp",
    "sc88pro_mute_high_conga",
    "sc88pro_open_low_conga",
    "sc88pro_orchestra_cymbal_crash",
    "sc88pro_orchestra_snare",
    "sc88pro_piano1_48",
    "sc88pro_piano1_60",
    "sc88pro_piano1_72",
    "sc88pro_piano1_84",
    "sc88pro_rnd_kick",
    "sc88pro_rnd_snare",
    "sc88pro_string_ensemble_60",
    "sc88pro_string_ensemble_72",
    "sc88pro_string_ensemble_84",
    "sc88pro_tambourine",
    "sc88pro_timpani",
    "sc88pro_tr909_hand_clap",
    "sc88pro_trumpet_60",
    "sc88pro_trumpet_72",
    "sc88pro_trumpet_84",
    "sc88pro_tuba_39",
    "sc88pro_tuba_51",
    "sc88pro_tubular_bell",
    "sc88pro_xylophone",
    "trinity_cymbal_crash",
    "unknown_bell",
    "unknown_close_hihat",
    "unknown_open_hihat",
    "unused_sc55_tom",
];

/// The programmable-wave table numbers the same pipeline writes, as
/// `audio/sample/programmable-wave/<NN>` pack ids.
const REAL_PACK_PROGRAMMABLE_WAVES: [u32; 4] = [1, 2, 5, 6];

/// The producer/consumer round-trip pin for `audio/sample/*`.
///
/// The extractor encodes these payloads with its *own* copy of this
/// schema's wire format (`xtask::extract::audio_samples`'s
/// `encode_direct_sound`/`encode_programmable_wave`, deliberately duplicated
/// rather than shared — see that module's docs), so nothing short of
/// decoding a real pack proves the two sides still agree: each side's unit
/// tests only pin its own understanding of the layout, and this is what
/// catches them drifting apart.
///
/// Beyond "all 37 ids present and structurally decodable", two entries are
/// pinned to concrete values, so a silent change in the *derivation* — not
/// just in the framing — fails here too:
///
/// - `sc88pro_flute` — `base_frequency` `3_425_024` is that sample's `agbp`
///   override verbatim, deliberately *not* `sample_rate * 1024`
///   (3344 * 1024 = `3_424_256`); `loop_start` 1312 is its `smpl` loop
///   start; `sample_count()` 1874 is its `agbl` override, one less than the
///   naive loop end (inclusive `smpl` end 1874, plus one, = 1875). `data()`
///   holds 1875 values: wav2agb's payload writer still emits sample 1874
///   (`-8`, equal to the loop-start sample), and the pack retains it as the
///   interpolation guard. All read straight off
///   `sound/direct_sound_samples/sc88pro_flute.wav`'s own chunks and match
///   `tools/wav2agb`'s `converter.cpp:77-90,392-402` arithmetic.
/// - `programmable-wave/01` — the 16 bytes of
///   `sound/programmable_wave_samples/01.pcm`, copied through unchanged.
///
/// Needs a local pack: run `cargo xtask extract` first, then
/// `cargo test -p assets -- --ignored` (CI's Ubuntu `native` leg does
/// exactly this).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_audio_samples_decode_through_the_sample_schema() {
    use crate::audio::Sample;

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");

    let mut decoded = 0usize;
    for name in REAL_PACK_DIRECT_SOUND_SAMPLES {
        let id = format!("audio/sample/direct-sound/{name}");
        let bytes = pack
            .raw(&id)
            .unwrap_or_else(|e| panic!("`{id}` should be in the pack: {e}"));
        let Sample::DirectSound(sample) =
            Sample::decode(bytes).unwrap_or_else(|e| panic!("`{id}` should decode: {e}"))
        else {
            panic!("`{id}` should decode as a DirectSound sample, not a wave table");
        };
        assert!(!sample.data().is_empty(), "`{id}` should carry PCM");
        assert_ne!(sample.base_frequency, 0, "`{id}` should carry a pitch word");
        if let Some(start) = sample.loop_start() {
            let start = usize::try_from(start).expect("a real loop start fits a usize");
            let count = usize::try_from(sample.sample_count()).expect("a real count fits a usize");
            assert!(
                start < count,
                "`{id}`'s loop start {start} is past its {count} logical samples"
            );
        }
        assert_eq!(
            sample.data().len(),
            usize::try_from(sample.sample_count()).expect("a real count fits a usize") + 1,
            "`{id}` should retain exactly one interpolation-guard sample"
        );
        decoded += 1;
    }

    for n in REAL_PACK_PROGRAMMABLE_WAVES {
        let id = format!("audio/sample/programmable-wave/{n:02}");
        let bytes = pack
            .raw(&id)
            .unwrap_or_else(|e| panic!("`{id}` should be in the pack: {e}"));
        let Sample::ProgrammableWave(_) =
            Sample::decode(bytes).unwrap_or_else(|e| panic!("`{id}` should decode: {e}"))
        else {
            panic!("`{id}` should decode as a programmable-wave table, not PCM");
        };
        decoded += 1;
    }
    // Both loops iterate fixed const arrays and count unconditionally, so
    // this pins the expected-id lists' lengths (33 + 4), not decoding --
    // the decode coverage is the loop bodies above.
    assert_eq!(
        decoded, 37,
        "the expected-id lists must cover all 37 samples"
    );

    let flute_bytes = pack
        .raw("audio/sample/direct-sound/sc88pro_flute")
        .expect("the flute sample should be in the pack");
    let Sample::DirectSound(flute) =
        Sample::decode(flute_bytes).expect("the flute sample should decode")
    else {
        panic!("the flute sample should decode as a DirectSound sample");
    };
    assert_eq!(flute.base_frequency, 3_425_024);
    assert_eq!(flute.loop_start(), Some(1312));
    // `agbl` sets the logical count to 1874; the pack retains the one
    // encoded sample past it for the mixer's final interpolation step,
    // which for this loop is the loop-start value.
    assert_eq!(flute.sample_count(), 1874);
    assert_eq!(flute.data().len(), 1875);
    assert_eq!(flute.data()[1874], -8);
    assert_eq!(flute.data()[1874], flute.data()[1312]);

    let wave_bytes = pack
        .raw("audio/sample/programmable-wave/01")
        .expect("programmable-wave 01 should be in the pack");
    let Sample::ProgrammableWave(wave) =
        Sample::decode(wave_bytes).expect("programmable-wave 01 should decode")
    else {
        panic!("programmable-wave 01 should decode as a wave table");
    };
    assert_eq!(
        wave.table,
        [
            0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9, 0x63, 0x10, 0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9,
            0x63, 0x10
        ]
    );
}

/// The producer/consumer round-trip pin for `audio/song/mus_title` (issue
/// #181, S-4, `#115` child 2): `xtask::extract::midi` encodes this payload
/// with its own copy of `crate::audio::song::Song`'s wire format
/// (`xtask::extract::midi::encode`, deliberately duplicated rather than
/// shared -- see that module's docs), so nothing short of decoding a real
/// pack proves the two sides still agree.
///
/// Values are hand-verified against a locally built `tools/mid2agb` oracle
/// (`xtask::extract::midi::compile`'s module docs cite the exact citations);
/// the same concrete values are independently pinned on the producer side by
/// `xtask::extract::midi`'s own
/// `mus_title_compiles_to_hand_verified_values` test, which is what would
/// catch the two encoders drifting apart from either direction.
///
/// Needs a local pack: run `cargo xtask extract` first, then
/// `cargo test -p assets -- --ignored` (CI's Ubuntu `native` leg does
/// exactly this).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_audio_song_decodes_through_the_song_schema() {
    use crate::audio::{Song, SongEvent, VoiceGroupId};

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let bytes = pack
        .raw("audio/song/mus_title")
        .expect("`audio/song/mus_title` should be in the pack");
    let song = Song::decode(bytes).expect("`audio/song/mus_title` should decode");

    assert_eq!(
        song.voicegroup(),
        &VoiceGroupId("audio/voicegroup/title".to_owned())
    );
    assert_eq!(song.priority(), 0);
    assert_eq!(song.reverb(), Some(50));
    assert_eq!(song.tracks().len(), 10);

    let track0 = &song.tracks()[0];
    assert_eq!(
        &track0[..6],
        [
            SongEvent::KeyShift(0),
            SongEvent::Tempo(144),
            SongEvent::Voice(14),
            SongEvent::Pan(40),
            SongEvent::LfoSpeed(44),
            SongEvent::Volume(86),
        ]
    );
    let last = track0.len();
    assert_eq!(
        &track0[last - 5..],
        [
            SongEvent::Volume(9),
            SongEvent::Wait(4),
            SongEvent::Volume(8),
            SongEvent::Wait(4),
            SongEvent::Fine,
        ]
    );

    // No loop markers anywhere in mus_title.mid.
    let goto_count = song
        .tracks()
        .iter()
        .flatten()
        .filter(|e| matches!(e, SongEvent::Goto(_)))
        .count();
    assert_eq!(goto_count, 0);

    // Three `XCMD xIECV`/`xIECL` pairs total (pseudo-echo volumes 10, 10,
    // 16; length 12 each time).
    let volumes: Vec<u8> = song
        .tracks()
        .iter()
        .flatten()
        .filter_map(|e| match e {
            SongEvent::PseudoEchoVolume(v) => Some(*v),
            _ => None,
        })
        .collect();
    assert_eq!(volumes, vec![10, 10, 16]);
}

/// The full `MUS_TITLE` data-chain round-trip through the typed accessors
/// this module adds (S-4, issue #184, `#115` child 5): load
/// `audio/song/mus_title` through [`AssetPack::song`], resolve every
/// voicegroup [`AssetPack::voicegroup`] can reach from the song's own
/// [`Song::voicegroup`] -- transitively, through every
/// [`VoiceEntry::KeySplit`]/[`VoiceEntry::Rhythm`] indirection a resolved
/// group carries -- and decode every sample
/// [`VoiceEntry::DirectSound`]/[`VoiceEntry::ProgrammableWave`] leaf
/// references through [`AssetPack::sample`]. Where
/// [`real_pack_audio_song_decodes_through_the_song_schema`] and
/// [`real_pack_audio_samples_decode_through_the_sample_schema`] each pin one
/// schema's own producer/consumer agreement in isolation (`pack.raw` + a
/// manual `decode` call), and
/// `crates/pokeemerald-rs/src/voicegroup_pack_tests.rs`'s
/// `every_id_a_voicegroup_references_resolves_to_a_pack_entry` walks the
/// voicegroup graph checking only that referenced ids are *present*
/// (`pack.raw(id).is_err()`), this test is the thing none of them are: proof
/// that the *typed* accessors this module adds chain together end to end --
/// a caller reaching for `pack.song(..)` can walk straight through
/// `pack.voicegroup(..)`/`pack.sample(..)` without ever touching `pack.raw`
/// or a schema's `decode` directly, and every step of that chain, for this
/// one real song, actually holds together.
///
/// **Not a playability claim.** This is data-chain integrity only: every
/// reference resolves and every payload decodes to its schema's structural
/// shape. It does not sequence events, mix a waveform, or produce audio --
/// that is issue #185 (`#115` child 6), a separate slice.
///
/// The expected voicegroup label set (7: `title` plus its 6 keysplit/rhythm
/// children) and sample count (37: 33 `DirectSound` + 4 programmable-wave)
/// mirror `xtask::extract::voicegroups`'s and
/// `xtask::extract::audio_samples`'s own module docs, which hand-traced and
/// hand-verified exactly this same closure from the upstream sources -- see
/// [`REAL_PACK_DIRECT_SOUND_SAMPLES`]/[`REAL_PACK_PROGRAMMABLE_WAVES`]'s docs
/// for that trail. This test does not re-derive the set; it walks the real
/// pack's own data and checks the walk lands on the same numbers, which is
/// what would catch the resolver and the sample-extraction list drifting
/// apart from each other via this crate's own read side.
///
/// Needs a local pack: run `cargo xtask extract` first, then
/// `cargo test -p assets -- --ignored` (CI's Ubuntu `native` leg does
/// exactly this).
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end walk from song to every voicegroup and sample; a split scatters one chain claim across ignored tests"
)]
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_mus_title_data_chain_round_trips_through_the_typed_accessors() {
    use std::collections::HashSet;

    use crate::audio::VoiceEntry;

    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");

    // 1. The song, through the typed accessor.
    let song = pack
        .song("mus_title")
        .expect("`audio/song/mus_title` should load through `AssetPack::song`");

    // Every distinct VOICE index `mus_title`'s own event streams select, so
    // the closure below can confirm the top-level group actually has a slot
    // for each one -- not just that the group as a whole decodes.
    let mut selected_voices: Vec<u8> = song
        .tracks()
        .iter()
        .flatten()
        .filter_map(|event| match event {
            SongEvent::Voice(index) => Some(*index),
            _ => None,
        })
        .collect();
    selected_voices.sort_unstable();
    selected_voices.dedup();
    assert!(
        !selected_voices.is_empty(),
        "mus_title should select at least one VOICE"
    );

    // 2. Every voicegroup slot the song references, resolved transitively
    // through `AssetPack::voicegroup` -- a worklist walk starting at the
    // song's own top-level group, following every KeySplit/Rhythm child
    // reference outward, decoding each group exactly once.
    let mut visited_groups: HashSet<VoiceGroupId> = HashSet::new();
    let mut pending_groups = vec![song.voicegroup().clone()];
    let mut visited_samples: HashSet<SampleId> = HashSet::new();
    let mut leaf_voice_count = 0usize;

    while let Some(group_id) = pending_groups.pop() {
        if !visited_groups.insert(group_id.clone()) {
            continue; // already resolved (e.g. `rs_drumset`, reached twice).
        }
        let group = pack
            .voicegroup(&group_id)
            .unwrap_or_else(|e| panic!("`{}` should resolve: {e}", group_id.0));
        assert_eq!(
            group.slots().len(),
            crate::audio::VOICE_SLOT_COUNT,
            "`{}` should carry the full 128-slot normalization",
            group_id.0
        );
        for slot in group.slots() {
            match slot {
                // 3. Every sample a concrete leaf voice references, decoded
                // through `AssetPack::sample`.
                VoiceEntry::DirectSound(voice) => {
                    if visited_samples.insert(voice.sample.clone()) {
                        pack.sample(&voice.sample)
                            .unwrap_or_else(|e| panic!("`{}` should decode: {e}", voice.sample.0));
                    }
                    leaf_voice_count += 1;
                }
                VoiceEntry::ProgrammableWave(voice) => {
                    if visited_samples.insert(voice.wave.clone()) {
                        pack.sample(&voice.wave)
                            .unwrap_or_else(|e| panic!("`{}` should decode: {e}", voice.wave.0));
                    }
                    leaf_voice_count += 1;
                }
                VoiceEntry::Square1(_) | VoiceEntry::Square2(_) | VoiceEntry::Noise(_) => {
                    leaf_voice_count += 1; // concrete CGB voices, no sample to resolve.
                }
                VoiceEntry::KeySplit(voice) => pending_groups.push(voice.children.clone()),
                VoiceEntry::Rhythm(voice) => pending_groups.push(voice.children.clone()),
                VoiceEntry::Empty => {}
            }
        }
    }

    let mut visited_labels: Vec<&str> = visited_groups
        .iter()
        .map(|id| {
            id.0.strip_prefix("audio/voicegroup/")
                .unwrap_or_else(|| panic!("`{}` should carry the audio/voicegroup/ prefix", id.0))
        })
        .collect();
    visited_labels.sort_unstable();
    let mut expected_labels = [
        "french_horn_keysplit",
        "piano_keysplit",
        "rs_drumset",
        "strings_keysplit",
        "title",
        "trumpet_keysplit",
        "tuba_keysplit",
    ];
    expected_labels.sort_unstable();
    assert_eq!(
        visited_labels, expected_labels,
        "the voicegroup closure reached from mus_title's own song entry should be exactly \
         the 7 groups `xtask::extract::voicegroups` documents"
    );
    assert!(
        leaf_voice_count > 0,
        "the walk should have resolved at least one concrete (non-indirection) voice"
    );
    assert_eq!(
        visited_samples.len(),
        37,
        "the voicegroup closure should reference exactly the 37 samples \
         `xtask::extract::audio_samples` documents (33 DirectSound + 4 programmable-wave)"
    );

    // Every VOICE index `mus_title.mid` actually selects should resolve to a
    // real (non-[`VoiceEntry::Empty`]) voice in the top-level `title` group
    // -- including slot 127, which is populated only through the
    // link-adjacency overflow mechanism issue #201 modeled (see
    // `xtask::extract::voicegroups`'s module docs). Pinned on the *entry*,
    // not mere slot presence: every visited group already carries all
    // `VOICE_SLOT_COUNT` slots, so `is_some()` alone could not catch that
    // overflow fill regressing to an empty placeholder.
    let title = pack
        .voicegroup(song.voicegroup())
        .expect("the top-level `title` group should already be in `visited_groups`");
    for index in selected_voices {
        let entry = title
            .slot(usize::from(index))
            .unwrap_or_else(|| panic!("VOICE {index} has no slot in `title`"));
        assert!(
            !matches!(entry, VoiceEntry::Empty),
            "VOICE {index}, selected by mus_title's own event stream, resolves to an \
             empty placeholder in `title`"
        );
    }
}
