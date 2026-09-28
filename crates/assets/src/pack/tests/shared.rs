//! Pins the synthetic-pack fixture every other test module in this suite
//! shares: a hand-serialized pack byte layout, never real upstream art (per
//! the issue's CI caveat — CI has no `pokeemerald/` checkout and no real
//! pack).

use super::super::{FORMAT_VERSION, MAGIC};
use crate::audio::{
    DirectSoundMode, DirectSoundSample, DirectSoundVoice, Envelope, ProgrammableWave, Sample,
    SampleId, Song, SongEvent, VoiceEntry, VoiceGroup, VoiceGroupId,
};

/// A valid one-GBA-bank text-window palette payload (16 colours, 32
/// bytes — the shape the typed text-window accessors enforce), with the
/// first two colours set so each fixture entry stays distinguishable.
pub(super) fn text_window_palette_payload(first: u16, second: u16) -> Vec<u8> {
    let mut payload = Vec::with_capacity(32);
    payload.extend_from_slice(&first.to_le_bytes());
    payload.extend_from_slice(&second.to_le_bytes());
    payload.resize(32, 0);
    payload
}

/// The 16 colours of [`text_window_palette_payload`], as the values
/// [`PaletteRef::colors`](super::super::PaletteRef::colors) should yield.
pub(super) fn text_window_palette_colors(first: u16, second: u16) -> Vec<u16> {
    let mut colors = vec![first, second];
    colors.resize(16, 0);
    colors
}

/// Fixed image-entry metadata: width, height, bit depth.
pub(super) fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

/// A valid 24x24 border-frame pixel buffer (the exact shape
/// `text_window_frame` requires), seeded so each fixture frame stays
/// distinguishable; every index stays within a 16-colour palette.
pub(super) fn frame_pixels(seed: u8) -> Vec<u8> {
    (0..24u16 * 24)
        .map(|i| u8::try_from((i + u16::from(seed)) % 16).unwrap())
        .collect()
}

/// A valid 56x16 message-box pixel buffer (the exact shape `message_box`
/// requires); every index stays within a 16-colour palette.
pub(super) fn message_box_pixels() -> Vec<u8> {
    (0..56u16 * 16)
        .map(|i| u8::try_from(i % 16).unwrap())
        .collect()
}

/// One entry of a hand-serialized fixture pack: an id, its kind tag, the
/// kind-specific metadata bytes, and the payload.
pub(super) struct Entry {
    pub(super) id: &'static str,
    pub(super) kind_tag: u8,
    pub(super) meta: Vec<u8>,
    pub(super) payload: Vec<u8>,
}

/// Build a tiny, synthetic pack in memory (never real upstream art — per
/// the issue's CI caveat, no test in this crate touches `pokeemerald/` or
/// the real extracted pack) with one entry of each kind: an `Image`, a
/// `Palette`, and a `Raw` blob.
// Long because it lists one fixture entry per accessor this module tests
// (tileset/layout/font/text-window) — splitting the literal list across
// helper functions would just move the line count, not reduce it.
#[allow(clippy::too_many_lines)]
pub(super) fn synthetic_pack() -> Vec<u8> {
    let entries = vec![
        Entry {
            id: "tileset/test/tiles",
            kind_tag: 0,
            meta: {
                let mut m = Vec::new();
                m.extend_from_slice(&2u32.to_le_bytes()); // width
                m.extend_from_slice(&2u32.to_le_bytes()); // height
                m.push(8); // bit_depth
                m
            },
            payload: vec![1, 2, 3, 4],
        },
        Entry {
            id: "tileset/test/palette/00",
            kind_tag: 1,
            meta: 2u16.to_le_bytes().to_vec(),     // color_count
            payload: vec![0xFF, 0x7F, 0x00, 0x00], // two GBA555 colors
        },
        Entry {
            id: "tileset/test/metatiles",
            kind_tag: 2,
            meta: vec![],
            payload: vec![9, 9, 9],
        },
        Entry {
            id: "tileset/test/metatile_attributes",
            kind_tag: 2,
            meta: vec![],
            // Three valid behavior/layer-type pairs, little-endian.
            payload: vec![0x01, 0x00, 0x02, 0x10, 0x03, 0x20],
        },
        Entry {
            id: "layout/test/map",
            kind_tag: 2,
            meta: vec![],
            // A 2x1 grid: two raw MetatileCell u16s, little-endian.
            payload: vec![0x01, 0x00, 0x02, 0x00],
        },
        Entry {
            id: "layout/test/border",
            kind_tag: 2,
            meta: vec![],
            // A fixed 2x2 border block (8 bytes).
            payload: vec![0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00],
        },
        Entry {
            id: "font/normal/glyphs",
            kind_tag: 0,
            meta: {
                let mut m = Vec::new();
                m.extend_from_slice(&2u32.to_le_bytes()); // width
                m.extend_from_slice(&2u32.to_le_bytes()); // height
                m.push(2); // bit_depth
                m
            },
            payload: vec![0, 1, 2, 3],
        },
        Entry {
            id: "text-window/image/1",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels(0),
        },
        Entry {
            id: "text-window/palette/1",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0011, 0x0022),
        },
        // Frame source `2`'s image is fine, but its palette is malformed
        // (2 declared colours in 4 bytes) — read-side validation fodder.
        Entry {
            id: "text-window/image/2",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels(1),
        },
        Entry {
            id: "text-window/palette/2",
            kind_tag: 1,
            meta: 2u16.to_le_bytes().to_vec(),
            payload: vec![0x11, 0x00, 0x22, 0x00],
        },
        // Frame source `3` inverts it: a valid 16-colour palette, but an
        // 8-bit-indexed image whose pixel 16 that palette cannot map.
        Entry {
            id: "text-window/image/3",
            kind_tag: 0,
            meta: image_meta(24, 24, 8),
            payload: {
                let mut pixels = frame_pixels(2);
                pixels[5] = 16;
                pixels
            },
        },
        Entry {
            id: "text-window/palette/3",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0099, 0x00AA),
        },
        // Frame source `5` is a self-consistent 8x8 bitmap (payload
        // matches its declared dimensions) — but a border frame must be a
        // complete 3x3 grid of 8x8 tiles, i.e. exactly 24x24.
        Entry {
            id: "text-window/image/5",
            kind_tag: 0,
            meta: image_meta(8, 8, 4),
            payload: vec![0; 64],
        },
        Entry {
            id: "text-window/palette/5",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x00DD, 0x00EE),
        },
        Entry {
            id: "text-window/image/20",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame_pixels(3),
        },
        Entry {
            id: "text-window/palette/20",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0077, 0x0088),
        },
        Entry {
            id: "text-window/image/message_box",
            kind_tag: 0,
            meta: image_meta(56, 16, 4),
            payload: message_box_pixels(),
        },
        Entry {
            id: "text-window/palette/message_box",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0033, 0x0044),
        },
        Entry {
            id: "text-window/palette/text_pal1",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0055, 0x0066),
        },
        // `AssetPack::song`/`voicegroup`/`sample` fixtures (issue #184):
        // one well-formed entry per accessor, referencing each other the
        // same way a real extracted pack's `audio/song/mus_title` chain
        // does (song -> voicegroup -> sample), plus one malformed entry per
        // schema for the accessors' decode-error path, and one `not_raw`
        // entry (an `Image`, not `Raw`) for the `WrongKind` path shared by
        // all three.
        Entry {
            id: "audio/song/test_song",
            kind_tag: 2,
            meta: vec![],
            payload: Song::new(
                VoiceGroupId("audio/voicegroup/test_group".to_owned()),
                5,
                Some(30),
                vec![vec![
                    SongEvent::Voice(2),
                    SongEvent::Wait(4),
                    SongEvent::Fine,
                ]],
            )
            .unwrap()
            .encode(),
        },
        Entry {
            id: "audio/song/not_raw",
            kind_tag: 0,
            meta: image_meta(1, 1, 8),
            payload: vec![0],
        },
        // A `u16` string-length prefix (`0xFFFF`) with no bytes behind it:
        // `Song::decode` fails reading the leading `voicegroup` id, before
        // any other field, so this is a minimal, deterministic
        // `AudioError::Truncated`.
        Entry {
            id: "audio/song/malformed",
            kind_tag: 2,
            meta: vec![],
            payload: vec![0xFF, 0xFF],
        },
        Entry {
            id: "audio/voicegroup/test_group",
            kind_tag: 2,
            meta: vec![],
            payload: VoiceGroup::new(vec![
                VoiceEntry::DirectSound(DirectSoundVoice {
                    base_key: 60,
                    pan: None,
                    sample: SampleId("audio/sample/direct-sound/test_sample".to_owned()),
                    envelope: Envelope {
                        attack: 0,
                        decay: 0,
                        sustain: 15,
                        release: 0,
                    },
                    mode: DirectSoundMode::Resampled,
                }),
                VoiceEntry::Empty,
            ])
            .unwrap()
            .encode(),
        },
        // A declared slot count (`200`) over `VOICE_SLOT_COUNT` (128) is
        // rejected before any slot body is read -- a minimal, deterministic
        // `AudioError::TooManyVoiceSlots`.
        Entry {
            id: "audio/voicegroup/malformed",
            kind_tag: 2,
            meta: vec![],
            payload: vec![200],
        },
        Entry {
            id: "audio/sample/direct-sound/test_sample",
            kind_tag: 2,
            meta: vec![],
            payload: Sample::DirectSound(
                DirectSoundSample::new(12345, Some(2), 3, vec![-1, 0, 1, 2]).unwrap(),
            )
            .encode(),
        },
        Entry {
            id: "audio/sample/programmable-wave/01",
            kind_tag: 2,
            meta: vec![],
            payload: Sample::ProgrammableWave(ProgrammableWave { table: [7; 16] }).encode(),
        },
        // An unrecognized kind tag (`0xFF`) is rejected reading the very
        // first byte -- a minimal, deterministic `AudioError::UnknownSampleKind`.
        Entry {
            id: "audio/sample/malformed",
            kind_tag: 2,
            meta: vec![],
            payload: vec![0xFF],
        },
    ];
    pack_bytes(entries)
}

/// Serialize `entries` into a pack file's bytes by hand, without going
/// through `pack_format::PackWriter`, so these tests pin the layout
/// independently of the writer's own idea of it.
pub(super) fn pack_bytes(mut entries: Vec<Entry>) -> Vec<u8> {
    // Directory entries must be written in id-sorted order, exactly like
    // the real writer (`pack_format::PackWriter::finish`) -- sort here
    // rather than trusting the literal array order above, so
    // reordering/adding fixture entries later can't quietly reintroduce an
    // unsorted directory the reader's binary search then misses.
    entries.sort_by(|a, b| a.id.cmp(b.id));

    let header_size = 8 + 4 + 4;
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += 2 + e.id.len() + 1 + 8 + 8 + e.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for e in &entries {
        offsets.push(offset);
        offset += e.payload.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes()); // format_version
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (e, &off) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(e.id.as_bytes());
        out.push(e.kind_tag);
        out.extend_from_slice(&(off as u64).to_le_bytes());
        out.extend_from_slice(&(e.payload.len() as u64).to_le_bytes());
        out.extend_from_slice(&e.meta);
    }
    for e in &entries {
        out.extend_from_slice(&e.payload);
    }
    out
}

pub(super) fn write_synthetic_pack(dir_hint: &str) -> std::path::PathBuf {
    write_pack(dir_hint, &synthetic_pack())
}

/// Put `bytes` where [`super::super::AssetPack::load`] can read them back as
/// a pack file.
pub(super) fn write_pack(dir_hint: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-assets-pack-test-{dir_hint}-{}.pack",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    path
}
