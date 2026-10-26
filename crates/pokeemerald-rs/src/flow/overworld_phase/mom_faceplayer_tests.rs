//! Mom's `faceplayer` (`data/scripts/players_house.inc:299-301`): the live object event the
//! interaction selected turns toward the player as the interaction starts.

use super::interaction::InteractionOutcome;
use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{select_interaction_target, Direction, InteractionTarget, PlayerState};
use platform::Buttons;

/// Mom's `local_id` and tile in Brendan's House 1F ([`ONE_F`]).
const MOM_LOCAL_ID: u8 = 1;
const MOM_TILE: (i32, i32) = (2, 6);
const MOM_SCRIPT: &str = "PlayersHouse_1F_EventScript_Mom";
/// The Vigoroth beside Mom, whose script this port does not model.
const VIGOROTH_LOCAL_ID: u8 = 2;
const VIGOROTH_SCRIPT: &str = "PlayersHouse_1F_EventScript_Vigoroth2";
const FLOOR_ELEVATION: u8 = 3;

/// Each side of Mom: where the player stands and the direction they face to talk to her.
const SIDES: [((i32, i32), Direction); 4] = [
    ((2, 7), Direction::North),
    ((2, 5), Direction::South),
    ((3, 6), Direction::West),
    ((1, 6), Direction::East),
];

fn mom_facing(phase: &OverworldPhase) -> Direction {
    phase
        .object_events
        .get(MOM_LOCAL_ID)
        .expect("Brendan's House 1F declares Mom as local id 1")
        .state()
        .facing()
}

/// Turns Mom away from the player, so a passing assertion can only come from the interaction.
fn turn_mom_to(phase: &mut OverworldPhase, direction: Direction) {
    phase
        .object_events
        .get_mut(MOM_LOCAL_ID)
        .unwrap()
        .state_mut()
        .face(direction);
}

fn selected_local_id(phase: &OverworldPhase) -> Option<u8> {
    let runtime = phase
        .scene
        .runtime()
        .with_object_events(&phase.object_events);
    match select_interaction_target(&phase.player, &runtime, &phase.save1.event_data, &[]) {
        Some(InteractionTarget::Object(object)) => Some(object.local_id),
        _ => None,
    }
}

/// A pack with just the assets Mom's dialog box loads, so the real `step` can open it.
fn dialog_pack(label: &str) -> &'static std::path::Path {
    use crate::pack_test_support::{image_entry, pack_bytes, palette_entry};
    let bytes = pack_bytes(vec![
        image_entry("text-window/image/message_box", 56, 16, 4, 0),
        palette_entry("text-window/palette/message_box", 16),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            2,
            0,
        ),
    ]);
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-mom-faceplayer-{label}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::flow::tests::write_scratch_exclusive(&path, bytes);
    // `PackSource::Test` needs a `'static` path, leaked once per call as the sibling
    // dialog-pacing tests do.
    Box::leak(path.into_boxed_path())
}

fn phase_beside_mom(
    position: (i32, i32),
    facing: Direction,
    pack: Option<&'static std::path::Path>,
) -> OverworldPhase {
    let mut phase = synthetic_phase(PlayerState::new(position, FLOOR_ELEVATION, facing), None);
    if let Some(path) = pack {
        phase.pack_source = crate::pack_source::PackSource::Test(path);
    }
    phase
}

#[test]
fn mom_turns_toward_the_player_from_each_side_before_her_dialog_prints() {
    for (position, facing) in SIDES {
        let label = format!("{}-{}", position.0, position.1);
        let pack = dialog_pack(&label);
        let mut phase = phase_beside_mom(position, facing, Some(pack));
        turn_mom_to(&mut phase, facing);
        assert_eq!(
            mom_facing(&phase),
            facing,
            "precondition: Mom's back is to the player at {position:?}"
        );

        phase.step(pressed(Buttons::A));
        let _ = std::fs::remove_file(pack);

        assert_eq!(
            mom_facing(&phase),
            facing.opposite(),
            "Mom must face the player who talks to her from {position:?}"
        );
        let dialog = phase
            .dialog
            .as_ref()
            .expect("the A press on Mom opens her dialog");
        assert_eq!(
            dialog.revealed_glyph_count(),
            0,
            "the turn lands on the frame the box opens, before its first glyph"
        );
    }
}

#[test]
fn mom_stays_at_her_tile_and_interactable_after_turning() {
    for (position, facing) in SIDES {
        let mut phase = phase_beside_mom(position, facing, None);
        phase.step(pressed(Buttons::A));

        let mom = phase.object_events.get(MOM_LOCAL_ID).unwrap();
        assert_eq!(mom.state().position(), MOM_TILE);
        assert_eq!(mom.state().previous_position(), MOM_TILE);
        assert_eq!(mom.template().x, i16::try_from(MOM_TILE.0).unwrap());
        assert_eq!(
            selected_local_id(&phase),
            Some(MOM_LOCAL_ID),
            "an A press from {position:?} still selects Mom at her live tile"
        );
    }
}

#[test]
fn the_interaction_keeps_moms_dialog_text_and_adds_only_the_turn() {
    let expected = crate::overworld::npc_scripts::script_text(MOM_SCRIPT).unwrap();
    for (position, facing) in SIDES {
        let phase = phase_beside_mom(position, facing, None);
        let runtime = phase
            .scene
            .runtime()
            .with_object_events(&phase.object_events);
        let outcome = phase.interaction_tokens_this_frame(pressed(Buttons::A), &runtime);
        let Some(InteractionOutcome::Dialog(script)) = outcome else {
            panic!("an A press on Mom from {position:?} must open her dialog, got {outcome:?}");
        };
        let mut phase = phase;
        assert_eq!(phase.start_script_dialog(script), expected);
        assert_eq!(mom_facing(&phase), facing.opposite());
    }
}

#[test]
fn an_object_whose_script_has_no_faceplayer_keeps_its_facing() {
    let mut phase = phase_beside_mom((1, 4), Direction::North, None);
    // A fresh save hides this Vigoroth; unhide it so the press selects an object whose script has no `faceplayer`.
    let hide_flag = assets::object_event_flags::resolve(
        phase
            .object_events
            .get(VIGOROTH_LOCAL_ID)
            .unwrap()
            .template()
            .flag,
    )
    .expect("the Vigoroth carries a hide flag");
    phase.save1.event_data.flag_clear(hide_flag).unwrap();
    let vigoroth = phase.object_events.get(VIGOROTH_LOCAL_ID).unwrap();
    assert_eq!(vigoroth.template().script, VIGOROTH_SCRIPT);
    assert_eq!(vigoroth.state().position(), (1, 3), "fixture precondition");
    assert_eq!(selected_local_id(&phase), Some(VIGOROTH_LOCAL_ID));
    let vigoroth_before = *phase.object_events.get(VIGOROTH_LOCAL_ID).unwrap().state();
    let mom_before = mom_facing(&phase);

    phase.step(pressed(Buttons::A));

    assert!(phase.dialog.is_none(), "an unmodelled script opens no box");
    assert_eq!(
        *phase.object_events.get(VIGOROTH_LOCAL_ID).unwrap().state(),
        vigoroth_before,
        "an unmodelled interaction must not touch the selected object"
    );
    assert_eq!(mom_facing(&phase), mom_before);
}
