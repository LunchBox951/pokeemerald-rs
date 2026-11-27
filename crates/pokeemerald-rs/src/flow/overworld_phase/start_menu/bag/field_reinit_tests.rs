//! What the field restarts on the way back from the bag: its tileset
//! animation counter and its field-effect sprites.

use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

use super::test_support::Session;
use super::{BagTrip, OverworldPhase};
use crate::bag_menu::host_fixture;
use crate::flow::overworld_phase::tall_grass_tests::shown_image;
use crate::overworld::test_support::synthetic_scene_with_tall_grass_sprites;

const GROUND: u8 = 3;
/// `CB2_Overworld` frames from the one after the fade in begins to the one
/// the returned menu takes input on, both included: calls 2 to 23 of the
/// fade, the two frames of the task handoff, and the menu's own.
const OVERWORLD_FRAMES_BACK: u32 = 25;

/// `ReturnToFieldLocal` state 1 runs `InitViewGraphics` -> `InitMapView` ->
/// `InitTilesetAnimations`, zeroing the tileset animation counter
/// (`src/overworld.c:523-529,1973-1974,2088-2093`), so the field returns
/// with its animations restarted rather than resumed. The counter then
/// steps once per `CB2_Overworld` frame (`:1465-1475`,
/// `src/tileset_anims.c:586-592`).
#[test]
fn bag_return_restarts_tileset_animations() {
    let mut session = Session::new("anim-reset", &host_fixture::pack_bytes());
    session.phase_mut().tick = 1000;
    session.open_the_bag();
    session.close_the_bag();
    assert_eq!(session.phase().tick, OVERWORLD_FRAMES_BACK);
}

fn fading_back_in(session: &Session) -> bool {
    matches!(
        session.phase().field_bag.trip,
        Some(BagTrip::FieldIn { .. })
    )
}

/// `ResumeMap`'s `ResetSpriteData` destroys the field-effect sprites
/// (`src/overworld.c:1965-1968,2131-2134`), and the respawned player gets a
/// settled tall-grass effect from its first sprite callback
/// (`ResetObjectEventFldEffData`, `src/event_object_movement.c:1794-1802`).
#[test]
fn bag_return_destroys_the_tall_grass_effect_and_places_a_settled_one() {
    let grass = [(5, 4), (6, 4)].map(|tile| (tile, MB_TALL_GRASS, GROUND));
    let phase = OverworldPhase::for_test(
        synthetic_scene_with_tall_grass_sprites(20, 20, &grass),
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((4, 4), GROUND, Direction::East),
        None,
    );
    let mut session = Session::with_phase("grass-reset", &host_fixture::pack_bytes(), phase);
    for held in [Buttons::RIGHT, Buttons::NONE] {
        for _ in 0..40 {
            let phase = session.phase();
            let walking = phase.player.position() != (6, 4);
            if (held == Buttons::RIGHT && !walking) || (held == Buttons::NONE && !phase.mid_step())
            {
                break;
            }
            session.frame(held);
        }
    }
    session.open_the_bag();
    assert_eq!(session.phase().scene.tall_grass_effect_count(), 1);

    session.tap(Buttons::B);
    for _ in 0..200 {
        if fading_back_in(&session) {
            break;
        }
        session.frame(Buttons::NONE);
    }
    assert!(fading_back_in(&session), "the field never faded back in");
    assert_eq!(session.phase().scene.tall_grass_effect_count(), 0);
    session.frame(Buttons::NONE);
    assert_eq!(session.phase().scene.tall_grass_effect_count(), 1);
    assert_eq!(shown_image(session.phase()), 0);
}
