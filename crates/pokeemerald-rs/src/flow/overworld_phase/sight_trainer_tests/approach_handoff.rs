//! The approach cutscene's intro speech and the handoff into the battle.

use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use super::super::sight_trainer_approach::fixture;
use super::super::test_support::pressed;
use super::super::OverworldPhase;
use super::support::*;
use crate::pack_source::PackSource;
use crate::pack_test_support::ScratchClaim;

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
        .check_standard_wild_encounter(0, None, None, &mut phase.rng);
    phase
        .wild
        .check_standard_wild_encounter(0, None, None, &mut phase.rng);
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
            phase.lead_for_test().is_some(),
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
        phase.lead_for_test().is_none(),
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

/// The first charmap byte of the control codes (`CHAR_PROMPT_SCROLL` and up),
/// and the two this test's speech may hold (`include/characters.h`).
const CONTROL_BYTES_START: u8 = 0xF7;
const CHAR_NEWLINE: u8 = 0xFE;
const EOS: u8 = 0xFF;

/// The pack-gated companion to
/// [`the_intro_speech_holds_the_battle_until_the_player_dismisses_it`]: that
/// test only reaches `advance_intro_message`'s real `!opened` arm one frame
/// after [`SightApproach::skip_to_open_intro_message`]'s synthetic
/// shortcut plants the box directly -- so
/// [`OverworldPhase::advance_intro_message`]'s actual
/// `NpcDialog::open_at_speed` call and its `opened` latch also have a
/// synthetic-pack sibling in `step_tests` (issue #1444); its `Err` fallback
/// path (`sight_trainer_approach`'s own module doc comment) is pinned below
/// by `a_pack_without_the_intro_table_starts_the_battle_without_a_box`. This one drives the real icon, the real
/// zero-tile turn, and the real message box -- open, print every glyph of
/// the speech the pack holds for the trainer's script, and dismiss through
/// the script's own `waitbuttonpress` -- against the genuinely extracted
/// pack, the same way `frame_tests`' own
/// `walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog` does
/// for an ordinary NPC. `#[ignore]`d like this crate's other real-pack
/// tests: run `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_intro_message_opens_prints_and_dismisses_for_real() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = repo_route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
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

    // What the box must print comes from the pack this test opened it from,
    // read here a second time and counted by hand from the raw charmap bytes
    // -- not from `intro_tokens`' own decode, and not from any committed text.
    // Rhett's speech holds only printable glyphs and line breaks, which the
    // assertion below pins, so each byte but the line breaks and the
    // terminator is one glyph.
    let pack = assets::AssetPack::load_repo().expect("the checkout's extracted pack loads");
    let table = pack
        .sight_trainer_intro_speeches()
        .expect("the extracted pack carries the sight-trainer intro table");
    let raw = table
        .speech(RHETT_SCRIPT)
        .expect("the table holds Rhett's own speech");
    assert!(
        raw.iter()
            .all(|&byte| byte < CONTROL_BYTES_START || byte == CHAR_NEWLINE || byte == EOS),
        "Rhett's speech is expected to hold nothing but glyphs and line breaks"
    );
    let expected_glyph_count = raw
        .iter()
        .filter(|&&byte| byte != CHAR_NEWLINE && byte != EOS)
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
        phase.lead_for_test().is_some(),
        "the trigger never got a second chance to spend the lead"
    );
}

/// Drives a seeded approach through its intro box to the battle start.
fn hand_off_to_the_battle(phase: &mut OverworldPhase) {
    use engine::text::Token;

    seed_approach(phase, 0);
    phase
        .sight_approach
        .as_mut()
        .expect("just seeded")
        .skip_to_open_intro_message();
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![Token::Char('W'), Token::End])
            .with_waitbuttonpress(),
    );
    for _ in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
    }
    phase.step(pressed(Buttons::A));
    assert!(phase.is_sight_trainer_battle_active(), "setup: handed off");
}

/// The approach only observes the lead; it stays at home to be lent at the
/// handoff, so the owner can still lend it mid-approach.
#[test]
fn a_running_approach_has_not_lent_the_lead() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 3);
    let lent = phase
        .take_lead_battler()
        .expect("an approach in flight leaves the lead with the owner");
    phase.restore_lead_battler(lent);
    phase.assert_lead_at_slot_for_test(0);
}

/// The handoff lends the lead, so a second lend is refused.
#[test]
#[should_panic(expected = "lent out to a battle")]
fn a_started_sight_battle_holds_the_lead_on_loan() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    hand_off_to_the_battle(&mut phase);
    let _ = phase.take_lead_battler();
}

/// The battle returns the lead to its slot through the owner when it ends.
#[test]
fn a_finished_sight_battle_restores_the_lead_to_its_slot() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    hand_off_to_the_battle(&mut phase);
    assert!(phase.lead_for_test().is_none(), "the lead is on loan");

    let outcome = play_out_sight_battle(&mut phase, 32);
    assert!(outcome.is_some(), "the fight must conclude");
    assert!(phase.lead_for_test().is_some(), "the lead came back");
    phase.assert_lead_at_slot_for_test(0);
    let lent = phase.take_lead_battler().expect("the loan was closed");
    phase.restore_lead_battler(lent);
}

/// A scratch pack holding the message-box assets, the font sheets and -- when
/// `with_table` -- the invented intro table, for the production intro path to
/// read through `PackSource::Test`. The guard removes the file on drop.
fn claim_intro_pack(label: &str, with_table: bool) -> (ScratchClaim, PackSource) {
    use crate::pack_test_support::{image_entry, pack_bytes_with_fonts, palette_entry};

    let mut entries = vec![
        image_entry("text-window/image/message_box", 56, 16, 4, 0),
        palette_entry("text-window/palette/message_box", 16),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            2,
            0,
        ),
    ];
    if with_table {
        entries.push(fixture::invented_table_entry());
    }
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-sight-intro-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let claim = ScratchClaim::claim_with(path, pack_bytes_with_fonts(entries));
    // `PackSource::Test` needs a `'static` path; leaked once per call and
    // reclaimed when the test process exits, the idiom `dialog_pacing_tests`
    // uses too.
    let leaked: &'static std::path::Path = Box::leak(claim.path().to_path_buf().into_boxed_path());
    (claim, PackSource::Test(leaked))
}

/// The production intro path prints the pack's entry for the approaching
/// trainer's *script*: Amy and Liv share `TRAINER_AMY_AND_LIV_1` yet each
/// opens with a speech of its own, and Rhett's differs from both.
///
/// Every speech here is invented (`sight_trainer_approach::fixture`), a
/// different glyph count per script, so a lookup keyed by anything but the
/// script -- the shared battle id, a fixed script, a first entry -- prints the
/// wrong number of glyphs for at least one of the three.
#[test]
fn the_intro_box_prints_the_pack_entry_of_the_trainers_own_script() {
    const SHARED_BATTLE_ID_SCRIPTS: [&str; 2] =
        ["Route103_EventScript_Amy", "Route103_EventScript_Liv"];
    const OPEN_FRAME_BUDGET: usize = 200;
    const PRINT_FRAME_BUDGET: usize = 400;
    const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

    assert_ne!(
        fixture::invented_glyph_count(SHARED_BATTLE_ID_SCRIPTS[0]),
        fixture::invented_glyph_count(SHARED_BATTLE_ID_SCRIPTS[1]),
        "setup: the two trainers sharing a battle id must have different invented speeches"
    );
    let (_guard, source) = claim_intro_pack("entries", true);
    let (rx, ry) = RHETT_TILE;
    for script in [
        SHARED_BATTLE_ID_SCRIPTS[0],
        SHARED_BATTLE_ID_SCRIPTS[1],
        RHETT_SCRIPT,
    ] {
        let expected = fixture::invented_glyph_count(script);
        let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
        phase.pack_source = source.clone();
        phase.save2.options_text_speed = OPTIONS_TEXT_SPEED_FAST;
        seed_approach_with_script(&mut phase, script, rhetts_object_event().local_id, None, 0);

        let mut frames = 0;
        while phase.dialog.is_none() {
            frames += 1;
            assert!(
                frames <= OPEN_FRAME_BUDGET,
                "{script}: the intro box must open against the synthetic pack"
            );
            phase.step(ButtonState::new());
        }
        let mut printed = 0;
        for _ in 0..PRINT_FRAME_BUDGET {
            phase.step(ButtonState::new());
            printed = phase
                .dialog
                .as_ref()
                .expect("the box waits for `waitbuttonpress`")
                .revealed_glyph_count();
            if printed >= expected {
                break;
            }
        }
        assert_eq!(
            printed, expected,
            "{script}: its own speech must print whole"
        );
        for _ in 0..FRAMES_STANDING_STILL {
            phase.step(ButtonState::new());
        }
        assert_eq!(
            phase
                .dialog
                .as_ref()
                .expect("still waiting on the player")
                .revealed_glyph_count(),
            expected,
            "{script}: no glyph of another speech may follow"
        );
    }
}

/// A pack that cannot supply the intro -- here, one without the table -- opens
/// no box and starts the battle anyway, the way a missing font already does:
/// the fight is built and paid for, and the player must not be stranded in a
/// cutscene with no way out.
#[test]
fn a_pack_without_the_intro_table_starts_the_battle_without_a_box() {
    const FRAME_BUDGET: usize = 200;

    let (_guard, source) = claim_intro_pack("no-table", false);
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.pack_source = source;
    seed_approach(&mut phase, 0);

    for frame in 0..FRAME_BUDGET {
        phase.step(ButtonState::new());
        assert!(
            phase.dialog.is_none(),
            "frame {frame}: there is no speech to print"
        );
        if phase.is_sight_trainer_battle_active() {
            assert!(phase.sight_approach.is_none());
            return;
        }
    }
    panic!("the battle must start without the box within {FRAME_BUDGET} frames");
}
