//! A hand-rolled WAV reader for upstream's `DirectSound` instrument sample
//! sources (`pokeemerald/sound/direct_sound_samples/*.wav`), reimplementing
//! (idiomatically, not verbatim — `no-verbatim`) exactly the subset of
//! `.wav` chunks and the pitch/loop-point derivation that upstream's own
//! build tool, `tools/wav2agb`, uses to compile these into the GBA
//! `WaveData` binary the real cartridge ships (`pokeemerald/tools/wav2agb`:
//! `wav_file.cpp`, `converter.cpp`).
//!
//! # Why WAV, not AIFF
//!
//! This decomp checkout's direct-sound sample sources are `.wav` (RIFF),
//! compiled by `tools/wav2agb` — not `.aif`/`aif2pcm`, which some older
//! decomp forks used. `pokeemerald/tools/aif2pcm` does not exist in this
//! tree; `pokeemerald/tools/wav2agb` does, and
//! `pokeemerald/audio_rules.mk`'s uncompressed-sound rule
//! (`$(SOUND_BIN_DIR)/%.bin: sound/%.wav`, `audio_rules.mk:24-26`) is what
//! actually builds `sound/direct_sound_samples/*.wav` into the compiled
//! `.bin` the linker embeds — confirmed against the real checkout before
//! writing this module, rather than assumed from the issue's original
//! (aif2pcm-era) wording.
//!
//! # Chunk layout this reader understands
//!
//! - `fmt `: `wFormatTag` (`1` = integer PCM, `3` = IEEE float — any other
//!   tag is [`WavError::UnsupportedFormat`]), channel count (must be `1`;
//!   every upstream direct-sound sample is mono), sample rate, and the
//!   `(block_align, bits_per_sample)` pair, matching exactly the six
//!   combinations `wav_file.cpp`'s constructor accepts
//!   (`wav_file.cpp:125-156`): 8-bit unsigned, 16/24/32-bit signed integer,
//!   or 32/64-bit IEEE float. Every other combination is
//!   [`WavError::UnsupportedFormat`] — this is this format's analogue of
//!   the issue's "unsupported compression type" fail-closed requirement
//!   (WAV has no AIFF-C-style compression tag; an unrecognized
//!   `wFormatTag`/width combination is the equivalent failure mode).
//! - `smpl`: the standard MIDI sampler chunk. `MIDIUnityNote` (offset 12)
//!   and `MIDIPitchFraction` (offset 16) feed the pitch formula below;
//!   `MIDIPitchFraction` converts to cents the same way
//!   `wav_file.cpp:170` does (`fraction / (2^32 * 100)`; upstream's own
//!   comment at `:169` glosses that as `0.0..100.0` cents, though the
//!   expression actually maps to `0.0..0.01` -- mirrored as-is for
//!   fidelity, and moot in practice: every shipped sample has pitch
//!   fraction `0` and an `agbp` override besides). `NumSampleLoops` (offset 28) must be `0` or
//!   `1` — more than one loop is [`WavError::TooManySampleLoops`],
//!   matching `wav_file.cpp:172-173`'s own rejection. When one loop is
//!   present, its `Type` (offset 40 relative to the chunk start; `0` is
//!   forward-only, the only type real samples use — any other is
//!   [`WavError::UnsupportedLoopType`], `wav_file.cpp:175-177`), `Start`
//!   (offset 44), and `End` (offset 48) set [`WavSample::loop_start`] and
//!   the *naive* loop end. The sampler chunk's `End` is the last sample
//!   *played* (inclusive), so the naive exclusive end is `End + 1`
//!   (`wav_file.cpp:180`), clamped to the file's total sample count
//!   (`wav_file.cpp:208`).
//! - `agbp` / `agbl`: two non-standard chunks this decomp's fork of
//!   `wav2agb` adds (`README.md:6-11`, `:9-11`), each a single little-endian
//!   `u32`. A *zero* word in either chunk means "no override", not "override
//!   with zero": upstream stores both in fields that default to `0` and
//!   consults them only when non-zero (`converter.cpp:392-397` for `agbp`,
//!   `:399-402` for `agbl`), so a present-but-zero chunk falls back to the
//!   derived value here too. `agbp` is an *exact* override for the compiled
//!   pitch word, needed because recomputing it from the sample rate/MIDI key
//!   can lose precision the original build didn't (`README.md:7`) — see
//!   [`derive_pitch`]. `agbl` is an exact override for the compiled
//!   `WaveData::size` word, correcting a genuine upstream-tool quirk:
//!   every shipped direct-sound sample's compiled data is one sample
//!   *shorter* than the naive loop-end calculation above would produce
//!   (`README.md:9-11`, `:43`) — every one of the 32 samples this pipeline
//!   extracts (title voicegroup, see [`super::audio_samples`]) carries both
//!   chunks, with `agbl` equal to `naive_loop_end - 1` in every case, so
//!   honouring the override (rather than reimplementing that arithmetic
//!   directly) is what makes this reader agree with whichever future
//!   sample the pipeline adds, `--set-agbl`-authored quirk and all.
//! - `data`: the raw PCM payload. Decoded to this schema's normalized
//!   `f64` range and then to a signed 8-bit sample exactly as
//!   `convert_uncompressed`/`convert_uncompressed_bin` do
//!   (`converter.cpp:56-92`): `(sample * 128.0).floor()`, clamped to
//!   `-128..=127`.
//!
//! # Pitch derivation ([`WavSample::base_frequency`])
//!
//! Mirrors `converter.cpp:385-397`. Given the sample rate `sr`, MIDI unity
//! note `key` (default `60`, i.e. middle C — the value most upstream
//! samples use, meaning no pitch shift), and `tuning` in cents (default
//! `0.0`):
//!
//! ```text
//! pitch = sr                                         if key == 60 && tuning == 0.0
//!       = sr * 2^((60 - key) / 12 + tuning / 1200)    otherwise
//! base_frequency = agbp, if non-zero, else (pitch * 1024) truncated to a u32
//! ```
//!
//! The truncation is deliberate and not a rounding: `converter.cpp:396`
//! writes `static_cast<uint32_t>(pitch * 1024.0)`, which discards the
//! fractional part rather than rounding to nearest, and this reader has to
//! agree with the word the real cartridge ships.
//!
//! The `* 1024` fixed-point scale is what
//! `crates/assets/src/audio/sample.rs`'s [`base_frequency`
//! docs](../../../assets/src/audio/sample.rs) mean by "pre-scaled" — it is
//! `upstream WaveData::freq` untouched, not a plain sample rate.
//!
//! # Sample count / loop-point derivation ([`WavSample::data`], [`WavSample::loop_start`])
//!
//! The compiled `WaveData::size` word (`agbl`, if present and non-zero,
//! else the naive loop end above) is this schema's `data.len()`:
//! `crates/assets`'s `Sample::decode` and
//! this reader agree that a `DirectSoundSample`'s `data` is exactly the
//! samples the real mixer would ever read, nothing more. This deliberately
//! excludes two artifacts of the *compiled binary*'s own physical layout
//! that carry no additional waveform content:
//!
//! - The zero bytes the shipped layout pads with. This tree builds these
//!   sources with `wav2agb -b` (`audio_rules.mk:26`), i.e. binary output,
//!   whose sample region is written by `convert_uncompressed_bin`
//!   (`converter.cpp:77-92`, called from the binary-output branch at
//!   `:426`): exactly `loopEnd` samples, then zero bytes until the emitted
//!   `.bin`'s total length is a multiple of 4 (`:88-91`). That path appends
//!   **no** guard sample — the padding is physical 4-byte alignment
//!   carrying no waveform, so this schema drops it.
//!
//!   The `.s`-output variant, `convert_uncompressed` (`converter.cpp:56-75`,
//!   called at `:457`), is *not* what this tree builds, but it is worth
//!   naming: it appends one extra sample after the declared loop end
//!   (`:74`) — a copy of the sample at `loop_start`, or silence for a
//!   one-shot — purely so the original hardware mixer's linear
//!   interpolation has a defined value to read one sample past the wrap
//!   point. That is the motivation for the consumer discipline this schema
//!   assumes either way: a mixer over these samples wraps its own read
//!   index back to `loop_start`, so it never needs a physically-duplicated
//!   guard sample and neither layout has to supply one.
//! - Any bytes beyond `agbl` when the naive loop end exceeds it. The naive
//!   end (`wf.loopEnd`) is what bounds the sample loop that actually writes
//!   payload bytes (`converter.cpp:79`); the `agbl` override applies only
//!   to the `loop_end` word written into the compiled header (`:399-402`,
//!   written at `:422`), never to how many samples get emitted. So the real
//!   compiled binary can carry samples past `agbl`. One of those bytes *is*
//!   reachable on hardware: the interpolating type-0 `DirectSound` mixer
//!   loads the sample one past the current index (`m4a_1.s:400-401`,
//!   `:424-425`), so while rendering index `size - 1` it blends with the
//!   physical byte at offset `size`. Dropping it is still lossless for
//!   every shipped source, because that byte is `pcm[loop_start]` for a
//!   looped sample and `0` for a one-shot (`wav2agb -b` output, verified
//!   across the tree) — exactly the values a consumer reconstructs by
//!   wrapping to `loop_start` or extending silence. Whether the pack
//!   should carry the guard byte physically instead is tracked in
//!   issue #200.
//!
//! `loop_start` is `Some(smpl loop Start)` when the sampler chunk declares
//! exactly one loop, `None` otherwise (matching
//! `crates/assets/src/audio/sample.rs`'s "absent, not a don't-care `0`"
//! modelling) — there is no override chunk for the loop *start*, only for
//! the derived pitch and size words.

use std::fmt;

/// An error decoding a `.wav` source into a [`WavSample`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WavError {
    /// The file did not start with the 4-byte `RIFF` magic.
    BadRiffMagic,
    /// The RIFF chunk's declared length (plus the 8-byte RIFF header) did
    /// not match the file's actual length.
    RiffSizeMismatch { declared: u32, actual: usize },
    /// The RIFF form type was not `WAVE`.
    BadWaveMagic,
    /// A sub-chunk's header or declared body ran past the end of the file.
    ChunkTruncated,
    /// No `fmt ` chunk was present.
    MissingFmtChunk,
    /// No `data` chunk was present.
    MissingDataChunk,
    /// The `fmt ` chunk declared more than one channel. Every upstream
    /// direct-sound sample is mono.
    NotMono { channels: u16 },
    /// The `fmt ` chunk's `(format tag, block align, bits per sample)`
    /// combination is not one of the six `tools/wav2agb` supports (see the
    /// module docs).
    UnsupportedFormat {
        format_tag: u16,
        block_align: u16,
        bits_per_sample: u16,
    },
    /// The `data` chunk's length was not a whole number of samples for the
    /// declared format.
    DataLengthNotAligned {
        data_len: usize,
        bytes_per_sample: u32,
    },
    /// A `smpl` chunk was present but shorter than its declared loop count
    /// requires.
    TruncatedSmplChunk,
    /// A `smpl` chunk declared more than one sample loop (`wav_file.cpp`
    /// rejects this too — no upstream sample needs more than one).
    TooManySampleLoops { count: u32 },
    /// A `smpl` chunk's loop type was not `0` (forward-only, the only type
    /// any upstream sample uses).
    UnsupportedLoopType { loop_type: u32 },
    /// The derived sample count (`agbl`, if present and non-zero, else the
    /// naive loop end) exceeds the number of samples actually present in
    /// the `data` chunk.
    SizeOutOfRange { size: u32, num_samples: u32 },
    /// A `smpl` loop's start point falls at or past the derived sample
    /// count.
    LoopStartOutOfRange { loop_start: u32, size: u32 },
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRiffMagic => write!(f, "not a WAV file (missing RIFF magic)"),
            Self::RiffSizeMismatch { declared, actual } => write!(
                f,
                "RIFF chunk length ({declared}) plus header doesn't match file length ({actual})"
            ),
            Self::BadWaveMagic => write!(f, "RIFF form type is not WAVE"),
            Self::ChunkTruncated => write!(f, "WAV chunk truncated or runs past end of file"),
            Self::MissingFmtChunk => write!(f, "WAV file has no `fmt ` chunk"),
            Self::MissingDataChunk => write!(f, "WAV file has no `data` chunk"),
            Self::NotMono { channels } => {
                write!(f, "WAV file is not mono ({channels} channels)")
            }
            Self::UnsupportedFormat {
                format_tag,
                block_align,
                bits_per_sample,
            } => write!(
                f,
                "unsupported WAV format: tag={format_tag}, block_align={block_align}, \
                 bits_per_sample={bits_per_sample}"
            ),
            Self::DataLengthNotAligned {
                data_len,
                bytes_per_sample,
            } => write!(
                f,
                "WAV data chunk length ({data_len}) is not a multiple of the sample width \
                 ({bytes_per_sample})"
            ),
            Self::TruncatedSmplChunk => write!(f, "WAV `smpl` chunk is truncated"),
            Self::TooManySampleLoops { count } => {
                write!(
                    f,
                    "WAV `smpl` chunk declares {count} loops (at most 1 supported)"
                )
            }
            Self::UnsupportedLoopType { loop_type } => {
                write!(f, "WAV `smpl` chunk loop type {loop_type} is not supported (only forward-only, type 0)")
            }
            Self::SizeOutOfRange { size, num_samples } => write!(
                f,
                "derived sample count {size} exceeds the {num_samples} samples in the data chunk"
            ),
            Self::LoopStartOutOfRange { loop_start, size } => write!(
                f,
                "loop start {loop_start} is at or past the derived sample count {size}"
            ),
        }
    }
}

impl std::error::Error for WavError {}

/// A decoded `DirectSound` sample, in this schema's own units — see the
/// module docs for exactly how each field is derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WavSample {
    pub(super) base_frequency: u32,
    pub(super) loop_start: Option<u32>,
    pub(super) data: Vec<i8>,
}

/// One parsed RIFF sub-chunk: its 4-byte id and body bytes (the length
/// prefix and any trailing pad byte are already consumed).
struct Chunk<'a> {
    id: [u8; 4],
    data: &'a [u8],
}

fn read_chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, WavError> {
    let mut chunks = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= bytes.len() {
        let id = [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]];
        let len = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body_start = pos + 8;
        let body_end = body_start
            .checked_add(len)
            .ok_or(WavError::ChunkTruncated)?;
        if body_end > bytes.len() {
            return Err(WavError::ChunkTruncated);
        }
        chunks.push(Chunk {
            id,
            data: &bytes[body_start..body_end],
        });
        // RIFF chunks are padded to an even length; the pad byte itself
        // isn't part of any chunk's declared length.
        pos = body_end + (len % 2);
    }
    // Fail closed on a malformed tail: a 1-7-byte remnant is a truncated
    // chunk header, and `pos > bytes.len()` means the final odd-length
    // chunk's required pad byte is missing.
    if pos != bytes.len() {
        return Err(WavError::ChunkTruncated);
    }
    Ok(chunks)
}

fn find_chunk<'a>(chunks: &'a [Chunk<'a>], id: [u8; 4]) -> Option<&'a [u8]> {
    chunks.iter().find(|c| c.id == id).map(|c| c.data)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, WavError> {
    let b = bytes
        .get(offset..offset + 2)
        .ok_or(WavError::ChunkTruncated)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WavError> {
    let b = bytes
        .get(offset..offset + 4)
        .ok_or(WavError::ChunkTruncated)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The six `(format tag, block align, bits per sample)` combinations
/// `tools/wav2agb` accepts (`wav_file.cpp:125-156`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleFormat {
    U8,
    S16,
    S24,
    S32,
    F32,
    F64,
}

impl SampleFormat {
    const fn bytes_per_sample(self) -> u32 {
        match self {
            Self::U8 => 1,
            Self::S16 => 2,
            Self::S24 => 3,
            Self::S32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    fn resolve(format_tag: u16, block_align: u16, bits_per_sample: u16) -> Result<Self, WavError> {
        let resolved = match (format_tag, block_align, bits_per_sample) {
            (1, 1, 8) => Some(Self::U8),
            (1, 2, 16) => Some(Self::S16),
            (1, 3, 24) => Some(Self::S24),
            (1, 4, 32) => Some(Self::S32),
            (3, 4, 32) => Some(Self::F32),
            (3, 8, 64) => Some(Self::F64),
            _ => None,
        };
        resolved.ok_or(WavError::UnsupportedFormat {
            format_tag,
            block_align,
            bits_per_sample,
        })
    }

    /// Decode one sample at `bytes[index * bytes_per_sample ..]` to this
    /// schema's `-1.0..=1.0`-normalized `f64`, matching
    /// `wav_file.cpp:235-297`'s per-format scaling.
    fn normalize(self, bytes: &[u8], index: usize) -> f64 {
        let start = index * self.bytes_per_sample() as usize;
        match self {
            Self::U8 => (f64::from(bytes[start]) - 128.0) / 128.0,
            Self::S16 => {
                let v = i16::from_le_bytes([bytes[start], bytes[start + 1]]);
                f64::from(v) / 32768.0
            }
            Self::S24 => {
                let raw = u32::from(bytes[start])
                    | (u32::from(bytes[start + 1]) << 8)
                    | (u32::from(bytes[start + 2]) << 16);
                // Sign-extend the 24-bit value through a 32-bit arithmetic
                // shift, matching `wav_file.cpp:254-255`'s `s <<= 8; s >>= 8;`.
                #[allow(clippy::cast_possible_wrap)]
                let v = ((raw << 8) as i32) >> 8;
                f64::from(v) / 8_388_608.0
            }
            Self::S32 => {
                let v = i32::from_le_bytes([
                    bytes[start],
                    bytes[start + 1],
                    bytes[start + 2],
                    bytes[start + 3],
                ]);
                f64::from(v) / 2_147_483_648.0
            }
            Self::F32 => {
                let v = f32::from_le_bytes([
                    bytes[start],
                    bytes[start + 1],
                    bytes[start + 2],
                    bytes[start + 3],
                ]);
                f64::from(v)
            }
            Self::F64 => f64::from_le_bytes([
                bytes[start],
                bytes[start + 1],
                bytes[start + 2],
                bytes[start + 3],
                bytes[start + 4],
                bytes[start + 5],
                bytes[start + 6],
                bytes[start + 7],
            ]),
        }
    }
}

/// Parse the `fmt ` chunk's format tag / channel count / sample rate /
/// width fields, resolving the width into a [`SampleFormat`]. Split out of
/// [`decode`] to keep that function under this crate's line-count lint.
fn parse_fmt_chunk(fmt: &[u8]) -> Result<(SampleFormat, u32), WavError> {
    if fmt.len() < 16 {
        return Err(WavError::ChunkTruncated);
    }
    let format_tag = read_u16(fmt, 0)?;
    let num_channels = read_u16(fmt, 2)?;
    if num_channels != 1 {
        return Err(WavError::NotMono {
            channels: num_channels,
        });
    }
    let sample_rate = read_u32(fmt, 4)?;
    let block_align = read_u16(fmt, 12)?;
    let bits_per_sample = read_u16(fmt, 14)?;
    let format = SampleFormat::resolve(format_tag, block_align, bits_per_sample)?;
    Ok((format, sample_rate))
}

/// `(MIDI unity note, tuning in cents, loop start, naive loop end)` parsed
/// from a `smpl` chunk, defaulting the pitch fields to no-shift (key `60`,
/// `0.0` cents) and the loop fields to "no loop" (`None`, `num_samples`)
/// when the chunk declares zero loops. Split out of [`decode`] to keep that
/// function under this crate's line-count lint — see the module docs for
/// the field semantics.
fn parse_smpl_chunk(
    smpl: &[u8],
    num_samples: u32,
) -> Result<(u8, f64, Option<u32>, u32), WavError> {
    // The fixed `smpl` header is 36 bytes (through samplerData); each
    // declared loop record is a further 24. Anything shorter is a
    // truncated record, even if every field this parser *reads* happens
    // to sit below the cut.
    if smpl.len() < 36 {
        return Err(WavError::TruncatedSmplChunk);
    }
    let midi_unity_note = read_u32(smpl, 12)?;
    #[allow(clippy::cast_possible_truncation)]
    let midi_key = midi_unity_note.min(127) as u8;
    let midi_pitch_fraction = read_u32(smpl, 16)?;
    let tuning_cents = f64::from(midi_pitch_fraction) / (4_294_967_296.0 * 100.0);
    let num_loops = read_u32(smpl, 28)?;
    if num_loops > 1 {
        return Err(WavError::TooManySampleLoops { count: num_loops });
    }
    if num_loops == 0 {
        return Ok((midi_key, tuning_cents, None, num_samples));
    }
    if smpl.len() < 36 + 24 {
        return Err(WavError::TruncatedSmplChunk);
    }
    let loop_type = read_u32(smpl, 40)?;
    if loop_type != 0 {
        return Err(WavError::UnsupportedLoopType { loop_type });
    }
    let start = read_u32(smpl, 44)?;
    let end = read_u32(smpl, 48)?;
    let naive_size = end.saturating_add(1).min(num_samples);
    Ok((midi_key, tuning_cents, Some(start), naive_size))
}

/// `(sample rate, MIDI unity note, tuning in cents, optional exact `agbp`
/// override — already filtered to a non-zero word by [`decode`], matching
/// `converter.cpp:392-397`)` -> the compiled pitch word. See the module
/// docs.
fn derive_pitch(sample_rate: u32, midi_key: u8, tuning_cents: f64, agb_pitch: Option<u32>) -> u32 {
    if let Some(exact) = agb_pitch {
        return exact;
    }
    let pitch = if midi_key == 60 && tuning_cents == 0.0 {
        f64::from(sample_rate)
    } else {
        f64::from(sample_rate)
            * 2f64.powf((60.0 - f64::from(midi_key)) / 12.0 + tuning_cents / 1200.0)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pitch_value = (pitch * 1024.0) as u32;
    pitch_value
}

/// Decode a `.wav` file's bytes into a [`WavSample`]. See the module docs
/// for the exact chunk layout and field derivation.
///
/// # Errors
///
/// See [`WavError`]'s variants.
pub(super) fn decode(bytes: &[u8]) -> Result<WavSample, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" {
        return Err(WavError::BadRiffMagic);
    }
    let riff_len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if riff_len as usize + 8 != bytes.len() {
        return Err(WavError::RiffSizeMismatch {
            declared: riff_len,
            actual: bytes.len(),
        });
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(WavError::BadWaveMagic);
    }

    let chunks = read_chunks(&bytes[12..])?;

    let fmt = find_chunk(&chunks, *b"fmt ").ok_or(WavError::MissingFmtChunk)?;
    let (format, sample_rate) = parse_fmt_chunk(fmt)?;

    let data = find_chunk(&chunks, *b"data").ok_or(WavError::MissingDataChunk)?;
    let bytes_per_sample = format.bytes_per_sample();
    if data.len() % bytes_per_sample as usize != 0 {
        return Err(WavError::DataLengthNotAligned {
            data_len: data.len(),
            bytes_per_sample,
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    let num_samples = (data.len() / bytes_per_sample as usize) as u32;

    let (midi_key, tuning_cents, loop_start, naive_size) = match find_chunk(&chunks, *b"smpl") {
        Some(smpl) => parse_smpl_chunk(smpl, num_samples)?,
        None => (60u8, 0.0, None, num_samples),
    };

    // Both overrides are gated on the *value*, not just on the chunk being
    // present: upstream keeps them in `wav_file` fields that default to `0`
    // and applies them only `if (wf.agbPitch != 0)` / `if (wf.agbLoopEnd
    // != 0)` (`converter.cpp:392-397`, `:399-402`). So a present-but-zero
    // chunk has to fall back to the derived value here as well, rather than
    // compiling a zero pitch word or a zero-length sample.
    // A *truncated* chunk (under 4 bytes) is malformed input, not an
    // absent override: `read_u32` fails it closed as `ChunkTruncated`.
    let agb_pitch = find_chunk(&chunks, *b"agbp")
        .map(|c| read_u32(c, 0))
        .transpose()?
        .filter(|&pitch| pitch != 0);
    let agb_loop_end = find_chunk(&chunks, *b"agbl")
        .map(|c| read_u32(c, 0))
        .transpose()?
        .filter(|&size| size != 0);

    let base_frequency = derive_pitch(sample_rate, midi_key, tuning_cents, agb_pitch);
    let size = agb_loop_end.unwrap_or(naive_size);
    if size > num_samples {
        return Err(WavError::SizeOutOfRange { size, num_samples });
    }
    if let Some(start) = loop_start {
        if start >= size {
            return Err(WavError::LoopStartOutOfRange {
                loop_start: start,
                size,
            });
        }
    }

    let mut pcm = Vec::with_capacity(size as usize);
    for i in 0..size as usize {
        let normalized = format.normalize(data, i);
        let scaled = (normalized * 128.0).floor().clamp(-128.0, 127.0);
        #[allow(clippy::cast_possible_truncation)]
        pcm.push(scaled as i8);
    }

    Ok(WavSample {
        base_frequency,
        loop_start,
        data: pcm,
    })
}

#[cfg(test)]
mod tests;
