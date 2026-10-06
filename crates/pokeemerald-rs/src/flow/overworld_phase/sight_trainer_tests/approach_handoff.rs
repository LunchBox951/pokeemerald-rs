//! The approach cutscene's intro speech and the handoff into the battle.

use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use super::super::test_support::pressed;
use super::support::*;

/// `EventScript_ShowTrainerIntroMsg` (`trainer_battle.inc:101-107`): the
/// battle waits for the intro speech, the speech waits for the player, and
/// only when the box closes does `dotrainerbattle` run -- taking the party
/// lead and keying the fight to the real sight trainer.
///
/// Driven against a synthetic message box (`skip_to_open_intro_message`'s own
/// docs) so the handshake is pinned without an extracted pack -- built the
/// exact way the production path builds it since issue #410: no trailing
/// `{P}`, and the script's `waitbuttonpress` opted into on the dialog
/// (`NpcDialog::open_at_speed` applies it for the real
/// `advance_intro_message`).
#[test]
fn the_intro_speech_holds_the_battle_until_the_player_dismisses_it() {
    use engine::text::Token;

    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);
    phase
        .sight_approach
        .as_mut()
        .expect("just seeded")
        .skip_to_open_intro_message();
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![
            Token::Char('W'),
            Token::Char('h'),
            Token::Char('o'),
            Token::End,
        ])
        .with_waitbuttonpress(),
    );

    // Spend a couple of immune steps before the handoff so the reset
    // assertion below is genuinely witnessed: `immunity_steps() == 0` is
    // also this counter's untouched default, so proving the restart call
    // actually ran means starting from something other than zero.
    phase
        .wild
        .check_standard_wild_encounter(0, None, &mut phase.rng);
    phase
        .wild
        .check_standard_wild_encounter(0, None, &mut phase.rng);
    assert_eq!(
        phase.wild.immunity_steps(),
        2,
        "setup: the counter must be nonzero before the handoff, or the reset assertion \
         below cannot distinguish a real restart from the default"
    );

    // No button: the box prints and waits, and nothing hands off.
    for frame in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
        assert!(
            phase.dialog.is_some(),
            "frame {frame}: an undismissed intro box stays open"
        );
        assert!(
            !phase.is_sight_trainer_battle_active(),
            "frame {frame}: `dotrainerbattle` must not run until the speech is dismissed"
        );
        assert!(
            phase.party_lead.is_some(),
            "frame {frame}: and the lead is still the player's"
        );
    }

    // Issue #410: once printed, the speech stays fully on screen for every
    // one of those waiting frames. The synthetic trailing `{P}` this stage
    // used to carry cleared the box on the confirm instead and then drained
    // a post-clear reveal delay, so the player watched an empty box for
    // several frames before `dotrainerbattle` -- and, since the script-level
    // wait then demanded a *second* fresh confirm edge on top of the `{P}`'s
    // own, sat on that empty box indefinitely under a held button.
    // `FRAMES_STANDING_STILL` is far past the three glyphs' print time.
    let printed = phase
        .dialog
        .as_ref()
        .expect("still open")
        .revealed_glyph_count();
    assert_eq!(
        printed, 3,
        "every glyph of the intro must still be on screen while `waitbuttonpress` waits"
    );

    // `waitbuttonpress`: A closes the box, and the fight starts with it --
    // on that very frame, with the text still whole right up to it.
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_sight_trainer_battle_active(),
        "the confirm edge must hand off to `dotrainerbattle` on its own frame -- no clear, \
         no post-clear reveal delay, exactly as upstream runs `waitbuttonpress` straight \
         into `dotrainerbattle` (`trainer_battle.inc:104-107`)"
    );
    assert!(phase.dialog.is_none(), "the box closed with the handoff");
    assert!(
        phase.sight_approach.is_none(),
        "the approach is over once its battle has started"
    );
    assert!(
        phase.party_lead.is_none(),
        "`dotrainerbattle` is where the lead is finally taken into the fight"
    );
    assert_eq!(
        active_sight_trainer_id(&phase),
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "the fight is keyed to the real sight trainer, for the defeated flag"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "the post-battle wild-encounter immunity window is restarted with the fight (the \
         setup above spent it first, so this zero is the restart call firing, not merely \
         the counter's untouched default), for stream-order parity with \
         `begin_route103_rival_battle`"
    );
}

/// The pack-gated companion to
/// [`the_intro_speech_holds_the_battle_until_the_player_dismisses_it`]: that
/// test only reaches `advance_intro_message`'s real `!opened` arm one frame
/// after [`SightApproach::skip_to_open_intro_message`]'s synthetic
/// shortcut plants the box directly -- so
/// [`OverworldPhase::advance_intro_message`]'s actual
/// `NpcDialog::open_at_speed` call and its `opened` latch also have a
/// synthetic-pack sibling in `step_tests` (issue #1444); its `Err` fallback
/// path (`sight_trainer_approach.rs`'s own module doc comment) does not, and
/// still only runs here. This one drives the real icon, the real
/// zero-tile turn, and the real message box -- open, print every glyph, and
/// dismiss through the script's own `waitbuttonpress` -- against the
/// genuinely extracted
/// pack, the same way `frame_tests`' own
/// `walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog` does
/// for an ordinary NPC. `#[ignore]`d like this crate's other real-pack
/// tests: run `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_intro_message_opens_prints_and_dismisses_for_real() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);

    // Icon, then the trainer's own zero-tile `MOVEMENT_ACTION_FACE_PLAYER`
    // and stop: everything up to (but not including) the frame that opens
    // the box.
    let mut opened = false;
    for frame in 0..120 {
        phase.step(ButtonState::new());
        assert!(
            phase.sight_approach.is_some(),
            "frame {frame}: the approach must not end before the real box has even opened"
        );
        if phase.dialog.is_some() {
            opened = true;
            break;
        }
    }
    assert!(
        opened,
        "the real intro box must open against the extracted pack within a generous budget"
    );
    assert!(
        phase.sight_approach.is_some(),
        "the box only opened -- `dotrainerbattle` has not run yet"
    );
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "opening the box must not itself start the battle"
    );

    // The exact real text this trainer's own `seed_approach` intro carries
    // (module docs' "The stand-in party" section: the object event and the
    // speech are real, only the party behind the battle is a stand-in).
    let expected_tokens =
        crate::authored_message::parse_message("Whoa!\nHow'd you get into a space this small?")
            .expect("this test's own literal intro speech is a valid authored message");
    let expected_glyph_count = expected_tokens
        .iter()
        .filter(|t| matches!(t, engine::text::Token::Char(_)))
        .count();
    assert!(
        expected_glyph_count > 0,
        "the real intro line must contain visible text"
    );

    let mut fully_printed = false;
    for _ in 0..400 {
        phase.step(ButtonState::new());
        let Some(dialog) = &phase.dialog else {
            panic!("the box must not close on its own before `waitbuttonpress` confirms");
        };
        if dialog.revealed_glyph_count() == expected_glyph_count {
            fully_printed = true;
            break;
        }
    }
    assert!(
        fully_printed,
        "every glyph of the real intro line must print within the frame budget"
    );

    // Confirm through the script's `waitbuttonpress`. Issue #410: the box
    // holds every printed glyph right up to the confirm frame and closes on
    // it, so this budget is spent on reaching a fresh edge, not on draining
    // a clear that no longer happens.
    let mut handed_off = false;
    for _ in 0..30 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            handed_off = true;
            break;
        }
    }
    assert!(
        handed_off,
        "confirming `waitbuttonpress` must close the real box"
    );
    assert!(
        phase.is_sight_trainer_battle_active(),
        "`dotrainerbattle` must run the instant the real box closes"
    );
    assert!(
        phase.sight_approach.is_none(),
        "the approach is over once its battle has started"
    );
    assert_eq!(
        active_sight_trainer_id(&phase),
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "the fight is keyed to the real sight trainer"
    );
}

/// An approach in progress preempts the whole rest of the frame, including
/// the sight check that started it: a second cone entry must not stack a
/// second approach on top of the first, and the trainer's own walk must not
/// restart.
#[test]
fn a_running_approach_preempts_the_cone_check_that_started_it() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);

    for frame in 0..EXCLAMATION_ICON_FRAMES {
        phase.step(ButtonState::new());
        assert!(
            phase.sight_approach.is_some(),
            "frame {frame}: still exactly one approach"
        );
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "frame {frame}: a re-fired cone check would have restarted the walk"
        );
    }
    assert!(
        phase.party_lead.is_some(),
        "the trigger never got a second chance to spend the lead"
    );
}
