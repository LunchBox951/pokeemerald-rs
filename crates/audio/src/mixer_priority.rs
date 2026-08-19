//! Tests for [`Mixer`]'s `ply_note` channel-allocation search: reuse a free
//! or already-released channel, steal the weakest voice the newcomer
//! outranks, or refuse the note. See [`crate::mixer`]'s module docs for the
//! rule-by-rule reading of `m4a_1.s:1647`..`:1718` these pin.

use std::sync::Arc;

use super::Mixer;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::{CgbChannelNumber, CgbVoice};
use crate::envelope::Adsr;
use crate::mixer::DEFAULT_MASTER_VOLUME;
use crate::pitch::{self, SAMPLES_PER_FRAME};
use crate::sample::WaveData;
use crate::voice::Voice;

fn unity_freq() -> u32 {
    (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
}

/// A tied (gate `0`) voice on `track` with the given effective priority; its
/// MIDI key doubles as an identity tag the assertions read back.
fn voice(track: usize, priority: u8, key: u8) -> Voice {
    let wave = Arc::new(WaveData::one_shot(0, vec![50; SAMPLES_PER_FRAME + 4]));
    Voice::new(
        wave,
        Adsr::flat(),
        unity_freq(),
        0xFF,
        0xFF,
        127,
        0,
        key,
        track,
        0,
        0,
    )
    .with_priority(priority)
}

fn cgb_voice(track: usize, priority: u8, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        None,
        CgbAdsr::flat(),
        key,
        0,
        0xFF,
        0xFF,
        127,
        0,
        key,
        track,
        0,
        0,
        0,
    )
    .with_priority(priority)
}

/// A mixer whose every slot `voices` already fills, so the next note-on has
/// to reuse, steal, or be refused.
fn full_mixer(voices: Vec<Voice>) -> Mixer {
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, voices.len());
    for voice in voices {
        assert!(mixer.add_voice(voice), "setup voices must all fit");
    }
    mixer
}

/// The MIDI keys of the live DirectSound voices, in slot order.
fn live_keys(mixer: &Mixer) -> Vec<u8> {
    mixer.voices().into_iter().map(Voice::midi_key).collect()
}

/// The voice occupying pool slot `slot`.
fn slot(mixer: &Mixer, slot: usize) -> &Voice {
    mixer.voices[slot].as_ref().expect("slot must be occupied")
}

/// Note-off the voice occupying pool slot `slot` (upstream's `SF_STOP`).
fn release_slot(mixer: &mut Mixer, slot: usize) {
    mixer.voices[slot]
        .as_mut()
        .expect("slot must be occupied")
        .note_off();
}

/// Release the voice in pool slot `slot`, then render one frame so it retires
/// and the slot goes free — a voice ending mid-song.
fn end_slot(mixer: &mut Mixer, slot: usize) {
    release_slot(mixer, slot);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
}

#[test]
fn a_free_channel_is_used_before_any_steal() {
    // Rule 1 (`m4a_1.s:1678`..`:1681`): below the cap nothing is displaced,
    // even by a note that outranks everything sounding.
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 3);
    assert!(mixer.add_voice(voice(0, 200, 60)));
    assert!(mixer.add_voice(voice(1, 1, 61)));
    assert!(mixer.add_voice(voice(2, 250, 62)));
    assert_eq!(live_keys(&mixer), vec![60, 61, 62]);
}

#[test]
fn a_higher_priority_note_steals_the_weakest_voice() {
    // Rule 3 (`m4a_1.s:1694`..`:1700`): strictly lower priority loses, and
    // the scan keeps the lowest it saw -- key 61 at priority 10, not key 60.
    let mut mixer = full_mixer(vec![voice(0, 40, 60), voice(1, 10, 61), voice(2, 90, 62)]);
    assert!(mixer.add_voice(voice(3, 100, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70, 62]);
}

#[test]
fn a_lower_priority_note_is_refused() {
    // Rule 5 (`m4a_1.s:1716`..`:1718`): the candidate seeds with the new
    // note's own priority, so when every voice outranks it nothing is
    // selected and the note never sounds.
    let mut mixer = full_mixer(vec![voice(0, 40, 60), voice(1, 50, 61)]);
    assert!(!mixer.add_voice(voice(2, 20, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 61]);
}

#[test]
fn an_equal_priority_note_steals_the_latest_track() {
    // Rule 4 (`m4a_1.s:1703`..`:1707`): among equal priorities the candidate
    // tracks the *highest* track index seen, so track 3 loses to the new
    // note on track 1 while track 0 is untouchable.
    let mut mixer = full_mixer(vec![voice(0, 60, 60), voice(3, 60, 61), voice(2, 60, 62)]);
    assert!(mixer.add_voice(voice(1, 60, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70, 62]);
}

#[test]
fn an_equal_priority_note_cannot_displace_an_earlier_track() {
    // Rule 4's skip arm (`m4a_1.s:1708`..`:1709`): every equal-priority voice
    // belongs to a track before the newcomer's, so the note is refused.
    let mut mixer = full_mixer(vec![voice(0, 60, 60), voice(1, 60, 61)]);
    assert!(!mixer.add_voice(voice(2, 60, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 61]);
}

#[test]
fn an_equal_priority_note_displaces_its_own_track() {
    // The equal-track arm (`m4a_1.s:1710`..`:1711`) is a *take*, not a skip:
    // seeded with the newcomer's own track, a voice on that same track is a
    // valid victim -- a track cannibalises itself before it is refused.
    let mut mixer = full_mixer(vec![voice(0, 60, 60), voice(2, 60, 61)]);
    assert!(mixer.add_voice(voice(2, 60, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn a_retired_slot_is_refilled_in_place() {
    // The pool must keep upstream's slot order once a voice ends mid-song,
    // because rule 4's equal-track arm is a *take*: the victim of a full
    // `(priority, track)` tie is the LAST channel in slot order
    // (`m4a_1.s:1708`..`:1711`).
    //
    // Three chord notes share track 3 and priority 60 across slots 0..2. The
    // middle one ends; the freed slot is the lowest free one, so the next
    // note-on refills slot 1 rather than landing after slot 2
    // (`m4a_1.s:1678`..`:1681`). A fifth note tying with all three then cuts
    // slot 2 -- key 64, the note that was never displaced -- and the chord
    // that survives is keys 60/66/68. A pool that compacted on retire would
    // have ordered the newcomer last and cut key 66 instead.
    let mut mixer = full_mixer(vec![voice(3, 60, 60), voice(3, 60, 62), voice(3, 60, 64)]);
    end_slot(&mut mixer, 1);
    assert!(mixer.voices[1].is_none(), "the middle slot must go free");
    assert_eq!(live_keys(&mixer), vec![60, 64]);

    assert!(mixer.add_voice(voice(3, 60, 66)));
    assert_eq!(
        slot(&mixer, 1).midi_key(),
        66,
        "the freed slot is the lowest free one, so it is refilled first",
    );
    assert_eq!(live_keys(&mixer), vec![60, 66, 64]);

    assert!(mixer.add_voice(voice(3, 60, 68)));
    assert_eq!(live_keys(&mixer), vec![60, 66, 68]);
}

#[test]
fn a_released_voice_is_reused_before_any_sounding_one() {
    // Rule 2 (`m4a_1.s:1682`..`:1692`): a stopped channel is taken
    // unconditionally, priority ignored -- here it outranks both the
    // newcomer and the far weaker still-sounding voice beside it.
    let mut mixer = full_mixer(vec![voice(0, 1, 60), voice(1, 255, 61)]);
    release_slot(&mut mixer, 1);
    assert!(slot(&mixer, 1).is_stopping());
    assert!(mixer.add_voice(voice(3, 2, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn a_released_voice_makes_the_note_unrefusable() {
    // The same rule from the other side: a note that every sounding voice
    // outranks still starts, because the released channel is compared on
    // neither priority nor track.
    let mut mixer = full_mixer(vec![voice(0, 200, 60), voice(0, 200, 61)]);
    release_slot(&mut mixer, 0);
    assert!(mixer.add_voice(voice(5, 1, 70)));
    assert_eq!(live_keys(&mixer), vec![70, 61]);
}

#[test]
fn released_voices_compete_among_themselves_on_priority() {
    // Once one released channel is the candidate, later released channels
    // still go through the ordinary priority comparison
    // (`m4a_1.s:1689`..`:1700`), so the weaker of the two is the victim.
    let mut mixer = full_mixer(vec![voice(0, 90, 60), voice(1, 30, 61)]);
    release_slot(&mut mixer, 0);
    release_slot(&mut mixer, 1);
    assert!(mixer.add_voice(voice(2, 50, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn a_saturated_priority_note_outranks_every_unsaturated_one() {
    // `0xFF` is the ceiling `ply_note` clamps to (`m4a_1.s:1628`..`:1633`),
    // so a saturated note beats anything below it whatever the track order --
    // here it starts despite being on the last track of all. Once the first
    // weaker voice becomes the candidate the tie-break compares against
    // *its* track, not the newcomer's, so the later-track member of the
    // equally-weak pair is the one displaced.
    let mut mixer = full_mixer(vec![voice(0, 254, 60), voice(1, 254, 61)]);
    assert!(mixer.add_voice(voice(9, 255, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn saturation_collapses_two_different_sums_into_a_track_order_tie() {
    // Two notes whose song+track sums both clamp to `0xFF` are exactly equal
    // afterwards, so the track tie-break -- not the pre-clamp magnitude --
    // decides. The incumbent on the later track loses even though its
    // unsaturated sum would have been larger.
    let mut mixer = full_mixer(vec![voice(0, 255, 60), voice(4, 255, 61)]);
    assert!(mixer.add_voice(voice(2, 255, 70)));
    assert_eq!(live_keys(&mixer), vec![60, 70]);
}

#[test]
fn cgb_channel_keeps_a_higher_priority_occupant() {
    // The CGB arm's refuse path: an occupant of strictly higher priority
    // falls straight through to the refuse label (`m4a_1.s:1658`..`:1663`).
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, 100, 60)));
    assert!(!mixer.add_cgb_voice(cgb_voice(0, 99, 70)));
    let slot = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("occupant kept");
    assert_eq!(slot.midi_key(), 60);
}

#[test]
fn cgb_channel_yields_to_a_higher_priority_note() {
    // The `bcc` steal path (`m4a_1.s:1658`..`:1661`).
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, 100, 60)));
    assert!(mixer.add_cgb_voice(cgb_voice(3, 101, 70)));
    let slot = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("replacement present");
    assert_eq!(slot.midi_key(), 70);
}

#[test]
fn cgb_equal_priority_resolves_on_track_order() {
    // `m4a_1.s:1665`..`:1667`: an equal-priority newcomer takes the channel
    // only when the occupant's track is not earlier than its own.
    let mut refused = Mixer::default();
    assert!(refused.add_cgb_voice(cgb_voice(1, 100, 60)));
    assert!(!refused.add_cgb_voice(cgb_voice(2, 100, 70)));

    let mut stolen = Mixer::default();
    assert!(stolen.add_cgb_voice(cgb_voice(2, 100, 60)));
    assert!(stolen.add_cgb_voice(cgb_voice(1, 100, 70)));

    let mut same_track = Mixer::default();
    assert!(same_track.add_cgb_voice(cgb_voice(2, 100, 60)));
    assert!(same_track.add_cgb_voice(cgb_voice(2, 100, 70)));
}

#[test]
fn cgb_channel_always_reuses_a_released_occupant() {
    // `m4a_1.s:1655`..`:1657`: a stopped occupant short-circuits the whole
    // priority test, exactly as in the DirectSound scan.
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, 255, 60)));
    mixer.cgb_voices[CgbChannelNumber::Square1.slot()]
        .as_mut()
        .expect("occupant present")
        .note_off();
    assert!(mixer.add_cgb_voice(cgb_voice(9, 0, 70)));
    let slot = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("replacement present");
    assert_eq!(slot.midi_key(), 70);
}

#[test]
fn a_cgb_note_only_contends_for_its_own_hardware_channel() {
    // Each CGB voice owns exactly one fixed slot, so a square-2 note never
    // competes with the square-1 occupant however they rank.
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_voice(0, 255, 60)));
    let square2 = CgbVoice::square(
        CgbChannelNumber::Square2,
        2,
        None,
        CgbAdsr::flat(),
        70,
        0,
        0xFF,
        0xFF,
        127,
        0,
        70,
        9,
        0,
        0,
        0,
    )
    .with_priority(0);
    assert!(mixer.add_cgb_voice(square2));
    assert_eq!(mixer.voice_count(), 2);
}
