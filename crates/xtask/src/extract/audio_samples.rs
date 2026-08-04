//! Extracts the `Sample` pack entries (S-4, issue #183, `#115` child 4) that
//! `mus_title`'s voicegroup needs: every `DirectSound` instrument sample and
//! CGB programmable-wave table it references, transitively through its
//! key-split sub-groups.
//!
//! # Which voicegroup, and how that was verified
//!
//! `mus_title`'s compiled voicegroup is resolved the same way upstream's
//! build resolves it, traced end to end in the real checkout rather than
//! assumed from the issue's original wording (which named a different
//! voicegroup file — this checkout's actual chain is below):
//!
//! 1. `sound/song_table.inc:422`: `song mus_title, MUSIC_PLAYER_BGM, 0`.
//! 2. `sound/songs/midi/midi.cfg:186`: `mus_title.mid: -E -R50 -G_title -V090`
//!    — the `-G_title` `mid2agb` option.
//! 3. `tools/mid2agb/main.cpp:148-154`: `-G<suffix>` sets `g_voiceGroup` to
//!    `<suffix>` verbatim, later emitted as `.equ  voice, voicegroup<suffix>`
//!    in the compiled song header — i.e. `mus_title` plays through the
//!    voicegroup named `title`.
//! 4. `sound/voice_groups.inc:66`: `.include "sound/voicegroups/title.inc"`.
//!
//! `sound/voicegroups/title.inc` itself has 5 `DirectSoundWaveData_*`
//! slots (`sc88pro_glockenspiel`, `sc88pro_tubular_bell`, `sc88pro_harp`,
//! `sc88pro_timpani`, `sc88pro_flute`) and 4 `ProgrammableWaveData_*`
//! slots directly, plus 6
//! `voice_keysplit`/`voice_keysplit_all` indirections to further voicegroups
//! (`rs_drumset`, and the `piano`/`strings`/`trumpet`/`tuba`/`french_horn`
//! keysplits under `sound/voicegroups/keysplits/`) that themselves resolve
//! to `DirectSoundWaveData_*` slots — no nested `voice_rhythm` indirection is
//! present in this tree. One more sample enters through the link-adjacency
//! overflow modeled for issue #201: `title`'s borrowed slot 102 (=
//! `voicegroup_intro` entry 13) references
//! `DirectSoundWaveData_sc88pro_xylophone`
//! (`sound/voicegroups/intro.inc:15`). Flattening all of it gives the
//! exact, hand-verified sets below ([`DIRECT_SOUND_SAMPLES`],
//! [`PROGRAMMABLE_WAVE_SAMPLES`]); each symbol's upstream `.wav`/`.pcm`
//! source is confirmed against `sound/direct_sound_data.inc` /
//! `sound/programmable_wave_data.inc`'s own `.incbin` lines. The lists are
//! hand-maintained while the sample pass runs *before* the voicegroup
//! pass; the real-pack closure test
//! (`crates/pokeemerald-rs/src/voicegroup_pack_tests.rs`) fails the build
//! if a resolver change ever references a sample this list misses.
//!
//! # Asset id scheme
//!
//! - `audio/sample/direct-sound/<basename>` — `<basename>` is the upstream
//!   `.wav` file's stem (e.g. `audio/sample/direct-sound/sc88pro_flute`),
//!   matching the upstream `DirectSoundWaveData_<basename>` symbol name
//!   minus its prefix.
//! - `audio/sample/programmable-wave/<NN>` — `<NN>` is the upstream
//!   `sound/programmable_wave_samples/<NN>.pcm` file's two-digit stem
//!   (e.g. `audio/sample/programmable-wave/01`), matching
//!   `ProgrammableWaveData_<N>`'s `.incbin` target.
//!
//! Both live under a shared `audio/sample/` namespace (not a flat
//! `audio/sample/<basename>`) so a `DirectSound` basename can never collide
//! with a programmable-wave number, and so a reader can select one kind
//! without inspecting the payload — mirrors `tileset/<name>/palette/<NN>`
//! vs `tileset/<name>/tiles` living under one `tileset/<name>/` namespace.
//! Slice #182 (voicegroup resolver) references samples by these same ids.
//!
//! # Payload shape: the schema's own wire format, duplicated
//!
//! Every entry's payload is byte-for-byte what
//! `crates/assets::audio::sample::Sample::encode` would produce for the
//! equivalent [`Sample::DirectSound`](../../../assets/src/audio/sample.rs)/
//! `Sample::ProgrammableWave` value, stored as a plain
//! [`PackKind::Raw`] entry (this pipeline never depends on `crates/assets`,
//! and vice versa — see `crate::extract::pack`'s module docs — so
//! [`encode_direct_sound`]/[`encode_programmable_wave`] below are this
//! crate's own copy of that wire format, not a shared abstraction).
//!
//! Because they *are* a copy, this module's own tests can only pin this
//! side's understanding of the layout. The cross-crate half of the pin —
//! decoding the real extracted pack's `audio/sample/*` payloads back
//! through `Sample::decode` and asserting concrete field values — lives in
//! `crates/assets/src/pack/tests.rs`'s
//! `real_pack_audio_samples_decode_through_the_sample_schema`, which CI
//! runs (`cargo test -p assets -- --ignored`, after the extract step). That
//! is what would catch the two encoders drifting apart.
//!
//! # Field derivation: see [`super::wav`]
//!
//! [`super::wav::decode`] does the actual `base_frequency`/`loop_start`/PCM
//! derivation from each `.wav` source; see its module docs for the full
//! citation trail into `tools/wav2agb`. Programmable-wave tables need no
//! derivation at all: each `sound/programmable_wave_samples/*.pcm` file
//! *is* the raw 16-byte CGB wave table already (confirmed by inspecting the
//! real files: exactly 16 bytes each), so [`extract_audio_samples`] copies
//! it in directly.

use std::path::Path;

use super::pack::{PackEntry, PackKind, PackWriter};
use super::wav;
use super::{read_file, ExtractError};

/// The upstream `.wav` basenames (`sound/direct_sound_samples/<name>.wav`,
/// symbol `DirectSoundWaveData_<name>`) that `mus_title`'s voicegroup
/// references, directly or through a key-split sub-group — see the module
/// docs for how this list was traced. Kept sorted for readability (pack
/// output order is independent of this list's order — [`PackWriter::finish`]
/// sorts by id regardless).
const DIRECT_SOUND_SAMPLES: [&str; 33] = [
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

/// The upstream `sound/programmable_wave_samples/<NN>.pcm` numbers (symbol
/// `ProgrammableWaveData_<N>`) that `mus_title`'s voicegroup (`title.inc`)
/// references: slots for `ProgrammableWaveData_1`, `_2`, `_5`, `_6`.
const PROGRAMMABLE_WAVE_SAMPLES: [u32; 4] = [1, 2, 5, 6];

/// Every programmable-wave sample source is exactly 16 bytes: two 4-bit
/// samples packed per byte, 32 samples per CGB wave table.
const PROGRAMMABLE_WAVE_SIZE: usize = 16;

/// Mirrors `crates/assets::audio::sample::Sample`'s `KIND_DIRECT_SOUND` tag
/// byte. Duplicated, not imported — see the module docs.
const SAMPLE_KIND_DIRECT_SOUND: u8 = 0;
/// Mirrors `crates/assets::audio::sample::Sample`'s `KIND_PROGRAMMABLE_WAVE`
/// tag byte.
const SAMPLE_KIND_PROGRAMMABLE_WAVE: u8 = 1;

/// Encode a `DirectSound` sample to the schema's wire format: kind tag,
/// `base_frequency`, a loop-presence flag plus loop start (present but
/// meaningless when the flag is clear, exactly like the schema's own
/// encoder), a `u32` sample count, then the signed 8-bit PCM payload.
fn encode_direct_sound(base_frequency: u32, loop_start: Option<u32>, data: &[i8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 4 + 1 + 4 + 4 + data.len());
    out.push(SAMPLE_KIND_DIRECT_SOUND);
    out.extend_from_slice(&base_frequency.to_le_bytes());
    out.push(u8::from(loop_start.is_some()));
    out.extend_from_slice(&loop_start.unwrap_or(0).to_le_bytes());
    #[allow(clippy::cast_possible_truncation)]
    let len = data.len() as u32;
    out.extend_from_slice(&len.to_le_bytes());
    for &sample in data {
        out.push(sample.to_ne_bytes()[0]);
    }
    out
}

/// Encode a programmable-wave table to the schema's wire format: kind tag
/// then the 16 raw table bytes.
fn encode_programmable_wave(table: &[u8; PROGRAMMABLE_WAVE_SIZE]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + PROGRAMMABLE_WAVE_SIZE);
    out.push(SAMPLE_KIND_PROGRAMMABLE_WAVE);
    out.extend_from_slice(table);
    out
}

/// Extract every `DirectSound`/programmable-wave sample [`DIRECT_SOUND_SAMPLES`]/
/// [`PROGRAMMABLE_WAVE_SAMPLES`] name (see the module docs for how that set
/// was derived) as `audio/sample/*` [`PackKind::Raw`] entries.
///
/// # Errors
///
/// [`ExtractError::ReadFailed`] if a source file is missing;
/// [`ExtractError::Wav`] if a `.wav` source fails to decode (see
/// [`wav::WavError`]'s variants); [`ExtractError::ProgrammableWaveWrongSize`]
/// if a `.pcm` source is not exactly [`PROGRAMMABLE_WAVE_SIZE`] bytes.
pub(super) fn extract_audio_samples(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let direct_sound_dir = upstream.join("sound/direct_sound_samples");
    for name in DIRECT_SOUND_SAMPLES {
        let path = direct_sound_dir.join(format!("{name}.wav"));
        let bytes = read_file(&path)?;
        let sample = wav::decode(&bytes).map_err(|e| ExtractError::Wav(path.clone(), e))?;
        let payload = encode_direct_sound(sample.base_frequency, sample.loop_start, &sample.data);
        writer.push(PackEntry {
            id: format!("audio/sample/direct-sound/{name}"),
            kind: PackKind::Raw,
            payload,
        });
    }

    let programmable_wave_dir = upstream.join("sound/programmable_wave_samples");
    for n in PROGRAMMABLE_WAVE_SAMPLES {
        let path = programmable_wave_dir.join(format!("{n:02}.pcm"));
        let bytes = read_file(&path)?;
        let table: [u8; PROGRAMMABLE_WAVE_SIZE] =
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| ExtractError::ProgrammableWaveWrongSize {
                    path: path.clone(),
                    actual: bytes.len(),
                })?;
        writer.push(PackEntry {
            id: format!("audio/sample/programmable-wave/{n:02}"),
            kind: PackKind::Raw,
            payload: encode_programmable_wave(&table),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        encode_direct_sound, encode_programmable_wave, read_file, wav, DIRECT_SOUND_SAMPLES,
        PROGRAMMABLE_WAVE_SAMPLES, PROGRAMMABLE_WAVE_SIZE,
    };

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-{name}-{}.pack",
            std::process::id()
        ))
    }

    /// Every expected id reaches the pack, *and* at least one real payload
    /// is pinned at the byte level rather than only by its id: an id search
    /// alone would pass even if [`encode_direct_sound`] wrote garbage. The
    /// consumer half of that pin (the same concrete triple, read back out
    /// of the real pack through `crates/assets::audio::Sample::decode`)
    /// lives in `crates/assets/src/pack/tests.rs`'s
    /// `real_pack_audio_samples_decode_through_the_sample_schema`, which CI
    /// runs via `cargo test -p assets -- --ignored` -- this crate's own
    /// `--ignored` lane is not part of the CI gate.
    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn direct_sound_and_programmable_wave_samples_are_extracted() {
        // Same crude substring-search strategy as
        // `extract::tests::layout_grids_are_extracted` (no pack reader
        // lives in this crate -- see its comment).
        use super::super::{extract_to, repo_root, upstream_present};
        assert!(upstream_present(), "run ./init.sh first");
        let path = scratch_path("audio-samples");
        let report = extract_to(&path).expect("extraction should succeed against a real checkout");
        let bytes = std::fs::read(&report.output_path).unwrap();
        for name in DIRECT_SOUND_SAMPLES {
            let id = format!("audio/sample/direct-sound/{name}");
            assert!(
                bytes
                    .windows(id.len())
                    .any(|window| window == id.as_bytes()),
                "missing pack entry id `{id}`"
            );
        }
        for n in PROGRAMMABLE_WAVE_SAMPLES {
            let id = format!("audio/sample/programmable-wave/{n:02}");
            assert!(
                bytes
                    .windows(id.len())
                    .any(|window| window == id.as_bytes()),
                "missing pack entry id `{id}`"
            );
        }

        // Payload-level pin, on this crate's own representation. Concrete
        // values read off `sound/direct_sound_samples/sc88pro_flute.wav`'s
        // own chunks: `agbp` = 3425024 (so the pitch word is that override,
        // deliberately not `sample_rate * 1024` = 3424256), `smpl` loop
        // start = 1312, `agbl` = 1874 (one less than the naive loop end
        // 1875 = the inclusive `smpl` loop end 1874, plus one). See
        // `super::wav`'s module docs for why each of those is the derived
        // field.
        let flute_path =
            repo_root().join("pokeemerald/sound/direct_sound_samples/sc88pro_flute.wav");
        let flute =
            read_file(&flute_path).expect("the flute sample source exists in a real checkout");
        let sample = wav::decode(&flute).expect("the flute sample source should decode");
        assert_eq!(sample.base_frequency, 3_425_024);
        assert_eq!(sample.loop_start, Some(1312));
        assert_eq!(sample.data.len(), 1874);

        // ...and that exact payload is what actually reached the pack.
        let payload = encode_direct_sound(sample.base_frequency, sample.loop_start, &sample.data);
        assert!(
            bytes.windows(payload.len()).any(|window| window == payload),
            "the flute sample's encoded payload is not present in the pack"
        );

        let wave_path = repo_root().join("pokeemerald/sound/programmable_wave_samples/01.pcm");
        let table =
            read_file(&wave_path).expect("the programmable-wave 01 source exists in a checkout");
        assert_eq!(
            table,
            vec![
                0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9, 0x63, 0x10, 0x01, 0x25, 0x8a, 0xde, 0xfe, 0xc9,
                0x63, 0x10
            ]
        );
        let wave_payload = encode_programmable_wave(
            &<[u8; PROGRAMMABLE_WAVE_SIZE]>::try_from(table.as_slice()).unwrap(),
        );
        assert!(
            bytes
                .windows(wave_payload.len())
                .any(|window| window == wave_payload),
            "programmable-wave 01's encoded payload is not present in the pack"
        );

        let _ = std::fs::remove_file(report.output_path);
    }

    #[test]
    fn direct_sound_sample_list_has_no_duplicates() {
        let unique: std::collections::HashSet<_> = DIRECT_SOUND_SAMPLES.iter().collect();
        assert_eq!(unique.len(), DIRECT_SOUND_SAMPLES.len());
        assert!(
            DIRECT_SOUND_SAMPLES.is_sorted(),
            "the list documents itself as sorted -- keep it that way"
        );
        for name in DIRECT_SOUND_SAMPLES {
            assert!(name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        }
    }

    #[test]
    fn programmable_wave_sample_list_has_no_duplicates() {
        let unique: std::collections::HashSet<_> = PROGRAMMABLE_WAVE_SAMPLES.iter().collect();
        assert_eq!(unique.len(), PROGRAMMABLE_WAVE_SAMPLES.len());
        for n in PROGRAMMABLE_WAVE_SAMPLES {
            assert!(n >= 1);
        }
    }

    /// Pins the wire format byte-for-byte against
    /// `crates/assets::audio::sample::Sample::encode`'s documented layout
    /// (that crate's own `direct_sound_looping_round_trips` test uses the
    /// same `base_frequency`/`loop_start`/`data` triple) -- the two encoders are
    /// deliberately not shared code (module docs), so each side pins its
    /// own understanding of the format.
    #[test]
    fn encode_direct_sound_matches_the_documented_wire_format() {
        let bytes = encode_direct_sound(0x0012_3456, Some(42), &[-1, 0, 1, 127, -128]);
        let mut expected = Vec::new();
        expected.push(0u8); // KIND_DIRECT_SOUND
        expected.extend_from_slice(&0x0012_3456u32.to_le_bytes());
        expected.push(1); // looping = true
        expected.extend_from_slice(&42u32.to_le_bytes());
        expected.extend_from_slice(&5u32.to_le_bytes()); // sample count
        expected.extend_from_slice(&[0xFF, 0x00, 0x01, 0x7F, 0x80]); // -1,0,1,127,-128 as bytes
        assert_eq!(bytes, expected);
    }

    #[test]
    fn encode_direct_sound_one_shot_writes_a_false_loop_flag_and_zero_start() {
        let bytes = encode_direct_sound(1 << 20, None, &[-128, -1, 0, 1, 127]);
        let mut expected = Vec::new();
        expected.push(0u8);
        expected.extend_from_slice(&(1u32 << 20).to_le_bytes());
        expected.push(0); // looping = false
        expected.extend_from_slice(&0u32.to_le_bytes()); // loop_start_field, unused
        expected.extend_from_slice(&5u32.to_le_bytes());
        expected.extend_from_slice(&[0x80, 0xFF, 0x00, 0x01, 0x7F]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn encode_programmable_wave_matches_the_documented_wire_format() {
        let mut table = [0u8; PROGRAMMABLE_WAVE_SIZE];
        for (i, b) in table.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            {
                *b = (i * 17) as u8;
            }
        }
        let bytes = encode_programmable_wave(&table);
        let mut expected = vec![1u8]; // KIND_PROGRAMMABLE_WAVE
        expected.extend_from_slice(&table);
        assert_eq!(bytes, expected);
    }
}
