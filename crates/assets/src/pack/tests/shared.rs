use super::super::{FORMAT_VERSION, MAGIC};
use crate::audio::{
    DirectSoundMode, DirectSoundSample, DirectSoundVoice, Envelope, ProgrammableWave, Sample,
    SampleId, Song, SongEvent, VoiceEntry, VoiceGroup, VoiceGroupId,
};

pub(super) const IMAGE_KIND_TAG: u8 = 0;
pub(super) const PALETTE_KIND_TAG: u8 = 1;
pub(super) const RAW_KIND_TAG: u8 = 2;

pub(super) const TEXT_WINDOW_PALETTE_COLOR_COUNT: u16 = 16;
pub(super) const TEXT_WINDOW_PALETTE_COLOR_COUNT_USIZE: usize = 16;
const TEXT_WINDOW_PALETTE_BYTE_COUNT: usize = 32;
pub(super) const SHORT_TEXT_WINDOW_PALETTE_COLOR_COUNT: u16 = 2;
pub(super) const SHORT_TEXT_WINDOW_PALETTE_BYTE_COUNT: usize = 4;
pub(super) const TEXT_WINDOW_BIT_DEPTH: u8 = 4;
pub(super) const INDEXED_8_BIT: u8 = 8;
const FONT_2_BIT: u8 = 2;

pub(super) const EXPECTED_FRAME_WIDTH: u32 = 24;
pub(super) const EXPECTED_FRAME_HEIGHT: u32 = 24;
pub(super) const WRONG_FRAME_WIDTH: u32 = 8;
pub(super) const WRONG_FRAME_HEIGHT: u32 = 8;
pub(super) const EXPECTED_MESSAGE_BOX_WIDTH: u32 = 56;
pub(super) const EXPECTED_MESSAGE_BOX_HEIGHT: u32 = 16;
const MINI_IMAGE_WIDTH: u32 = 2;
const MINI_IMAGE_HEIGHT: u32 = 2;
const MINI_PALETTE_COLOR_COUNT: u16 = 2;
pub(super) const FIRST_PIXEL_OUTSIDE_TEXT_WINDOW_PALETTE: u8 = 16;

pub(super) const EXCESS_VOICE_SLOT_COUNT_BYTE: u8 = 200;
pub(super) const PROGRAMMABLE_WAVE_BYTE_COUNT: usize = 16;
pub(super) const UNKNOWN_SAMPLE_KIND_TAG: u8 = 0xFF;
const TRUNCATED_SONG_VOICEGROUP_ID_LENGTH: u16 = 0xFFFF;

const MAGIC_FIELD_BYTES: usize = 8;
const FORMAT_VERSION_FIELD_BYTES: usize = 4;
const ENTRY_COUNT_FIELD_BYTES: usize = 4;
const ID_LENGTH_FIELD_BYTES: usize = 2;
const KIND_TAG_FIELD_BYTES: usize = 1;
const PAYLOAD_OFFSET_FIELD_BYTES: usize = 8;
const PAYLOAD_LENGTH_FIELD_BYTES: usize = 8;

const WRONG_FRAME_PIXEL_COUNT: usize = 64;

pub(super) fn text_window_palette_payload(first: u16, second: u16) -> Vec<u8> {
    let mut payload = Vec::with_capacity(TEXT_WINDOW_PALETTE_BYTE_COUNT);
    payload.extend_from_slice(&first.to_le_bytes());
    payload.extend_from_slice(&second.to_le_bytes());
    payload.resize(TEXT_WINDOW_PALETTE_BYTE_COUNT, 0);
    payload
}

pub(super) fn text_window_palette_colors(first: u16, second: u16) -> Vec<u16> {
    let mut colors = vec![first, second];
    colors.resize(TEXT_WINDOW_PALETTE_COLOR_COUNT_USIZE, 0);
    colors
}

pub(super) fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

pub(super) fn frame_pixels(seed: u8) -> Vec<u8> {
    (0..EXPECTED_FRAME_WIDTH * EXPECTED_FRAME_HEIGHT)
        .map(|i| {
            u8::try_from((i + u32::from(seed)) % u32::from(TEXT_WINDOW_PALETTE_COLOR_COUNT))
                .unwrap()
        })
        .collect()
}

pub(super) fn message_box_pixels() -> Vec<u8> {
    (0..EXPECTED_MESSAGE_BOX_WIDTH * EXPECTED_MESSAGE_BOX_HEIGHT)
        .map(|i| u8::try_from(i % u32::from(TEXT_WINDOW_PALETTE_COLOR_COUNT)).unwrap())
        .collect()
}

pub(super) struct Entry {
    pub(super) id: &'static str,
    pub(super) kind_tag: u8,
    pub(super) meta: Vec<u8>,
    pub(super) payload: Vec<u8>,
}

#[expect(
    clippy::too_many_lines,
    reason = "one fixture entry per accessor under test; helpers would move the lines, not remove them"
)]
pub(super) fn synthetic_pack() -> Vec<u8> {
    let entries = vec![
        Entry {
            id: "tileset/test/tiles",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(MINI_IMAGE_WIDTH, MINI_IMAGE_HEIGHT, INDEXED_8_BIT),
            payload: vec![1, 2, 3, 4],
        },
        Entry {
            id: "tileset/test/palette/00",
            kind_tag: PALETTE_KIND_TAG,
            meta: MINI_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: vec![0xFFu8, 0x7F, 0x00, 0x00],
        },
        Entry {
            id: "tileset/test/metatiles",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![9, 9, 9],
        },
        Entry {
            id: "tileset/test/metatile_attributes",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![0x01u8, 0x00, 0x02, 0x10, 0x03, 0x20],
        },
        Entry {
            id: "layout/test/map",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![0x01u8, 0x00, 0x02, 0x00],
        },
        Entry {
            id: "layout/test/border",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![0x01u8, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00],
        },
        Entry {
            id: "font/normal/glyphs",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(MINI_IMAGE_WIDTH, MINI_IMAGE_HEIGHT, FONT_2_BIT),
            payload: vec![0, 1, 2, 3],
        },
        Entry {
            id: "text-window/image/1",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(
                EXPECTED_FRAME_WIDTH,
                EXPECTED_FRAME_HEIGHT,
                TEXT_WINDOW_BIT_DEPTH,
            ),
            payload: frame_pixels(0),
        },
        Entry {
            id: "text-window/palette/1",
            kind_tag: PALETTE_KIND_TAG,
            meta: TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0011, 0x0022),
        },
        Entry {
            id: "text-window/image/2",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(
                EXPECTED_FRAME_WIDTH,
                EXPECTED_FRAME_HEIGHT,
                TEXT_WINDOW_BIT_DEPTH,
            ),
            payload: frame_pixels(1),
        },
        Entry {
            id: "text-window/palette/2",
            kind_tag: PALETTE_KIND_TAG,
            meta: SHORT_TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: vec![0x11u8, 0x00, 0x22, 0x00],
        },
        Entry {
            id: "text-window/image/3",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(EXPECTED_FRAME_WIDTH, EXPECTED_FRAME_HEIGHT, INDEXED_8_BIT),
            payload: {
                let mut pixels = frame_pixels(2);
                pixels[5] = FIRST_PIXEL_OUTSIDE_TEXT_WINDOW_PALETTE;
                pixels
            },
        },
        Entry {
            id: "text-window/palette/3",
            kind_tag: PALETTE_KIND_TAG,
            meta: TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0099, 0x00AA),
        },
        Entry {
            id: "text-window/image/5",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(WRONG_FRAME_WIDTH, WRONG_FRAME_HEIGHT, TEXT_WINDOW_BIT_DEPTH),
            payload: vec![0; WRONG_FRAME_PIXEL_COUNT],
        },
        Entry {
            id: "text-window/palette/5",
            kind_tag: PALETTE_KIND_TAG,
            meta: TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x00DD, 0x00EE),
        },
        Entry {
            id: "text-window/image/20",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(
                EXPECTED_FRAME_WIDTH,
                EXPECTED_FRAME_HEIGHT,
                TEXT_WINDOW_BIT_DEPTH,
            ),
            payload: frame_pixels(3),
        },
        Entry {
            id: "text-window/palette/20",
            kind_tag: PALETTE_KIND_TAG,
            meta: TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0077, 0x0088),
        },
        Entry {
            id: "text-window/image/message_box",
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(
                EXPECTED_MESSAGE_BOX_WIDTH,
                EXPECTED_MESSAGE_BOX_HEIGHT,
                TEXT_WINDOW_BIT_DEPTH,
            ),
            payload: message_box_pixels(),
        },
        Entry {
            id: "text-window/palette/message_box",
            kind_tag: PALETTE_KIND_TAG,
            meta: TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0033, 0x0044),
        },
        Entry {
            id: "text-window/palette/text_pal1",
            kind_tag: PALETTE_KIND_TAG,
            meta: TEXT_WINDOW_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: text_window_palette_payload(0x0055, 0x0066),
        },
        Entry {
            id: "audio/song/test_song",
            kind_tag: RAW_KIND_TAG,
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
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(1, 1, INDEXED_8_BIT),
            payload: vec![0],
        },
        Entry {
            id: "audio/song/malformed",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: TRUNCATED_SONG_VOICEGROUP_ID_LENGTH.to_le_bytes().to_vec(),
        },
        Entry {
            id: "audio/voicegroup/test_group",
            kind_tag: RAW_KIND_TAG,
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
        Entry {
            id: "audio/voicegroup/malformed",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![EXCESS_VOICE_SLOT_COUNT_BYTE],
        },
        Entry {
            id: "audio/sample/direct-sound/test_sample",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: Sample::DirectSound(
                DirectSoundSample::new(12345, Some(2), 3, vec![-1, 0, 1, 2]).unwrap(),
            )
            .encode(),
        },
        Entry {
            id: "audio/sample/programmable-wave/01",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: Sample::ProgrammableWave(ProgrammableWave {
                table: [7; PROGRAMMABLE_WAVE_BYTE_COUNT],
            })
            .encode(),
        },
        Entry {
            id: "audio/sample/malformed",
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![UNKNOWN_SAMPLE_KIND_TAG],
        },
    ];
    pack_bytes(entries)
}

/// Serialize independently of `PackWriter` so reader tests do not inherit writer layout errors.
pub(super) fn pack_bytes(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.id.cmp(b.id));

    let header_size = MAGIC_FIELD_BYTES + FORMAT_VERSION_FIELD_BYTES + ENTRY_COUNT_FIELD_BYTES;
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += ID_LENGTH_FIELD_BYTES
            + e.id.len()
            + KIND_TAG_FIELD_BYTES
            + PAYLOAD_OFFSET_FIELD_BYTES
            + PAYLOAD_LENGTH_FIELD_BYTES
            + e.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for e in &entries {
        offsets.push(offset);
        offset += e.payload.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
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

pub(super) fn write_pack(dir_hint: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-assets-pack-test-{dir_hint}-{}.pack",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    path
}
