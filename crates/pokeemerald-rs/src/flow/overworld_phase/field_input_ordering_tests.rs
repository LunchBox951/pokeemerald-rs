//! Which owner takes a frame's input: open dialog, START menu, interaction, or sight approach.

use super::step::InteractionOutcome;
use super::test_support::*;
use super::SyntheticStartMenu;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// Headless counterpart to the real-pack acceptance test: while a dialog is
/// open, [`OverworldPhase::step`] must not feed movement to the player at
/// all (module docs' "NPC dialog routing" section -- upstream's `lock`,
/// which stops `RunFieldInput` being polled while a message box owns
/// input). Dropping that early return lets the held direction through and
/// fails here, with no local pack needed.
#[test]
fn an_open_dialog_freezes_movement_until_it_closes() {
    use engine::text::Token;

    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    // Facing south already, so an un-frozen held Down would step
    // immediately -- no turn-in-place frame to absorb it. (7, 5), the tile
    // below, is clear of visible object events, so this test measures the
    // dialog freeze and nothing else -- unlike (4, 5), which a Vigoroth now
    // occupies solidly; see [`ONE_F`]'s docs.
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.player.position(),
            (7, 4),
            "movement must be frozen while a dialog is open"
        );
        assert!(!phase.player.in_transit(), "no step may even have started");
    }
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "and no turn either"
    );
    assert!(phase.dialog.is_some(), "the dialog must still be open");

    // Confirm through the trailing prompt (same held-A-across-the-window
    // reasoning as this module's real-pack dialog test) and let it close.
    let mut closed = false;
    for _ in 0..40 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "confirming must close the synthetic dialog");

    // Control returns: the very next held Down steps.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (7, 5),
        "ordinary movement must resume once the box has closed"
    );
}

/// The field lock ends an in-flight turn the moment it engages
/// (`PlayerFreeze`, `field_player_avatar.c:1039-1046`), so a dialog must not freeze it.
#[test]
fn a_dialog_opened_inside_a_turns_busy_window_must_not_swallow_input_after_it_closes() {
    use engine::text::Token;

    // One tile east of Mom (module docs' `ONE_F` fixture notes), facing
    // South so a held Left is a turn, not a step.
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::South), None);

    phase.step(held(Buttons::LEFT));
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "the held direction must turn the player in place, starting the busy window"
    );
    assert_eq!(phase.player.position(), (3, 6), "a turn must not move");

    // Reachability: the next frame's A press really does find Mom while
    // that window is still draining.
    {
        let runtime = runtime_for(&phase);
        assert!(
            matches!(
                phase.interaction_tokens_this_frame(pressed(Buttons::A), &runtime),
                Some(InteractionOutcome::Dialog(_))
            ),
            "an A press one frame into the turn's busy window must still interact with Mom"
        );
    }

    // The box the A press opens, in this pack-less suite's headless stand-in
    // form.
    phase.dialog = Some(crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]));

    // It owns far more frames than the eight-frame window is long.
    for _ in 0..20 {
        phase.step(held(Buttons::LEFT));
    }
    assert!(phase.dialog.is_some(), "the box must still be open");
    let mut closed = false;
    for _ in 0..40 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed, "confirming must close the synthetic dialog");

    // The first field frame after the box closes must act on the held
    // direction, not sit in a window frozen before the box opened.
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.facing(),
        Direction::East,
        "the turn's busy window must not survive the message box that opened inside it"
    );
}

/// Issue #908 regression: a same-frame `A`-plus-`START` press must resolve
/// this port's counterpart to `TryStartInteractionScript`
/// (`field_control_avatar.c:172`) before `pressedStartButton`
/// (`:182-187`) ever gets a look, because upstream's own
/// `ProcessPlayerFieldInput` checks the interaction branch first and
/// returns `TRUE` out of it before the `START` branch is even reached.
///
/// Stands the player where
/// [`super::step_tests::a_pressed_mid_step_is_discarded_and_the_same_press_at_rest_interacts`]
/// settles -- one tile east of Mom, at rest, already facing her -- so the
/// same real, recognized interaction backs `field_input_claimed` here.
/// Asserted at [`OverworldPhase::start_menu_may_open`] directly, the same
/// decision [`OverworldPhase::step`]'s own "Field start menu ordering"
/// section feeds from a real
/// [`OverworldPhase::interaction_tokens_this_frame`] lookup every frame;
/// [`step_lets_a_same_frame_npc_interaction_beat_a_menu_that_would_really_open`]
/// is this same claim driven through `step` itself, with a menu that
/// genuinely builds.
#[test]
fn start_does_not_preempt_a_same_frame_npc_interaction() {
    let phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    let buttons = pressed(Buttons::A | Buttons::START);

    let interaction_found = {
        let runtime = runtime_for(&phase);
        phase
            .interaction_tokens_this_frame(buttons, &runtime)
            .is_some()
    };
    assert!(
        interaction_found,
        "the fixture must face an NPC whose script this port recognizes"
    );

    assert!(
        !phase.start_menu_may_open(buttons, interaction_found),
        "a same-frame interaction must refuse a fresh START the same frame \
         (field_control_avatar.c:172 returns TRUE before :182)"
    );
    // Positive control: the same fixture, told nothing else claimed the
    // frame, is where a fresh START normally works -- so the refusal above
    // is really the interaction claim, not some other gate this fixture
    // happens to fail.
    assert!(
        phase.start_menu_may_open(buttons, false),
        "the fixture must otherwise be a frame START can open"
    );
}

/// A same-frame `START` press whose menu fails to build must not cost that
/// frame's movement ([`OverworldPhase::build_start_menu`]'s own doc comment
/// on why the menu is built ahead of movement).
#[test]
fn a_failed_pack_load_on_start_does_not_cost_the_frames_movement() {
    let mut phase = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    phase.synthetic_start_menu = SyntheticStartMenu::Fails;

    // Already facing west (module docs' `ONE_F` fixture notes): holding
    // Left begins a step immediately, no separate turn frame first.
    let mut buttons = ButtonState::new();
    buttons.update(Buttons::LEFT);
    buttons.update(Buttons::LEFT | Buttons::START);
    phase.step(buttons);

    assert!(
        phase.start_menu().is_none(),
        "a failed build must leave no menu open"
    );
    assert!(
        phase.player.in_transit(),
        "a failed pack load must leave START exactly as inert as a refused \
         gate -- the held-direction step must still have started"
    );
}

/// Issue #908, end to end: the ordering
/// [`start_does_not_preempt_a_same_frame_npc_interaction`] pins at the gate
/// directly, driven through the real [`OverworldPhase::step`] instead, with
/// [`OverworldPhase::synthetic_start_menu`] standing in for a real
/// pack load so a menu can genuinely open in a test.
///
/// Three same-frame outcomes, one fixture: `A`+`START` next to Mom leaves
/// `START` inert; `START` alone opens a menu from inside `step` itself;
/// and an opening menu preempts that frame's movement, the mirror of
/// [`a_failed_pack_load_on_start_does_not_cost_the_frames_movement`].
#[test]
fn step_lets_a_same_frame_npc_interaction_beat_a_menu_that_would_really_open() {
    let mut with_interaction = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    with_interaction.synthetic_start_menu = SyntheticStartMenu::Builds;
    with_interaction.step(pressed(Buttons::A | Buttons::START));
    assert!(
        with_interaction.start_menu().is_none(),
        "a same-frame interaction must claim the frame ahead of a fresh \
         START, even when the menu would really have built"
    );

    let mut alone = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    alone.synthetic_start_menu = SyntheticStartMenu::Builds;
    alone.step(pressed(Buttons::START));
    assert!(
        alone.start_menu().is_some(),
        "with nothing else claiming the frame, START must open the menu \
         from inside step itself"
    );

    let mut walking = synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None);
    walking.synthetic_start_menu = SyntheticStartMenu::Builds;
    let mut buttons = ButtonState::new();
    buttons.update(Buttons::LEFT);
    buttons.update(Buttons::LEFT | Buttons::START);
    walking.step(buttons);
    assert!(walking.start_menu().is_some(), "the menu must have opened");
    assert!(
        !walking.player.in_transit() && walking.player.position() == (4, 6),
        "upstream never calls PlayerStep on a frame ProcessPlayerFieldInput \
         claims -- no step may have begun either"
    );
}

/// Issue #436, end to end: an already-owning sight-trainer approach must
/// keep outranking a fresh `START` driven through
/// [`OverworldPhase::step`] itself, with the same injected build as above
/// so the menu really would have opened. The *trigger* frame is
/// `sight_trainer_tests::start_does_not_preempt_the_sight_trainer_scan_on_its_trigger_frame`.
#[test]
fn step_keeps_an_owning_sight_trainer_approach_ahead_of_a_fresh_start() {
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;
    phase.begin_synthetic_sight_approach_for_test();

    phase.step(pressed(Buttons::START));
    assert!(
        phase.start_menu().is_none(),
        "the approach owns the frame ahead of pressedStartButton \
         (field_control_avatar.c:182), even when the menu would have built"
    );
}

/// The same ordering as
/// [`step_lets_a_same_frame_npc_interaction_beat_a_menu_that_would_really_open`],
/// driven through [`crate::flow::advance_scene`]'s dispatch rather than
/// [`OverworldPhase::step`] directly.
#[test]
fn advance_scene_lets_a_same_frame_npc_interaction_beat_a_fresh_start() {
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;
    let mut slot = crate::game_save::SaveSlot::disabled();

    let (next, _frame) = crate::flow::advance_scene(
        crate::flow::AppScene::Overworld(Box::new(phase)),
        pressed(Buttons::A | Buttons::START),
        &mut slot,
        crate::pack_source::PackSource::Runtime,
    );

    let crate::flow::AppScene::Overworld(phase) = next else {
        panic!("a START press must leave the overworld in place");
    };
    assert!(
        phase.start_menu().is_none(),
        "the dispatch must weigh a fresh START inside step, behind the \
         same-frame interaction -- not open the menu ahead of it"
    );
}
