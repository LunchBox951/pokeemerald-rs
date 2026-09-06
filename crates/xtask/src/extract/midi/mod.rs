//! Compiles `mus_title.mid` into the asset pack's backend-neutral song
//! schema ([`event::SongEvent`]).
//!
//! # Scope: `mus_title` only, at its own compile flags
//!
//! This is a *title-scoped* compiler, not a general `tools/mid2agb`
//! reimplementation: it reads `sound/songs/midi/midi.cfg`'s `mus_title.mid`
//! line generically ([`mod@cfg`]), but [`compile`] hard-requires the two
//! flags that line always carries and fails closed
//! ([`error::MidiError::NonExactGateTime`]/
//! [`error::MidiError::UnsupportedClocksPerBeat`]) if a future caller ever
//! points it at a `midi.cfg` entry that doesn't — see [`compile`]'s module
//! docs for why each flag is required. `MEMACC` controllers
//! ([`error::MidiError::UnsupportedMemAccController`]) fail closed in
//! [`compile`] for every song; `mus_title.mid` carries none, and the
//! schema's one `MEMACC` user, `mus_vs_trainer`, is a different song this
//! compiler never touches.
//!
//! Every other command family `mus_title.mid` uses — notes, ties, tempo,
//! program change, pan/volume/modulation/LFO/bend/tune/priority
//! controllers, and the pseudo-echo `XCMD` pair — is modelled. Loop markers
//! are modelled too, unreached by `mus_title.mid` itself but pinned on
//! crafted fragments in [`compile`]'s own tests.
//!
//! # Pipeline
//!
//! [`reader`] frames `MThd`/`MTrk` chunks; [`parse`] turns one chunk's bytes
//! into a flat, time-ordered event list; [`compile`] is the semantic
//! translation (tick/velocity scaling, note-off pairing, tie-splitting,
//! sort order, controller mapping) into [`event::SongEvent`]; [`encode`]
//! serializes that to the schema's exact wire bytes, duplicated rather than
//! shared — see [`encode`]'s module docs, mirroring
//! `xtask::extract::voicegroups::encode`'s documented rationale. [`mod@cfg`]
//! resolves `midi.cfg`'s per-song compile flags this pipeline needs
//! (voicegroup label, priority, reverb, master volume, `-E`/`-X`)
//! generically, the same way `xtask::extract::layouts_json` resolves
//! `layouts.json` entries.
//!
//! # Output
//!
//! Reads `sound/songs/midi/midi.cfg` and [`SONG_MIDI_FILENAME`] from the
//! upstream checkout and writes the compiled result as one
//! [`PackKind::Raw`] entry under [`SONG_PACK_ID`] (`crate::extract`'s
//! "Asset id scheme" docs).

mod cfg;
mod compile;
mod encode;
mod error;
mod event;
mod parse;
mod reader;
mod translate;
mod velocity;

use std::path::Path;

pub(crate) use error::MidiError;

use super::pack::{PackEntry, PackKind, PackWriter};
use super::{read_file, read_text, ExtractError};

/// The upstream `.mid` source this slice compiles, and the pack id its
/// compiled song is written under (module docs, "Output").
const SONG_MIDI_FILENAME: &str = "mus_title.mid";
const SONG_PACK_ID: &str = "audio/song/mus_title";

/// Compile [`SONG_MIDI_FILENAME`] into its normalized [`event::SongEvent`]
/// streams (per [`compile::compile`]'s own semantics) and write it as a
/// [`PackKind::Raw`] entry under [`SONG_PACK_ID`].
///
/// # Errors
///
/// [`ExtractError::ReadFailed`] if `midi.cfg` or the `.mid` source is
/// missing; [`ExtractError::MidiCfg`] if `midi.cfg` has no entry for
/// [`SONG_MIDI_FILENAME`] or that entry is malformed; [`ExtractError::Midi`]
/// if compiling the `.mid` source itself fails, or if the compiled song
/// carries more tracks than the wire format's `u8` count can describe (see
/// [`error::MidiError`]'s variants).
pub(super) fn extract_song(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let cfg_path = upstream.join("sound/songs/midi/midi.cfg");
    let cfg_text = read_text(&cfg_path)?;
    let entry = cfg::parse_entry_for(&cfg_text, SONG_MIDI_FILENAME)
        .map_err(|e| ExtractError::MidiCfg(cfg_path.clone(), e))?;

    let midi_path = upstream.join("sound/songs/midi").join(SONG_MIDI_FILENAME);
    let midi_bytes = read_file(&midi_path)?;
    let song = compile::compile(&midi_bytes, &entry)
        .map_err(|e| ExtractError::Midi(midi_path.clone(), e))?;
    let payload =
        encode::encode_song(&song).map_err(|e| ExtractError::Midi(midi_path.clone(), e))?;

    writer.push(PackEntry {
        id: SONG_PACK_ID.to_owned(),
        kind: PackKind::Raw,
        payload,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::event::SongEvent;
    use super::{extract_song, SONG_PACK_ID};
    use crate::extract::pack::PackWriter;

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn mus_title_compiles_and_reaches_the_pack() {
        use super::super::{repo_root, upstream_present};
        assert!(upstream_present(), "run ./init.sh first");
        let upstream = repo_root().join("pokeemerald");
        let mut writer = PackWriter::new();
        extract_song(&upstream, &mut writer).expect("mus_title.mid should compile");
        let bytes = writer.finish().expect("packing should succeed");
        assert!(
            bytes
                .windows(SONG_PACK_ID.len())
                .any(|window| window == SONG_PACK_ID.as_bytes()),
            "missing pack entry id `{SONG_PACK_ID}`"
        );
    }

    /// Pins values hand-verified against a locally built `tools/mid2agb`
    /// oracle: compiling `mus_title.mid` at its real `midi.cfg` flags
    /// produces 10 tracks (one per real MIDI channel; the conductor `MTrk`
    /// chunk itself carries no notes), priority `0`, reverb `50`, track 0's
    /// first six events, and its final five events ending in `Fine`,
    /// matching the oracle's own `mus_title_1` assembly block in order (not
    /// encoded wire bytes — see [`super::compile`]'s module docs for why
    /// this compiler's `Wait` placement can still diverge from the oracle's
    /// own `Wnn` opcodes while representing the identical delay).
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

        // mus_title.mid has no loop markers: zero `GOTO` events in the
        // oracle's compiled output.
        let goto_count = song
            .tracks
            .iter()
            .flatten()
            .filter(|e| matches!(e, SongEvent::Goto(_)))
            .count();
        assert_eq!(goto_count, 0);

        // Three `XCMD xIECV`/`xIECL` pairs, pseudo-echo volumes 10, 10, 16
        // and length 12 each time: the oracle's `mus_title_7`/`_8`/`_10`
        // blocks (its 1-based `g_agbTrack` labels), this compiler's 0-based
        // `song.tracks[6]`/`[7]`/`[9]`.
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
