//! Voicegroup and song-bytecode regression tests for the audio locator.

use assets::audio::{VoiceEntry, VoiceGroup};
use pack_format::image_entry;

use super::super::audio;
use super::super::error::GenRomProfileError;
use super::super::plan::AudioPlan;
use super::audio::{
    title_song_payload, title_voicegroup_payload, title_voicegroup_slots, with_audio_pack,
    with_audio_pack_entry, with_audio_song_rom,
};

/// Locate against a pack whose `audio/voicegroup/title` holds `voicegroup`.
fn locate_with_voicegroup(
    name: &str,
    voicegroup: Option<&[u8]>,
) -> Result<AudioPlan, GenRomProfileError> {
    with_audio_pack(
        name,
        &["audio/song/mus_title"],
        &title_song_payload(0),
        voicegroup,
        |_| {},
        |ctx| audio::locate(ctx, &mut Vec::new()),
    )
}

#[test]
fn a_missing_voicegroup_entry_is_refused() {
    let err = locate_with_voicegroup("audio-voicegroup-missing", None)
        .expect_err("a pack without the voicegroup cannot corroborate the root");
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "audio/voicegroup/title"),
        "{err:?}"
    );
}

#[test]
fn a_malformed_voicegroup_entry_is_refused() {
    let err = locate_with_voicegroup("audio-voicegroup-malformed", Some(&[129]))
        .expect_err("a payload that is not a voicegroup is refused");
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == "audio/voicegroup/title"),
        "{err:?}"
    );
}

#[test]
fn a_voicegroup_entry_that_disagrees_with_the_rom_is_refused() {
    // Same slot count and sample as the ROM; only one instrument field
    // differs, so a refusal can only come from comparing the group's content.
    let mut slots = title_voicegroup_slots();
    let VoiceEntry::DirectSound(voice) = &mut slots[0] else {
        panic!("slot 0 is the DirectSound voice");
    };
    voice.envelope.release = 164;
    let altered = VoiceGroup::new(slots).expect("valid group").encode();
    let err = locate_with_voicegroup("audio-voicegroup-mismatch", Some(&altered))
        .expect_err("an altered instrument is refused");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/voicegroup/title"),
        "{err:?}"
    );

    // A borrowed slot past the declared one is compared too.
    let mut slots = title_voicegroup_slots();
    let VoiceEntry::Square1(voice) = &mut slots[5] else {
        panic!("slot 5 is a square-1 voice");
    };
    voice.duty = 1;
    let altered = VoiceGroup::new(slots).expect("valid group").encode();
    let err = locate_with_voicegroup("audio-voicegroup-borrowed", Some(&altered))
        .expect_err("an altered borrowed slot is refused");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/voicegroup/title"),
        "{err:?}"
    );
}

#[test]
fn a_voicegroup_entry_that_matches_the_rom_is_accepted() {
    let plan = locate_with_voicegroup("audio-voicegroup-match", Some(&title_voicegroup_payload()))
        .expect("the matching entry locates");
    assert_eq!(plan.voicegroups.len(), 1);
}

#[test]
fn a_voicegroup_entry_of_the_wrong_kind_is_refused() {
    // The matching payload, stored as an 8-bit image the runtime's raw()
    // lookup rejects.
    let payload = title_voicegroup_payload();
    let width = u32::try_from(payload.len()).expect("payload fits in u32");
    let entry = image_entry("audio/voicegroup/title".to_owned(), width, 1, 8, payload)
        .expect("valid image entry");
    let err = with_audio_pack_entry(
        "audio-voicegroup-wrong-kind",
        &["audio/song/mus_title"],
        &title_song_payload(0),
        vec![entry],
        |_| {},
        |ctx| audio::locate(ctx, &mut Vec::new()),
    )
    .expect_err("an image-kind voicegroup entry is refused");
    assert!(
        matches!(&err, GenRomProfileError::WrongPackEntryKind { id, .. } if id == "audio/voicegroup/title"),
        "{err:?}"
    );
}

#[test]
fn a_voicegroup_entry_the_plan_never_reaches_is_refused() {
    // The title entry matches the ROM; the extra group is what a stale or
    // wider extraction would leave behind, and the importer never emits it.
    let payload = title_voicegroup_payload();
    let entries = vec![
        pack_format::raw_entry("audio/voicegroup/title".to_owned(), payload.clone()),
        pack_format::raw_entry("audio/voicegroup/stale".to_owned(), payload),
    ];
    let err = with_audio_pack_entry(
        "audio-voicegroup-extra",
        &["audio/song/mus_title"],
        &title_song_payload(0),
        entries,
        |_| {},
        |ctx| audio::locate(ctx, &mut Vec::new()),
    )
    .expect_err("a pack voicegroup outside the plan is refused");
    let GenRomProfileError::StructMismatch { id, reason } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(id, "audio/voicegroup/*");
    assert!(reason.contains("audio/voicegroup/stale"), "{reason}");
}

/// Run `locate` against the audio fixture with `mutate` applied to the ROM
/// and assert it fails closed with `SongBytecodeMismatch` for the song.
fn assert_bytecode_refused(name: &str, mutate: impl FnOnce(&mut Vec<u8>)) {
    with_audio_song_rom(
        name,
        &["audio/song/mus_title"],
        &title_song_payload(0),
        mutate,
        |ctx| {
            let mut report = Vec::new();
            let err = audio::locate(ctx, &mut report).expect_err("bytecode must be compared");
            let GenRomProfileError::SongBytecodeMismatch { id, .. } = &err else {
                panic!("{err:?}");
            };
            assert_eq!(id, "audio/song/mus_title");
        },
    );
}

#[test]
fn a_song_table_player_that_disagrees_with_the_pack_is_refused() {
    with_audio_song_rom(
        "audio-song-player",
        &["audio/song/mus_title"],
        &title_song_payload(0),
        |rom| rom[0x50_000C..0x50_000E].copy_from_slice(&1u16.to_le_bytes()),
        |ctx| {
            let err = audio::locate(ctx, &mut Vec::new()).expect_err("`ms` must be compared");
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, reason }
                    if id == "audio/song/mus_title" && reason.contains("`ms`")),
                "{err:?}"
            );
        },
    );
}

#[test]
fn a_changed_track_byte_is_refused() {
    // Header metadata is untouched; only WAIT 1 becomes WAIT 2.
    assert_bytecode_refused("audio-track-byte", |rom| rom[0x40_0000] = 0x82);
}

#[test]
fn a_missing_track_is_refused() {
    // Point the track at the ROM window's end, where no bytes exist.
    assert_bytecode_refused("audio-track-missing", |rom| {
        rom[0x30_0008..0x30_000C].copy_from_slice(&0x0900_0000u32.to_le_bytes());
    });
}
