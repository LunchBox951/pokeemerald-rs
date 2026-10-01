use super::fixture::*;
use super::*;

#[test]
fn rhythm_pan_shifts_the_stereo_split_on_construction_and_reruns() {
    let half_track_volume = 0x40;
    let rightward_pan = 63;
    let centred_note = TestNote {
        track_right: half_track_volume,
        track_left: half_track_volume,
        ..TestNote::default()
    };
    let panned_note = TestNote {
        rhythm_pan: rightward_pan,
        ..centred_note
    };
    let centred = noise_voice(CgbAdsr::flat(), WIDE_NOISE, centred_note);
    let panned = noise_voice(CgbAdsr::flat(), WIDE_NOISE, panned_note);
    assert!(
        panned.routing.right > centred.routing.right,
        "a positive rhythm pan should raise the right-channel base volume"
    );

    let mut voice = noise_voice(CgbAdsr::flat(), WIDE_NOISE, panned_note);
    voice.set_track_volume(half_track_volume, half_track_volume);
    assert!(
        voice.routing.right > voice.routing.left,
        "the rhythm-pan override must survive a mid-note VOL rerun"
    );
}

/// Sum of absolute per-side render output across one `begin_frame`/`render` pass.
fn frame_side_energy(voice: &mut CgbVoice) -> (i32, i32) {
    voice.begin_frame(false);
    let mut acc = vec![(0i32, 0i32); 64];
    voice.render(&mut acc, &[]);
    acc.iter()
        .fold((0, 0), |(left_sum, right_sum), &(left, right)| {
            (left_sum + left.abs(), right_sum + right.abs())
        })
}

#[test]
fn a_live_volume_write_keeps_the_latched_pan_for_that_frame() {
    // `chan->pan` is written only by `CgbPan`/`CgbModVol`, which run at
    // note-on and at an envelope transition whose counter reached zero
    // (`m4a.c:878-923`, `:994`, `:1077-1085`). A live `MPT_FLG_VOLCHG`
    // rewrites only `rightVolume`/`leftVolume` and raises
    // `CGB_CHANNEL_MO_VOL` (`m4a_1.s:1391-1400`, `:1508-1536`), so the
    // NR51 write it triggers still carries the previously latched pan
    // (`m4a.c:1205-1208`): a hard-pan flip cannot reroute a sustained
    // note on the frame of the write.
    let mut voice = square_voice(CgbChannelNumber::Square1, None, TestNote::default());
    let (centred_left, centred_right) = frame_side_energy(&mut voice);
    assert!(
        centred_left > 0 && centred_right > 0,
        "sanity: a centred note must latch a both-sides pan at note-on"
    );

    voice.set_track_volume(FULL_TRACK_VOLUME, 0);
    let (left, right) = frame_side_energy(&mut voice);

    assert!(
        left > 0 && right > 0,
        "the volume write must reuse the latched both-sides pan, leaving both sides audible"
    );
}

#[test]
fn commit_pan_reroutes_from_the_latest_volumes_once_called() {
    // Companion to `a_live_volume_write_keeps_the_latched_pan_for_that_frame`:
    // the deferred route is not stuck forever. `update_volumes` alone
    // (the live-write path) must not move it, but `commit_pan` (the
    // `CgbModVol`/`CgbPan` boundary, `m4a.c:878-923`) must reroute it
    // from whatever side volumes are current at that point, not just the
    // ones in effect when it was last called.
    let mut routing = StereoRouting::new(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME, FULL_VELOCITY, 0);
    assert!(
        routing.right_enabled && routing.left_enabled,
        "sanity: a centred note latches both sides at construction"
    );

    routing.update_volumes(FULL_TRACK_VOLUME, 0);
    assert!(
        routing.right_enabled && routing.left_enabled,
        "update_volumes alone, matching ChnVolSetAsm, must not move the committed route"
    );

    routing.recompute_pan();
    assert!(
        routing.right_enabled && routing.left_enabled,
        "recompute_pan alone, matching CgbPan's write to chan->pan, must not move the \
         rendered route either"
    );

    routing.commit_pan();
    assert!(
        routing.right_enabled && !routing.left_enabled,
        "commit_pan must reroute to the hard-right pan the latest recompute resolved to"
    );
}

/// `frame_side_energy` on a fresh voice until its stereo route reroutes
/// hard right, or `None` past `max_frames` (a stuck route).
fn frames_until_hard_right(voice: &mut CgbVoice, max_frames: usize) -> Option<usize> {
    (0..max_frames).find(|_| {
        let (left, right) = frame_side_energy(voice);
        left == 0 && right > 0
    })
}

#[test]
fn a_pending_pan_change_commits_by_the_next_sustain_refresh_on_wave() {
    // Wave alone raises `CGB_CHANNEL_MO_VOL` at every bare
    // `envelopeCounter == 0` boundary, not only a retrigger-worthy one
    // (`m4a.c:1081-1082`), so its rendered route must catch up with no
    // second live write -- unlike square/noise
    // (`a_sustained_square_holds_its_route_until_the_next_modify_volume_write`).
    let mut voice = wave_voice(false, TestNote::default());
    let (centred_left, centred_right) = frame_side_energy(&mut voice); // enters sustain
    assert!(
        centred_left > 0 && centred_right > 0,
        "sanity: a centred note must latch a both-sides pan at note-on"
    );

    voice.set_track_volume(FULL_TRACK_VOLUME, 0);
    let (still_left, still_right) = frame_side_energy(&mut voice);
    assert!(
        still_left > 0 && still_right > 0,
        "sanity: the write's own frame still holds the latched pan"
    );

    assert!(
        frames_until_hard_right(&mut voice, 8).is_some(),
        "the pending hard-right pan must commit by the next sustain refresh, not stay \
         latched forever"
    );
}

#[test]
fn an_equal_goal_hard_pan_reroute_still_commits_on_wave_at_a_sustain_refresh() {
    // A hard-right and a hard-left route resolve to the *same* envelope
    // goal of 15 (`cgb_envelope_goal`, `cgb_envelope.rs`), so a flat
    // sustain refresh reassigns the volume it already holds and reports
    // no transition. Upstream still recomputes `chan->pan` on that
    // boundary regardless of whether the volume moved (`m4a.c:1077-1085`),
    // and Wave commits that recompute to NR51 every such boundary
    // (`:1081-1082`), so the pending hard-left route must not stay
    // latched forever even though the goal never changes.
    let mut voice = wave_voice(
        false,
        TestNote {
            track_right: FULL_TRACK_VOLUME,
            track_left: 0,
            ..TestNote::default()
        },
    );
    let (left, right) = frame_side_energy(&mut voice); // enters sustain
    assert!(
        left == 0 && right > 0,
        "sanity: a hard-right note must latch a right-only pan at note-on"
    );
    let sustain_volume = voice.envelope_volume();

    voice.set_track_volume(0, FULL_TRACK_VOLUME);
    assert_eq!(
        voice.envelope_volume(),
        sustain_volume,
        "sanity: the reroute must leave the envelope numerically still"
    );

    assert!(
        (0..16).any(|_| {
            let (left, right) = frame_side_energy(&mut voice);
            left > 0 && right == 0
        }),
        "the pending hard-left pan must commit at a sustain refresh even though the \
         envelope goal is unchanged"
    );
}

#[test]
fn a_sustained_square_holds_its_route_until_the_next_modify_volume_write() {
    // A bare sustain refresh recomputes `chan->pan` through `CgbModVol`
    // but, for square and noise, raises no `CGB_CHANNEL_MO_VOL`
    // (`m4a.c:1077-1085`), and NR51 -- the audible stereo route -- is
    // rewritten only under that bit (`:1205-1208`). A sustained square
    // whose track pan flipped hard right therefore keeps playing both
    // sides until an actual volume write carries the recomputed pan to
    // NR51.
    let mut voice = square_voice(CgbChannelNumber::Square1, None, TestNote::default());
    let (left, right) = frame_side_energy(&mut voice);
    assert!(
        left > 0 && right > 0,
        "sanity: a centred note latches a both-sides pan at note-on"
    );

    // `ChnVolSetAsm`: new side volumes and a raised MO_VOL, with
    // `chan->pan` untouched (`m4a_1.s:1508-1536`).
    voice.set_track_volume(FULL_TRACK_VOLUME, 0);
    // Two sustain refresh periods (7 frames each, `m4a.c:1128`).
    for frame in 0..14 {
        let (left, right) = frame_side_energy(&mut voice);
        assert!(
            left > 0 && right > 0,
            "frame {frame} after the write: a square's route must not move at a bare \
             sustain refresh, which writes no NR51"
        );
    }

    // The next live volume write raises MO_VOL, which finally carries
    // the pan `CgbModVol` already recomputed to NR51.
    voice.set_track_volume(FULL_TRACK_VOLUME, 0);
    let (left, right) = frame_side_energy(&mut voice);
    assert!(
        left == 0 && right > 0,
        "the recomputed hard-right pan must reach the route at the next volume write"
    );
}
