//! The field start menu's own behaviour, driven through a real
//! [`OverworldPhase`] (I-6, issue #232) -- the `START` gate, frame
//! ownership, and the close paths that must never write.
//!
//! Split from the sibling [`super::save_continue_tests`] (`one module = one
//! concept` `(oop-boundaries)`, the same split
//! `crate::main_menu::saved_game_tests` already makes): that file owns the
//! save/reload/continue round trip, this one owns "when may a menu open,
//! and what does it do to the frame". Both share that module's fixtures.
//!
//! The menu's *internal* state machine -- the `sSaveDialogCallback` chain,
//! its prompts, and which `TrySavingData` arm each answer reaches -- is
//! unit-tested against a fake save medium in `crate::start_menu::tests`.

use platform::Buttons;

use super::overworld_phase::OverworldPhase;
use super::save_continue_tests::{new_game_phase, settle};
use super::tests::{held, pressed, TempSave};
use crate::new_game;
use crate::start_menu::{StartMenu, StartMenuItem};
use engine::overworld::PlayerState;

/// #230 review round four, restated as the door rather than the writer
/// (issue #232): a battle owns the frame, so `START` cannot open the start
/// menu -- and with no start menu there is no way to reach a save at all.
/// The blocks hold the *pre-battle* overworld (the party lead is borrowed
/// by the battle, its RNG draws already consumed), so a save taken now
/// would mint a valid save that undoes the live fight and let a quit escape
/// any encounter. Upstream cannot open its start menu mid-battle either.
#[test]
fn the_start_menu_does_not_open_mid_battle() {
    use battle::{BattlePokemon, Dex, Ivs, MAX_IV};
    use engine::overworld::wild_encounter::WildEncounter;
    use engine::rng::Rng;

    let temp = TempSave::new("mid-battle-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    settle(&mut phase);
    assert!(
        phase.start_menu_may_open(pressed(Buttons::START)),
        "the fixture must start somewhere START normally works"
    );

    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    // Level-50 Treecko with Pound vs a Route 101 Wurmple -- the same
    // fixture `wild_encounter::tests` fights full battles with.
    let lead = BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(277),
        50,
        ivs,
        0,
        vec![assets::MoveId(1)],
    )
    .expect("Treecko with Pound is in the dex");
    let mut rng = Rng::new(0x00C0_FFEE);
    let battle = super::wild_encounter::start_wild_battle(
        lead,
        WildEncounter {
            species: assets::SpeciesId(290),
            level: 2,
            slot: 0,
        },
        &mut rng,
    )
    .expect("a Wurmple encounter is fightable");
    phase.wild_battle = Some(battle);
    assert!(phase.in_battle());

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START)),
        "START must not open the start menu during a battle"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the battle"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// The Route 101 scripted first battle (#231) freezes the phase exactly
/// like a wild one -- same borrowed lead, same consumed RNG draws living
/// outside the `SaveBlock`s -- so `START` must be refused for the same
/// reason [`the_start_menu_does_not_open_mid_battle`] documents.
#[test]
fn the_start_menu_does_not_open_mid_first_battle() {
    use engine::rng::Rng;

    let temp = TempSave::new("mid-first-battle-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    let mut rng = Rng::new(0x00C0_FFEE);
    let battle = super::first_battle::start_first_battle(new_game::provisional_starter(), &mut rng)
        .expect("the scripted first battle constructs from the provisional starter");
    phase.first_battle = Some(battle);
    assert!(phase.in_battle());

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START)),
        "START must not open the start menu during the scripted first battle"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the battle"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// #230 review round five, restated as the door (issue #232): `START` must
/// not open the menu while a step is in flight. `save1.pos` is written at
/// step *start*, so the blocks already hold the landing tile while none of
/// the landing's consequences (door warps, encounters, coordinate events --
/// Route 101's scripted first battle among them) have run; a save taken now
/// would resume standing *past* whatever the landing triggers. Upstream's
/// `FieldGetPlayerInput` does not even set `pressedStartButton` while the
/// player is moving (`field_control_avatar.c:95-101`).
#[test]
fn the_start_menu_does_not_open_mid_step() {
    let temp = TempSave::new("mid-step-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    // Hold DOWN until a step is actually in flight -- the first frame may
    // only turn the player in place.
    for _ in 0..8 {
        phase.step(held(Buttons::DOWN));
        if phase.mid_step() {
            break;
        }
    }
    assert!(
        phase.mid_step(),
        "the fixture must catch a step in flight, or the guard is untested"
    );

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START)),
        "START must not open the start menu mid-step"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the walk"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// An open message box holds `LockPlayerFieldControls` upstream, so
/// `ProcessPlayerFieldInput` -- and with it the `START` branch -- is not
/// polled at all. A start menu opening over a live dialog would leave the
/// message box's own input routing fighting the menu's.
#[test]
fn the_start_menu_does_not_open_over_an_open_dialog() {
    use engine::text::Token;

    let phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        ),
        Some(crate::overworld::dialog::synthetic_dialog(vec![
            Token::Char('H'),
            Token::PromptClear,
            Token::End,
        ])),
    );
    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START)),
        "START must not open the start menu while a message box is up"
    );
}

/// The start menu freezes the overworld while it is open, the same way a
/// message box does: a held direction moves nobody until the menu closes,
/// and `EXIT` hands field control back without writing anything.
#[test]
fn an_open_start_menu_freezes_movement_and_exit_writes_nothing() {
    let temp = TempSave::new("menu-freezes-movement");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    settle(&mut phase);
    let before = phase.player.position();
    phase.open_synthetic_start_menu();

    for _ in 0..40 {
        assert!(
            phase.advance_start_menu_frame(held(Buttons::DOWN), &mut save_slot),
            "an open menu owns every frame"
        );
    }
    assert_eq!(
        phase.player.position(),
        before,
        "the player must not walk while the start menu is open"
    );

    assert_eq!(
        phase.start_menu().expect("still open").selected(),
        StartMenuItem::Save
    );
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Exit,
        "DOWN moves to the second item"
    );
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
    assert!(
        phase.start_menu().is_none(),
        "EXIT closes the menu (StartMenuExitCallback)"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "EXIT writes nothing"
    );
}

/// `HandleStartMenuInput`'s own close keys (`JOY_NEW(START_BUTTON |
/// B_BUTTON)`, `start_menu.c:628-633`) -- neither writes anything.
#[test]
fn start_and_b_close_the_menu_without_saving() {
    let temp = TempSave::new("menu-close-keys");
    let mut save_slot = temp.slot();

    for close_key in [Buttons::START, Buttons::B] {
        let mut phase = new_game_phase();
        phase.open_synthetic_start_menu();
        phase.advance_start_menu_frame(pressed(close_key), &mut save_slot);
        assert!(
            phase.start_menu().is_none(),
            "{close_key:?} must close the start menu"
        );
    }
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "closing the menu writes nothing"
    );
}

/// The item cursor wraps (`Menu_MoveCursor`, `src/menu.c:948-962`), unlike
/// the main menu's own no-wrap selection.
#[test]
fn the_start_menu_cursor_wraps() {
    let temp = TempSave::new("menu-cursor-wrap");
    let mut save_slot = temp.slot();
    let mut phase = new_game_phase();
    phase.open_synthetic_start_menu();

    let selected = |phase: &OverworldPhase| phase.start_menu().map(StartMenu::selected);
    assert_eq!(selected(&phase), Some(StartMenuItem::Save));
    phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
    assert_eq!(
        selected(&phase),
        Some(StartMenuItem::Exit),
        "UP from the first item wraps to the last"
    );
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    assert_eq!(
        selected(&phase),
        Some(StartMenuItem::Save),
        "DOWN from the last item wraps to the first"
    );
}
