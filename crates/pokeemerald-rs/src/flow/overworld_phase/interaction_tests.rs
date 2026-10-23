//! Which target a same-frame A press selects, and that a selected target owns the frame.

use super::step::InteractionOutcome;
use super::OverworldPhase;
use engine::overworld::metatile_behavior::MB_PC;
use engine::overworld::{
    BackgroundEventTarget, Direction, InteractionTarget, MetatileInteractionTarget, PlayerState,
};
use platform::Buttons;

use crate::flow::tests::pressed;

const LITTLEROOT: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN");
const SCENE_SIDE: u16 = 20;
const FLOOR_ELEVATION: u8 = 3;
/// Littleroot Town's sign, a wildcard-facing background event at elevation 0.
const TOWN_SIGN_TILE: (u16, u16) = (15, 13);
const TOWN_SIGN_SCRIPT: &str = "LittlerootTown_EventScript_TownSign";
const PLAIN_TILE_IN_FRONT_OF_NOTHING: (u16, u16) = (10, 10);

fn south_of(tile: (u16, u16)) -> (i32, i32) {
    (i32::from(tile.0), i32::from(tile.1) + 1)
}

fn phase_below(tile: (u16, u16), pc_tiles: &[(u16, u16)]) -> OverworldPhase {
    let specials: Vec<_> = pc_tiles.iter().map(|tile| (*tile, MB_PC)).collect();
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles(
            SCENE_SIDE, SCENE_SIDE, &specials,
        ),
        LITTLEROOT,
        PlayerState::new(south_of(tile), FLOOR_ELEVATION, Direction::North),
        None,
    )
}

fn press_a(phase: &OverworldPhase, buttons: Buttons) -> Option<InteractionOutcome> {
    let scene = &phase.scene;
    let header = assets::MapHeaderTable::new().header(LITTLEROOT).unwrap();
    let events = assets::MapEventsTable::new().resolve(LITTLEROOT).unwrap();
    let runtime = scene
        .runtime(LITTLEROOT, header, events)
        .with_object_events(&phase.object_events);
    phase.interaction_tokens_this_frame(pressed(buttons), &runtime)
}

fn sign() -> InteractionOutcome {
    InteractionOutcome::Unmodelled(InteractionTarget::Background(
        BackgroundEventTarget::Script {
            script: TOWN_SIGN_SCRIPT,
        },
    ))
}

#[test]
fn a_sign_is_selected_even_over_a_pc_metatile_beneath_it() {
    let phase = phase_below(TOWN_SIGN_TILE, &[TOWN_SIGN_TILE]);
    assert_eq!(press_a(&phase, Buttons::A), Some(sign()));
}

#[test]
fn a_pc_metatile_with_no_sign_or_object_is_selected() {
    let phase = phase_below(
        PLAIN_TILE_IN_FRONT_OF_NOTHING,
        &[PLAIN_TILE_IN_FRONT_OF_NOTHING],
    );
    assert_eq!(
        press_a(&phase, Buttons::A),
        Some(InteractionOutcome::Unmodelled(InteractionTarget::Metatile(
            MetatileInteractionTarget::Pc
        )))
    );
}

#[test]
fn facing_no_target_the_press_falls_through() {
    let phase = phase_below(PLAIN_TILE_IN_FRONT_OF_NOTHING, &[]);
    assert_eq!(press_a(&phase, Buttons::A), None);
}

#[test]
fn a_selected_background_or_metatile_target_preempts_the_frames_turn() {
    for (tile, pc_tiles) in [
        (TOWN_SIGN_TILE, vec![]),
        (
            PLAIN_TILE_IN_FRONT_OF_NOTHING,
            vec![PLAIN_TILE_IN_FRONT_OF_NOTHING],
        ),
    ] {
        let mut phase = phase_below(tile, &pc_tiles);
        let start = phase.player.position();
        phase.step(pressed(Buttons::A | Buttons::LEFT));
        assert_eq!(phase.player.position(), start);
        assert_eq!(
            phase.player.facing(),
            Direction::North,
            "the interaction claims the frame before the held direction can turn the player"
        );
        assert!(!phase.player.in_transit());
    }
}
