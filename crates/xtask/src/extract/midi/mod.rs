//! Compiles the checkpoint song manifest's MIDI sources into the normalized compiler event
//! model ([`event::SongEvent`]) that [`encode`] writes as the asset pack's backend-neutral song
//! schema.
//!
//! # Scope: the checkpoint manifest, at each song's own compile flags
//!
//! This is not a general `tools/mid2agb` reimplementation. [`manifest::CHECKPOINT_SONGS`] lists
//! the songs the early-game checkpoint reaches. For each, [`mod@cfg`] reads the song's
//! `sound/songs/midi/midi.cfg` line (the flags `audio_rules.mk`'s `MID_RULE` passes to
//! `mid2agb`) and [`song_table`] reads its `Song.ms` music player from
//! `sound/song_table.inc`. [`compile`] hard-requires `-E` (exact gate time) and rejects `-X`
//! (48-clocks-per-beat), and fails closed ([`error::MidiError::NonExactGateTime`] and
//! [`error::MidiError::UnsupportedClocksPerBeat`]) on a manifest song whose line does neither
//! constraint's way. See [`compile`]'s module docs for why each flag is required. A `MEMACC`
//! controller fails closed ([`error::MidiError::UnsupportedMemAccController`]) only on a
//! playable channel; [`compile`] drops unplayable channels before reading their controllers at
//! all. No manifest song carries one.
//!
//! A manifest song with no `-G` flag (`mus_dummy`, upstream's silence) takes upstream's `dummy`
//! voicegroup, and only if it compiles to no tracks
//! ([`error::MidiError::DefaultVoiceGroupOnPlayableSong`]).
//!
//! Voicegroup and sample resolution are not done here: a song only names its voicegroup.
//!
//! # Pipeline
//!
//! [`reader`] frames `MThd`/`MTrk` chunks; [`parse`] turns one chunk's bytes
//! into a flat, time-ordered event list; [`compile`] is the semantic
//! translation (tick/velocity scaling, note-off pairing, tie-splitting,
//! sort order, controller mapping) into [`event::SongEvent`]; [`encode`]
//! maps that onto `assets`' song schema, which canonicalizes the waits and
//! writes the wire bytes. [`mod@cfg`]
//! resolves `midi.cfg`'s per-song compile flags this pipeline needs
//! (voicegroup label, priority, reverb, master volume, `-E`/`-X`).
//!
//! # Output
//!
//! Writes each compiled song as one [`pack_format::EntryKind::Raw`] entry under
//! `audio/song/<symbol>` (`crate::extract`'s "Asset id scheme" docs).

mod cfg;
mod compile;
mod encode;
mod error;
mod event;
mod manifest;
mod parse;
mod reader;
mod song_table;
mod translate;
mod velocity;

use std::path::{Path, PathBuf};

pub(crate) use error::MidiError;

use super::{read_file, read_text, ExtractError};
use manifest::{SongSource, VoiceGroupSource, CHECKPOINT_SONGS};
use pack_format::PackWriter;

/// The pack id of the one song the ROM profile locates (`mus_title`), which the manifest also
/// carries. Crate-visible: `gen_rom_profile::audio` reuses this exact id as the one
/// `audio/song/` entry it locates a ROM address for, the same reuse [`super::voicegroups`]'s
/// `groups_by_label` documents for the voicegroup index.
pub(crate) const SONG_PACK_ID: &str = "audio/song/mus_title";

/// Upstream's voicegroup for a `midi.cfg` entry with no `-G` flag: `g_voiceGroup` starts as
/// `_dummy` (`tools/mid2agb/main.cpp:36`), and the leading underscore is the assembler symbol's.
const DEFAULT_VOICEGROUP_LABEL: &str = "dummy";

/// The checkout's `midi.cfg` and `song_table.inc`, read once for every manifest song.
struct SongSources {
    cfg_path: PathBuf,
    cfg_text: String,
    table_path: PathBuf,
    table_text: String,
    midi_dir: PathBuf,
}

impl SongSources {
    fn read(upstream: &Path) -> Result<Self, ExtractError> {
        let cfg_path = upstream.join("sound/songs/midi/midi.cfg");
        let table_path = upstream.join("sound/song_table.inc");
        Ok(Self {
            cfg_text: read_text(&cfg_path)?,
            table_text: read_text(&table_path)?,
            cfg_path,
            table_path,
            midi_dir: upstream.join("sound/songs/midi"),
        })
    }

    /// Encodes `source` from its `.mid`, `midi.cfg` line, and song-table row.
    fn encode(&self, source: &SongSource) -> Result<Vec<u8>, ExtractError> {
        let in_table = |error| ExtractError::Midi(self.table_path.clone(), error);
        let player = song_table::player_for(&self.table_text, source.symbol).map_err(in_table)?;
        if player != source.player {
            return Err(in_table(MidiError::SongTablePlayerMismatch {
                symbol: source.symbol.to_owned(),
                expected: source.player,
                actual: player,
            }));
        }

        let in_cfg = |error| ExtractError::MidiCfg(self.cfg_path.clone(), error);
        let filename = source.midi_filename();
        let (mut entry, defaulted_group) = match source.voicegroup {
            VoiceGroupSource::Configured => (
                cfg::parse_entry_for(&self.cfg_text, &filename).map_err(in_cfg)?,
                false,
            ),
            VoiceGroupSource::DefaultWhenSilent => {
                let entry = cfg::parse_entry_flags(&self.cfg_text, &filename).map_err(in_cfg)?;
                let defaulted = entry.voicegroup_label.is_empty();
                (entry, defaulted)
            }
        };
        if defaulted_group {
            DEFAULT_VOICEGROUP_LABEL.clone_into(&mut entry.voicegroup_label);
        }

        let midi_path = self.midi_dir.join(&filename);
        let in_midi = |error| ExtractError::Midi(midi_path.clone(), error);
        let song = compile::compile(&read_file(&midi_path)?, &entry).map_err(in_midi)?;
        if defaulted_group && !song.tracks.is_empty() {
            return Err(in_midi(MidiError::DefaultVoiceGroupOnPlayableSong));
        }
        encode::encode_song(&song, player).map_err(|source| ExtractError::InvalidSongSchema {
            path: midi_path.clone(),
            source,
        })
    }
}

/// Compile every [`CHECKPOINT_SONGS`] source into its normalized [`event::SongEvent`] streams
/// (per [`compile::compile`]'s own semantics) and write each as a
/// [`pack_format::EntryKind::Raw`] entry under `audio/song/<symbol>`.
///
/// # Errors
///
/// [`ExtractError::ReadFailed`] if `midi.cfg`, `song_table.inc`, or a `.mid` source is
/// missing; [`ExtractError::MidiCfg`] if a song's `midi.cfg` entry is absent, duplicated, or
/// malformed; [`ExtractError::Midi`] if the manifest is malformed, a song's song-table row is
/// absent, duplicated, malformed, or assigns a different music player than the manifest
/// expects, or compiling the `.mid` source itself fails (see [`error::MidiError`]'s variants);
/// [`ExtractError::InvalidSongSchema`] if `assets` rejects a compiled song as outside the pack
/// schema's domain.
pub(super) fn extract_songs(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    extract_listed_songs(upstream, writer, &CHECKPOINT_SONGS)
}

fn extract_listed_songs(
    upstream: &Path,
    writer: &mut PackWriter,
    sources: &[SongSource],
) -> Result<(), ExtractError> {
    let files = SongSources::read(upstream)?;
    manifest::check_manifest(sources)
        .map_err(|error| ExtractError::Midi(files.table_path.clone(), error))?;
    for source in sources {
        let payload = files.encode(source)?;
        writer.push(pack_format::raw_entry(source.pack_id(), payload));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::event::SongEvent;
    use super::{extract_songs, SONG_PACK_ID};
    use pack_format::PackWriter;

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn mus_title_reaches_the_pack_beside_the_manifest() {
        use super::super::{repo_root, upstream_present};
        assert!(upstream_present(), "run ./init.sh first");
        let upstream = repo_root().join("pokeemerald");
        let mut writer = PackWriter::new();
        extract_songs(&upstream, &mut writer).expect("the manifest songs should compile");
        let bytes = writer.finish().expect("packing should succeed");
        assert!(
            bytes
                .windows(SONG_PACK_ID.len())
                .any(|window| window == SONG_PACK_ID.as_bytes()),
            "missing pack entry id `{SONG_PACK_ID}`"
        );
    }

    /// Pins values hand-verified against a locally built `tools/mid2agb`
    /// oracle's assembly listing, not its encoded wire bytes: the slices and
    /// aggregates asserted below, not the full event stream. See
    /// [`super::compile`]'s module docs for why `Wait` placement can still
    /// diverge from the oracle's own `Wnn` opcodes while representing the
    /// identical delay.
    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn mus_title_compiles_to_hand_verified_values() {
        use super::super::{read_text, repo_root, upstream_present};
        assert!(upstream_present(), "run ./init.sh first");
        let upstream = repo_root().join("pokeemerald");
        let cfg_text = read_text(&upstream.join("sound/songs/midi/midi.cfg"))
            .expect("midi.cfg should be readable");
        let entry = super::cfg::parse_entry_for(&cfg_text, "mus_title.mid")
            .expect("mus_title.mid's midi.cfg entry should parse");
        let midi_bytes = std::fs::read(upstream.join("sound/songs/midi/mus_title.mid"))
            .expect("mus_title.mid should be readable");
        let song =
            super::compile::compile(&midi_bytes, &entry).expect("mus_title.mid should compile");

        assert_eq!(song.tracks.len(), 10);
        assert_eq!(song.priority, 0);
        assert_eq!(song.reverb, Some(50));
        assert_eq!(song.voicegroup_label, "title");

        let track0 = &song.tracks[0];
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

        // mus_title.mid has no loop markers.
        let goto_count = song
            .tracks
            .iter()
            .flatten()
            .filter(|e| matches!(e, SongEvent::Goto(_)))
            .count();
        assert_eq!(goto_count, 0);

        // Three `XCMD xIECV`/`xIECL` pairs, matching the oracle's
        // `mus_title_7`/`_8`/`_10` blocks (its 1-based `g_agbTrack` labels;
        // this compiler's 0-based `tracks[6]`/`[7]`/`[9]`).
        let volumes: Vec<u8> = song
            .tracks
            .iter()
            .flatten()
            .filter_map(|e| match e {
                SongEvent::PseudoEchoVolume(v) => Some(*v),
                _ => None,
            })
            .collect();
        assert_eq!(volumes, vec![10, 10, 16]);
        let lengths: Vec<u8> = song
            .tracks
            .iter()
            .flatten()
            .filter_map(|e| match e {
                SongEvent::PseudoEchoLength(v) => Some(*v),
                _ => None,
            })
            .collect();
        assert_eq!(lengths, vec![12, 12, 12]);
    }
}
