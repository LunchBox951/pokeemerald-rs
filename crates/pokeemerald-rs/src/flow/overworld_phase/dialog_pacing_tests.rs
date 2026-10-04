//! Dialog pacing, field-input ownership, and animation behind dialog.

use super::step::InteractionOutcome;
use super::test_support::*;
use super::SyntheticStartMenu;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// A field-dialog interaction repairs an out-of-range saved
/// `optionsTextSpeed` in place on the same frame, as
/// `GetPlayerTextSpeedDelay` does (`pokeemerald/src/menu.c:481-488`).
#[test]
fn a_field_dialog_interaction_repairs_an_out_of_range_saved_text_speed() {
    /// An `optionsTextSpeed` above `OPTIONS_TEXT_SPEED_FAST` (`2`) --
    /// invalid, same as `pokeemerald/include/constants/global.h:127-129`.
    const OUT_OF_RANGE_TEXT_SPEED: u8 = 5;
    /// `OPTIONS_TEXT_SPEED_MID`: what an invalid value repairs to.
    const REPAIRED_MID_TEXT_SPEED: u8 = 1;

    // One tile east of Mom, facing west -- directly adjacent (module docs'
    // `ONE_F` fixture notes, matching this file's other Mom-interaction
    // tests).
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.save2.options_text_speed = OUT_OF_RANGE_TEXT_SPEED;

    phase.step(pressed(Buttons::A));

    assert_eq!(
        phase.save2.options_text_speed, REPAIRED_MID_TEXT_SPEED,
        "an out-of-range saved optionsTextSpeed must be repaired to MID the moment a field \
         dialog interaction reads it, exactly as GetPlayerTextSpeedDelay repairs \
         gSaveBlock2Ptr->optionsTextSpeed in place"
    );
}

/// The complement: a saved `optionsTextSpeed` already in range must survive
/// a field dialog interaction unchanged -- only an invalid value is ever
/// repaired.
#[test]
fn a_field_dialog_interaction_leaves_an_in_range_saved_text_speed_untouched() {
    const FAST_TEXT_SPEED: u8 = 2;

    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.save2.options_text_speed = FAST_TEXT_SPEED;

    phase.step(pressed(Buttons::A));

    assert_eq!(
        phase.save2.options_text_speed, FAST_TEXT_SPEED,
        "a saved optionsTextSpeed already within range must not be rewritten"
    );
}

/// Drives a dialog the production `phase.step` opened from a synthetic
/// on-disk pack ([`crate::pack_source::PackSource::Test`]) and asserts a
/// FAST-saved session reveals its message in fewer frames than a SLOW-saved
/// one (`sTextSpeedFrameDelays`, `pokeemerald/src/menu.c:77-82`).
#[test]
fn a_field_dialog_opened_by_the_real_step_pipeline_paces_at_the_saved_text_speed() {
    use crate::pack_source::PackSource;
    use crate::pack_test_support::{image_entry, pack_bytes, palette_entry};

    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FRAME_BIT_DEPTH: u8 = 4;
    const FONT_BIT_DEPTH: u8 = 2;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    const PRINT_FRAME_BUDGET: usize = 64;
    /// Mom's real script text ([`crate::overworld::npc_scripts::script_text`])
    /// starts well past the third glyph -- plenty of room for FAST/SLOW to
    /// diverge before either dialog panics on a missing font glyph.
    const GLYPH_TARGET: usize = 3;
    const OPTIONS_TEXT_SPEED_SLOW: u8 = 0;
    const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

    let pack_bytes_blob = pack_bytes(vec![
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ]);

    let frames_to_reveal_third_glyph = |saved_text_speed: u8| {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-step-field-dialog-{saved_text_speed}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, &pack_bytes_blob).expect("the scratch directory is writable");
        // `PackSource::Test` needs a `'static` path; leaked once per call,
        // reclaimed only when the test process exits (test-only, matches
        // this crate's own `Box::leak`-for-`'static`-fixtures idiom).
        let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());

        // One tile east of Mom, facing west -- directly adjacent (this
        // file's other Mom-interaction tests' own fixture notes).
        let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
        phase.pack_source = PackSource::Test(leaked_path);
        phase.save2.options_text_speed = saved_text_speed;

        phase.step(pressed(Buttons::A));
        assert!(
            phase.dialog.is_some(),
            "the synthetic pack must let the real production path open Mom's dialog"
        );

        let mut frame = 0;
        loop {
            frame += 1;
            assert!(
                frame <= PRINT_FRAME_BUDGET,
                "the third glyph must reveal within the frame budget"
            );
            phase.step(ButtonState::new());
            let dialog = phase
                .dialog
                .as_ref()
                .expect("must still be printing well before waitbuttonpress");
            if dialog.revealed_glyph_count() >= GLYPH_TARGET {
                break;
            }
        }

        let _ = std::fs::remove_file(&path);
        frame
    };

    let fast = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_FAST);
    let slow = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_SLOW);

    assert!(
        fast < slow,
        "the dialog the real step pipeline opens for a FAST-saved session must reveal its \
         third glyph in fewer frames than a SLOW-saved one, but they took {fast} and {slow}"
    );
}

/// Issue #1444's sibling to the FAST/SLOW proof above, for the sight-trainer
/// intro speech instead of Mom's dialog: drives
/// [`OverworldPhase::begin_synthetic_sight_approach_for_test`]'s stand-in
/// approach through the real `phase.step` pipeline until `advance_intro_message`
/// itself opens the box against a synthetic on-disk pack.
#[test]
fn a_sight_trainer_intro_message_paces_at_the_saved_text_speed() {
    use crate::pack_source::PackSource;
    use crate::pack_test_support::{image_entry, pack_bytes, palette_entry};

    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FRAME_BIT_DEPTH: u8 = 4;
    const FONT_BIT_DEPTH: u8 = 2;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    /// Generous: the approach's own icon (sixty frames) and one walked tile
    /// (`WALK_FRAMES_PER_TILE`) plus the face and face-wait frames must all
    /// elapse before the box opens at all.
    const OPEN_FRAME_BUDGET: usize = 200;
    const PRINT_FRAME_BUDGET: usize = 64;
    /// The stand-in intro ("Whoa!") carries at least this many glyphs before
    /// its `{P}`-free end -- see [`SightApproach::intro`]'s own docs for why
    /// there is no trailing prompt to race against.
    const GLYPH_TARGET: usize = 3;
    const OPTIONS_TEXT_SPEED_SLOW: u8 = 0;
    const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

    let pack_bytes_blob = pack_bytes(vec![
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ]);

    let frames_to_reveal_third_glyph = |saved_text_speed: u8| {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-step-sight-trainer-intro-{saved_text_speed}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, &pack_bytes_blob).expect("the scratch directory is writable");
        // `PackSource::Test` needs a `'static` path; leaked once per call,
        // reclaimed only when the test process exits (this file's own
        // Mom-dialog text-speed test uses the identical idiom).
        let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());

        let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
        phase.pack_source = PackSource::Test(leaked_path);
        phase.save2.options_text_speed = saved_text_speed;
        phase.begin_synthetic_sight_approach_for_test();

        let mut frame = 0;
        while phase.dialog.is_none() {
            frame += 1;
            assert!(
                frame <= OPEN_FRAME_BUDGET,
                "the real intro box must open against the synthetic pack within a generous budget"
            );
            phase.step(ButtonState::new());
        }

        let mut print_frame = 0;
        loop {
            print_frame += 1;
            assert!(
                print_frame <= PRINT_FRAME_BUDGET,
                "the third glyph must reveal within the frame budget"
            );
            phase.step(ButtonState::new());
            let dialog = phase
                .dialog
                .as_ref()
                .expect("must still be printing well before waitbuttonpress");
            if dialog.revealed_glyph_count() >= GLYPH_TARGET {
                break;
            }
        }

        let _ = std::fs::remove_file(&path);
        print_frame
    };

    let fast = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_FAST);
    let slow = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_SLOW);

    assert!(
        fast < slow,
        "the sight trainer's intro box for a FAST-saved session must reveal its third glyph in \
         fewer frames than a SLOW-saved one, but they took {fast} and {slow}"
    );
}

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

/// Mutation guard for [`OverworldPhase::step`]'s tileset-animation tick
/// (issue #160): `self.tick` must advance by exactly one per `step` call,
/// and must keep advancing while a dialog box is open -- an explicit
/// fidelity claim in the field's own docs (upstream's
/// `UpdateTilesetAnimations` runs every `VBlank` regardless of message-box
/// state, so background tiles keep animating behind a frozen player).
/// Nothing else in the suite observes `tick` on a headless phase: deleting
/// the increment, moving it below `step`'s dialog early-return, or making it
/// conditional must all fail here.
#[test]
fn step_advances_the_tileset_animation_tick_once_per_frame_even_behind_a_dialog() {
    use engine::text::Token;

    // No dialog: one tick per frame, moving or not.
    let mut phase = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), None);
    assert_eq!(phase.tick, 0, "a freshly built phase starts at tick 0");
    phase.step(ButtonState::new());
    assert_eq!(phase.tick, 1, "an idle frame still advances the animation");
    for expected in 2..=WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.tick,
            u32::from(expected),
            "every frame of a walk step advances the tick exactly once"
        );
    }

    // Dialog open: movement is frozen (see
    // `an_open_dialog_freezes_movement_until_it_closes`), the tick is not.
    let dialog = crate::overworld::dialog::synthetic_dialog(vec![
        Token::Char('A'),
        Token::PromptClear,
        Token::End,
    ]);
    let mut frozen = synthetic_phase(PlayerState::new((7, 4), 3, Direction::South), Some(dialog));
    for expected in 1..=10u32 {
        frozen.step(held(Buttons::DOWN));
        assert_eq!(
            frozen.tick, expected,
            "tileset animation must keep running while a dialog freezes movement"
        );
    }
    assert!(
        frozen.dialog.is_some(),
        "the dialog must still be open -- otherwise the frames above weren't frozen ones"
    );
    assert_eq!(
        frozen.player.position(),
        (7, 4),
        "and movement really was frozen for all of them"
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
