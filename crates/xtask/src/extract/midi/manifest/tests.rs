//! The manifest against the audited call set, the pinned checkout, and the extraction pipeline.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use assets::audio::{MusicPlayerId, Song};
use assets::AssetPack;
use pack_format::PackWriter;

use super::audit::AUDITED_CALL_SITES;
use super::{check_manifest, SongSource, CHECKPOINT_SONGS};
use crate::extract::checkpoint_maps_tests::CHECKPOINT_MAPS;
use crate::extract::midi::error::MidiError;
use crate::extract::midi::{extract_listed_songs, extract_songs, SONG_PACK_ID};
use crate::extract::{read_text, repo_root, upstream_present, ExtractError};

/// A `mus_dummy.mid` byte image: format 1, 24 ticks per beat, and one track holding only its
/// end-of-track marker.
const SILENT_MIDI: [u8; 26] = [
    0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 1, 0, 1, 0, 24, // MThd
    0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 4, 0, 0xFF, 0x2F, 0, // MTrk
];

/// A song of one note on channel 0: a note-on at tick 0, its note-off at tick 24.
const ONE_NOTE_MIDI: [u8; 34] = [
    0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 1, 0, 1, 0, 24, // MThd
    0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 12, // MTrk
    0, 0x90, 60, 100, // note on
    24, 0x80, 60, 0, // note off
    0, 0xFF, 0x2F, 0, // end of track
];

const TABLE: &str = "\t.equiv MUSIC_PLAYER_BGM,0\n\t.equiv MUSIC_PLAYER_SE1,1\n\
    gSongTable::\n\tsong mus_dummy, MUSIC_PLAYER_BGM, 0\n\tsong se_beep, MUSIC_PLAYER_SE1, 1\n";

const CFG: &str = "mus_dummy.mid: -E -R40\nse_beep.mid: -E -R50 -G_rs_sfx_1 -V110 -P5\n";

fn lowercase_constants() -> BTreeSet<String> {
    AUDITED_CALL_SITES
        .iter()
        .map(|(constant, _)| constant.to_ascii_lowercase())
        .collect()
}

/// The audited constants no manifest song covers.
fn unlisted_audited_songs(sources: &[SongSource]) -> Vec<String> {
    let listed: BTreeSet<&str> = sources.iter().map(|source| source.symbol).collect();
    lowercase_constants()
        .into_iter()
        .filter(|symbol| !listed.contains(symbol.as_str()))
        .collect()
}

/// The manifest songs no audited call site justifies.
fn unaudited_manifest_songs(sources: &[SongSource]) -> Vec<&'static str> {
    let audited = lowercase_constants();
    sources
        .iter()
        .map(|source| source.symbol)
        .filter(|symbol| !audited.contains(*symbol))
        .collect()
}

fn without(symbol: &str) -> Vec<SongSource> {
    CHECKPOINT_SONGS
        .iter()
        .copied()
        .filter(|source| source.symbol != symbol)
        .collect()
}

#[test]
fn the_manifest_is_exactly_the_audited_checkpoint_call_set() {
    assert_eq!(
        unlisted_audited_songs(&CHECKPOINT_SONGS),
        Vec::<String>::new()
    );
    assert_eq!(
        unaudited_manifest_songs(&CHECKPOINT_SONGS),
        Vec::<&str>::new()
    );
}

#[test]
fn dropping_a_manifest_song_leaves_its_audited_call_unlisted() {
    for symbol in [
        "mus_encounter_interviewer",
        "se_wall_hit",
        "se_m_sketch",
        "mus_dummy",
    ] {
        assert_eq!(
            unlisted_audited_songs(&without(symbol)),
            vec![symbol.to_owned()],
            "dropping {symbol}"
        );
    }
}

#[test]
fn a_song_without_an_audited_call_is_reported() {
    let mut sources = CHECKPOINT_SONGS.to_vec();
    sources.push(SongSource::new("se_bike_bell", 1));
    assert_eq!(unaudited_manifest_songs(&sources), vec!["se_bike_bell"]);
}

#[test]
fn every_audited_constant_names_at_least_one_call_site() {
    for (constant, sites) in AUDITED_CALL_SITES {
        assert!(!sites.is_empty(), "{constant} has no call site");
    }
}

#[test]
fn the_manifest_is_well_formed_and_carries_the_title_the_rom_profile_locates() {
    assert_eq!(check_manifest(&CHECKPOINT_SONGS), Ok(()));
    let title = CHECKPOINT_SONGS
        .iter()
        .find(|source| source.symbol == "mus_title")
        .expect("the title is a checkpoint song");
    assert_eq!(title.pack_id(), SONG_PACK_ID);
    assert_eq!(title.midi_filename(), "mus_title.mid");
}

#[test]
fn players_follow_the_song_table_not_the_song_name_or_call() {
    let player_of = |symbol: &str| {
        CHECKPOINT_SONGS
            .iter()
            .find(|source| source.symbol == symbol)
            .map(|source| source.player)
    };
    for (symbol, player) in [
        ("se_low_health", 3),
        ("mus_evolution_intro", 2),
        ("mus_heal", 2),
        ("mus_rg_caught_intro", 2),
        ("se_m_gust", 2),
        ("se_m_earthquake", 2),
        ("se_select", 2),
        ("se_pc_on", 1),
        ("mus_title", 0),
    ] {
        assert_eq!(player_of(symbol), Some(player), "{symbol}");
    }
}

#[test]
fn a_malformed_symbol_is_rejected() {
    for symbol in [
        "",
        "mus_",
        "se_",
        "title",
        "MUS_TITLE",
        "mus_Title",
        "se_a/b",
        "../mus_title",
    ] {
        assert_eq!(
            check_manifest(&[SongSource::new(symbol, 0)]),
            Err(MidiError::ManifestSymbolMalformed(symbol.to_owned())),
            "{symbol:?}"
        );
    }
}

#[test]
fn a_symbol_listed_twice_is_rejected() {
    let twice = [
        SongSource::new("se_select", 2),
        SongSource::new("se_select", 2),
    ];
    assert_eq!(
        check_manifest(&twice),
        Err(MidiError::ManifestDuplicateSong("se_select".to_owned()))
    );
}

/// A scratch upstream tree unique to one test.
struct FakeUpstream(PathBuf);

impl FakeUpstream {
    fn new(name: &str, table: &str, cfg: &str, midis: &[(&str, &[u8])]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-song-manifest-{name}-{}",
            std::process::id()
        ));
        let midi_dir = root.join("sound/songs/midi");
        std::fs::create_dir_all(&midi_dir).expect("scratch directory");
        std::fs::write(root.join("sound/song_table.inc"), table).expect("table");
        std::fs::write(midi_dir.join("midi.cfg"), cfg).expect("cfg");
        for (filename, bytes) in midis {
            std::fs::write(midi_dir.join(filename), bytes).expect("midi");
        }
        Self(root)
    }

    fn extract(&self, sources: &[SongSource]) -> Result<Vec<u8>, ExtractError> {
        let mut writer = PackWriter::new();
        extract_listed_songs(&self.0, &mut writer, sources)?;
        Ok(writer.finish().expect("packing"))
    }

    fn pack(&self, sources: &[SongSource]) -> AssetPack {
        let bytes = self.extract(sources).expect("extraction");
        let path = self.0.join("pack.bin");
        std::fs::write(&path, bytes).expect("pack file");
        AssetPack::load(&path).expect("pack loads")
    }
}

impl Drop for FakeUpstream {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

const BEEP: SongSource = SongSource::new("se_beep", 1);
const SILENCE: SongSource = SongSource::silent_default_group("mus_dummy", 0);

fn both_midis() -> [(&'static str, &'static [u8]); 2] {
    [
        ("mus_dummy.mid", &SILENT_MIDI),
        ("se_beep.mid", &ONE_NOTE_MIDI),
    ]
}

#[test]
fn a_silent_song_without_a_voicegroup_flag_takes_the_upstream_default() {
    let upstream = FakeUpstream::new("silent", TABLE, CFG, &both_midis());
    let pack = upstream.pack(&[SILENCE, BEEP]);

    let silence = pack.song("mus_dummy").expect("silence decodes");
    assert_eq!(silence.voicegroup().0, "audio/voicegroup/dummy");
    assert_eq!(silence.player(), MusicPlayerId(0));
    assert_eq!((silence.priority(), silence.reverb()), (0, Some(40)));
    assert!(silence.tracks().is_empty());

    let beep = pack.song("se_beep").expect("beep decodes");
    assert_eq!(beep.voicegroup().0, "audio/voicegroup/rs_sfx_1");
    assert_eq!(beep.player(), MusicPlayerId(1));
    assert_eq!((beep.priority(), beep.reverb()), (5, Some(50)));
    assert_eq!(beep.tracks().len(), 1);
}

#[test]
fn a_playable_song_without_a_voicegroup_flag_cannot_take_the_default() {
    let upstream = FakeUpstream::new(
        "playable-default",
        TABLE,
        "se_beep.mid: -E -R50\n",
        &both_midis(),
    );
    let error = upstream
        .extract(&[SongSource::silent_default_group("se_beep", 1)])
        .expect_err("a note needs a real voicegroup");
    assert!(
        matches!(
            &error,
            ExtractError::Midi(_, MidiError::DefaultVoiceGroupOnPlayableSong)
        ),
        "{error:?}"
    );
}

#[test]
fn an_ordinary_song_without_a_voicegroup_flag_is_unsupported() {
    let upstream = FakeUpstream::new("no-group", TABLE, "se_beep.mid: -E -R50\n", &both_midis());
    let error = upstream.extract(&[BEEP]).expect_err("-G is required");
    assert!(
        matches!(
            &error,
            ExtractError::MidiCfg(_, MidiError::CfgMissingVoiceGroup)
        ),
        "{error:?}"
    );
}

#[test]
fn a_duplicate_cfg_entry_fails_with_a_typed_error() {
    let cfg = format!("{CFG}se_beep.mid: -E -R50 -G_rs_sfx_2\n");
    let upstream = FakeUpstream::new("dup-cfg", TABLE, &cfg, &both_midis());
    let error = upstream.extract(&[BEEP]).expect_err("ambiguous flags");
    assert!(
        matches!(&error, ExtractError::MidiCfg(_, MidiError::CfgEntryDuplicate(name)) if name == "se_beep.mid"),
        "{error:?}"
    );
}

#[test]
fn a_missing_cfg_entry_fails_with_a_typed_error() {
    let upstream = FakeUpstream::new("no-cfg", TABLE, "mus_dummy.mid: -E\n", &both_midis());
    let error = upstream.extract(&[BEEP]).expect_err("no flags");
    assert!(
        matches!(&error, ExtractError::MidiCfg(_, MidiError::CfgEntryMissing(name)) if name == "se_beep.mid"),
        "{error:?}"
    );
}

#[test]
fn a_missing_duplicate_malformed_or_disagreeing_table_row_fails_with_a_typed_error() {
    type IsExpectedError = fn(&MidiError) -> bool;
    let cases: [(&str, &str, IsExpectedError); 4] = [
        (
            "missing",
            "song mus_dummy, 0, 0\n",
            |e| matches!(e, MidiError::SongTableEntryMissing(s) if s == "se_beep"),
        ),
        (
            "duplicate",
            "song se_beep, 1, 1\nsong se_beep, 1, 1\n",
            |e| matches!(e, MidiError::SongTableEntryDuplicate(s) if s == "se_beep"),
        ),
        (
            "malformed",
            "song se_beep, 1\n",
            |e| matches!(e, MidiError::SongTableRowMalformed(s) if s == "se_beep"),
        ),
        ("disagreeing", "song se_beep, 2, 2\n", |e| {
            matches!(
                e,
                MidiError::SongTablePlayerMismatch {
                    expected: 1,
                    actual: 2,
                    ..
                }
            )
        }),
    ];
    for (name, table, expected) in cases {
        let upstream = FakeUpstream::new(name, table, CFG, &both_midis());
        let error = upstream.extract(&[BEEP]).expect_err(name);
        assert!(
            matches!(&error, ExtractError::Midi(path, e) if path.ends_with("song_table.inc") && expected(e)),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn a_missing_midi_source_fails_instead_of_being_skipped() {
    let upstream = FakeUpstream::new("no-midi", TABLE, CFG, &[("mus_dummy.mid", &SILENT_MIDI)]);
    let error = upstream
        .extract(&[SILENCE, BEEP])
        .expect_err("no se_beep.mid");
    assert!(
        matches!(&error, ExtractError::ReadFailed(path, _) if path.ends_with("se_beep.mid")),
        "{error:?}"
    );
}

#[test]
fn an_unsupported_clock_rate_fails_closed() {
    let upstream = FakeUpstream::new(
        "double-clock",
        TABLE,
        "se_beep.mid: -E -X -G_rs_sfx_1\n",
        &both_midis(),
    );
    let error = upstream.extract(&[BEEP]).expect_err("-X is unsupported");
    assert!(
        matches!(
            &error,
            ExtractError::Midi(_, MidiError::UnsupportedClocksPerBeat(2))
        ),
        "{error:?}"
    );
}

#[test]
fn a_duplicate_manifest_entry_fails_before_any_song_is_written() {
    let upstream = FakeUpstream::new("dup-manifest", TABLE, CFG, &both_midis());
    let error = upstream.extract(&[BEEP, BEEP]).expect_err("duplicate");
    assert!(
        matches!(&error, ExtractError::Midi(_, MidiError::ManifestDuplicateSong(s)) if s == "se_beep"),
        "{error:?}"
    );
}

/// `NAME` -> value of the `#define NAME <number>` lines of `include/constants/songs.h`.
fn song_constants(upstream: &Path) -> BTreeMap<String, u32> {
    read_text(&upstream.join("include/constants/songs.h"))
        .expect("songs.h")
        .lines()
        .filter_map(|line| {
            let mut words = line.strip_prefix("#define ")?.split_whitespace();
            let name = words.next()?;
            let value = words.next()?.parse().ok()?;
            Some((name.to_owned(), value))
        })
        .collect()
}

/// The song-table labels in row order, with their player operands resolved through `.equiv`.
fn song_table_rows(upstream: &Path) -> Vec<(String, u16)> {
    let text = read_text(&upstream.join("sound/song_table.inc")).expect("song_table.inc");
    let mut equivs = BTreeMap::new();
    let mut rows = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix(".equiv ") {
            let (name, value) = rest.split_once(',').expect("equiv operands");
            equivs.insert(
                name.trim().to_owned(),
                value.trim().parse().expect("equiv value"),
            );
        } else if let Some(rest) = line.strip_prefix("song ") {
            let mut fields = rest.split(',').map(str::trim);
            let symbol = fields.next().expect("label").to_owned();
            let player = fields.next().expect("player");
            rows.push((symbol, equivs[player]));
        }
    }
    rows
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn every_audited_call_site_and_table_row_matches_the_pinned_checkout() {
    assert!(upstream_present(), "run ./init.sh first");
    let upstream = repo_root().join("pokeemerald");
    let constants = song_constants(&upstream);
    let rows = song_table_rows(&upstream);

    for (constant, sites) in AUDITED_CALL_SITES {
        for (file, line) in sites {
            let text = read_text(&upstream.join(file)).expect("audited file");
            let source_line = text.lines().nth(*line as usize - 1).unwrap_or("");
            assert!(
                source_line
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .any(|word| word == constant),
                "{file}:{line} does not name {constant}: {source_line:?}"
            );
        }
    }
    for source in CHECKPOINT_SONGS {
        let constant = source.symbol.to_ascii_uppercase();
        let index = usize::try_from(constants[&constant]).expect("index");
        assert_eq!(
            rows.get(index)
                .map(|(symbol, player)| (symbol.as_str(), *player)),
            Some((source.symbol, source.player)),
            "{constant} = {index} indexes the wrong song-table row"
        );
    }
}

/// Songs a checkpoint map script names that the checkpoint cannot reach, and why.
const UNREACHABLE_MAP_SCRIPT_SONGS: [(&str, &str); 1] = [(
    "MUS_REGISTER_MATCH_CALL",
    "guarded by FLAG_HAS_MATCH_CALL, which the Pokenav-less checkpoint never sets",
)];

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn every_song_the_checkpoint_maps_name_is_listed_or_excluded() {
    assert!(upstream_present(), "run ./init.sh first");
    let upstream = repo_root().join("pokeemerald");
    let listed: BTreeSet<String> = CHECKPOINT_SONGS
        .iter()
        .map(|source| source.symbol.to_ascii_uppercase())
        .collect();
    let excluded: BTreeSet<&str> = UNREACHABLE_MAP_SCRIPT_SONGS
        .iter()
        .map(|(c, _)| *c)
        .collect();

    let mut named = BTreeSet::new();
    for dir in CHECKPOINT_MAPS {
        for file in ["map.json", "scripts.inc"] {
            let text = read_text(&upstream.join(format!("data/maps/{dir}/{file}"))).expect("map");
            named.extend(
                text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .filter(|word| word.starts_with("MUS_") || word.starts_with("SE_"))
                    .map(str::to_owned),
            );
        }
    }
    assert!(
        named.contains("MUS_BIRCH_LAB"),
        "the scan must read the maps"
    );
    for constant in named {
        // `PlayBGM` turns `MUS_NONE` into song 0 (`src/sound.c:563-569`).
        let song = if constant == "MUS_NONE" {
            "MUS_DUMMY"
        } else {
            &constant
        };
        assert!(
            listed.contains(song) || excluded.contains(song),
            "{constant} is named by a checkpoint map but is neither listed nor excluded"
        );
    }
}

/// The voicegroup, priority, and reverb `midi.cfg` gives `filename`, read without the extractor.
fn cfg_flags(cfg: &str, filename: &str) -> (Option<String>, u8, Option<u8>) {
    let prefix = format!("{filename}:");
    let line = cfg
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .expect("midi.cfg entry");
    let flag = |letter: char| {
        line.split_whitespace()
            .find_map(|token| token.strip_prefix('-')?.strip_prefix(letter))
    };
    (
        flag('G').map(|label| label.trim_start_matches('_').to_owned()),
        flag('P').map_or(0, |value| value.parse().expect("priority")),
        flag('R').map(|value| value.parse().expect("reverb")),
    )
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn every_manifest_song_reaches_the_pack_with_the_metadata_its_source_gives() {
    assert!(upstream_present(), "run ./init.sh first");
    let upstream = repo_root().join("pokeemerald");
    let mut writer = PackWriter::new();
    extract_songs(&upstream, &mut writer).expect("every manifest song compiles");
    let scratch = std::env::temp_dir().join(format!(
        "pokeemerald-rs-song-manifest-pack-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    let pack_path = scratch.join("songs.bin");
    std::fs::write(&pack_path, writer.finish().expect("packing")).expect("pack file");
    let pack = AssetPack::load(&pack_path).expect("pack loads");
    std::fs::remove_dir_all(&scratch).ok();

    let song_ids: BTreeSet<String> = pack
        .entries()
        .map(|entry| entry.id.clone())
        .filter(|id| id.starts_with("audio/song/"))
        .collect();
    let expected_ids: BTreeSet<String> = CHECKPOINT_SONGS.iter().map(SongSource::pack_id).collect();
    assert_eq!(song_ids, expected_ids);

    let cfg = read_text(&upstream.join("sound/songs/midi/midi.cfg")).expect("midi.cfg");
    let rows: BTreeMap<String, u16> = song_table_rows(&upstream).into_iter().collect();
    for source in CHECKPOINT_SONGS {
        let song: Song = pack.song(source.symbol).expect("song decodes");
        let (group, priority, reverb) = cfg_flags(&cfg, &source.midi_filename());
        let group = group.unwrap_or_else(|| "dummy".to_owned());
        assert_eq!(
            song.voicegroup().0,
            format!("audio/voicegroup/{group}"),
            "{}",
            source.symbol
        );
        assert_eq!(
            song.player(),
            MusicPlayerId(rows[source.symbol]),
            "{}",
            source.symbol
        );
        assert_eq!(song.priority(), priority, "{}", source.symbol);
        assert_eq!(song.reverb(), reverb, "{}", source.symbol);
    }

    let track_counts: [(&str, usize); 11] = [
        ("mus_title", 10),
        ("mus_vs_wild", 8),
        ("mus_vs_rival", 8),
        ("mus_heal", 4),
        ("mus_rg_caught_intro", 8),
        ("mus_intro", 10),
        ("mus_intro_battle", 9),
        ("mus_route122", 8),
        ("se_select", 1),
        ("se_pc_on", 1),
        ("se_dex_search", 2),
    ];
    for (symbol, count) in track_counts {
        let song = pack.song(symbol).expect("song decodes");
        assert_eq!(song.tracks().len(), count, "{symbol}");
    }
    assert!(pack.song("mus_dummy").expect("silence").tracks().is_empty());
}
