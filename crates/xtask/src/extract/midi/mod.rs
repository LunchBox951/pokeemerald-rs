//! Compiles `mus_title.mid`'s MIDI semantics into the backend-neutral song
//! schema (S-4, issue #181, `#115` child 2): the Policy-A dev backend's
//! answer to `crates/assets::audio::song`'s "`#115` child 2 (the MIDI
//! compiler) is what actually produces this from a `.mid` file" (that
//! module's own docs).
//!
//! # Scope: one song, `mus_title`, at its own compile flags
//!
//! This is a *title-scoped* compiler, not a general `tools/mid2agb`
//! reimplementation: it reads `sound/songs/midi/midi.cfg`'s
//! `mus_title.mid` line generically ([`cfg`]), but [`compile`] hard-requires
//! the two flags that line always carries (`-E` exact gate time, default
//! 24-clocks-per-beat — never `-X`) and fails closed
//! ([`error::MidiError::NonExactGateTime`]/
//! [`error::MidiError::UnsupportedClocksPerBeat`]) if a future caller ever
//! points it at a `midi.cfg` entry that doesn't. See [`compile`]'s module
//! docs for the full list of what upstream interprets that this compiler
//! deliberately does not reproduce, and why each cut is safe:
//!
//! - Note-duration LUT quantization (only reachable without `-E`).
//! - `-X`/48-clocks-per-beat (this schema has no field to record which
//!   convention a song's ticks use, so supporting it here would be silently
//!   wrong for a consumer — a schema gap, not a compiler gap).
//! - `tools/mid2agb`'s `WAIT`-opcode-legal-value bucketing and whole-note
//!   pattern-compression bookkeeping (`SplitTime`/`WholeNoteMark`/`PATT`/
//!   `PEND` — on-disk artifacts with zero audible effect, already out of
//!   scope for [`crates::assets::audio::song::SongEvent`] itself).
//! - Operand elision (`EOT`'s omitted key, compressed note/velocity
//!   mnemonics) — this compiler's stream is always fully explicit.
//! - `MEMACC` (controllers `0x0C`/`0x0D`/`0x0E`/`0x0F`/`0x10`/`0x11`) —
//!   `mus_title.mid` carries none (confirmed against a locally built
//!   `tools/mid2agb` oracle), and the schema's one canonical `MEMACC` user
//!   (`mus_vs_trainer`) is a different song, out of scope here.
//!
//! Every other command family `mus_title.mid` actually carries — notes,
//! ties, velocity quantization, tempo, program change, pan/volume/
//! modulation/LFO/bend/tune/priority controllers, the pseudo-echo `XCMD`
//! pair, and loop markers (the last unexercised by `mus_title` itself but
//! pinned on crafted fragments — [`compile`]'s tests) — is modelled.
//!
//! # Pipeline
//!
//! [`reader`] frames `MThd`/`MTrk` chunks; [`parse`] turns one chunk's bytes
//! into a flat, time-ordered event list; [`compile`] is the semantic
//! translation (tick/velocity scaling, note-off pairing, tie-splitting,
//! sort order, controller mapping) into [`event::SongEvent`], this crate's
//! own copy of `crates/assets::audio::song::SongEvent`'s shape; [`encode`]
//! serializes that to the schema's exact wire bytes (duplicated, not
//! shared — see [`encode`]'s module docs, mirroring
//! `xtask::extract::voicegroups::encode`'s documented rationale).
//! [`cfg`] resolves `midi.cfg`'s per-song compile flags this pipeline needs
//! (voicegroup label, priority, reverb, master volume, `-E`/`-X`)
//! generically, the same way `xtask::extract::layouts_json` resolves
//! `layouts.json` entries.
//!
//! # Asset id: `audio/song/mus_title`
//!
//! Mirrors `audio/voicegroup/<label>`/`audio/sample/*`'s convention
//! (`crate::extract`'s "Asset id scheme" docs): `<name>` is the upstream
//! `.mid` file's stem, matching how `xtask::extract::voicegroups` names
//! entries by the upstream `voice_group` symbol rather than a path. There is
//! exactly one entry this slice writes; a later slice compiling more songs
//! would extend this same one-id-per-`.mid`-stem scheme.

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

use super::{read_file, read_text, ExtractError};
use pack_format::PackWriter;

/// The upstream `.mid` source this slice compiles, and the pack id its
/// compiled song is written under (module docs, "Asset id").
const SONG_MIDI_FILENAME: &str = "mus_title.mid";
const SONG_PACK_ID: &str = "audio/song/mus_title";

/// Compile [`SONG_MIDI_FILENAME`] into its normalized [`event::SongEvent`]
/// streams (per [`compile::compile`]'s own semantics) and write it as a
/// [`EntryKind::Raw`] entry under [`SONG_PACK_ID`].
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

    writer.push(pack_format::raw_entry(SONG_PACK_ID.to_owned(), payload));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::event::SongEvent;
    use super::{extract_song, SONG_PACK_ID};
    use pack_format::PackWriter;

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

    /// Hand-verified against a locally built `tools/mid2agb` oracle (this
    /// module's docs): compiling `mus_title.mid` at its real `midi.cfg`
    /// flags produces exactly 10 tracks (one per real MIDI channel 0..=9;
    /// the conductor `MTrk` chunk itself carries no notes), priority `0`,
    /// reverb override `50`, and track 0's first six events are
    /// `KeyShift(0)` (always first), then `mid2agb`'s own printed tempo
    /// literal `144` BPM (the merged sequence track's tempo, which only
    /// ever reaches the first compiled track — module docs), `VOICE 14`,
    /// `PAN c_v+40`, `LFOS 44`, and `VOL 122*90/127` (truncating integer
    /// division = `86`) — matching the oracle's own
    /// `mus_title_1:`/`@ 000` block byte for byte (module order, not wire
    /// bytes — see `super::compile`'s "No operand elision"/"`Wait` is a
    /// free tick count" docs for why this compiler's `Wait` placement can
    /// legitimately diverge from the oracle's own `W24` while representing
    /// the identical 24-tick delay). Track 0's last two real commands
    /// (`VOL 13*90/127` = `9`, then `VOL 12*90/127` = `8`, the oracle's
    /// final two `VOL` bytes) are pinned as well, including the trailing
    /// `Wait(4)` before `FINE` that this compiler computes from the merged
    /// sequence track's own `EndOfTrack` tick (module docs on
    /// `final_boundary`) exactly as the oracle's own trailing `W04` does.
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

        // No loop markers anywhere in mus_title.mid (confirmed against the
        // oracle: zero `GOTO` bytes in the whole compiled file).
        let goto_count = song
            .tracks
            .iter()
            .flatten()
            .filter(|e| matches!(e, SongEvent::Goto(_)))
            .count();
        assert_eq!(goto_count, 0);

        // Three `XCMD xIECV`/`xIECL` pairs total, pseudo-echo volumes 10,
        // 10, 16 and length 12 each time. Confirmed against the oracle:
        // its `mus_title_7`/`mus_title_8`/`mus_title_10` blocks each carry
        // exactly one. Those are upstream's own 1-based `g_agbTrack`
        // labels, i.e. this compiler's `song.tracks[6]`/`[7]`/`[9]` -- an
        // earlier revision of this comment printed the 0-based indices
        // against the `mus_title_` label prefix, naming three blocks that
        // are not the ones carrying the pairs.
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
