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

/// `ReturnToFieldLocal` recreates every active object's sprite
/// (`SpawnObjectEventOnReturnToField`, `src/event_object_movement.c:1728-1809`),
/// so `CreateSprite` zeroes `sTypeFuncId` and the movement delay: a `LookAround`
/// NPC restarts at step 0 and draws a fresh delay on its first callback after
/// the returned menu closes, rather than finishing the delay it had pending.
#[test]
fn bag_return_restarts_a_pending_npc_movement_delay() {
    use crate::flow::overworld_phase::fixed_facing_tests::{phase_over, stand_near};
    use engine::overworld::object_event::ObjectEventCollection;
    use engine::rng::Rng;

    let events = assets::MapEventsTable::new()
        .resolve(assets::MapId("MAP_ROUTE101"))
        .unwrap();
    let mut phase = phase_over(assets::MapId("MAP_ROUTE101"), events);
    let template = phase.object_events.get(1).unwrap().template();
    phase.object_events = ObjectEventCollection::from_templates(std::slice::from_ref(template));
    phase.rng = Rng::new(0x1234);
    stand_near(&mut phase, 1);
    let mut session = Session::with_phase("npc-reset", &host_fixture::pack_bytes(), phase);
    // Seed 0x1234: the youngster draws a 128-frame delay on his first frame.
    session.frame(Buttons::NONE);
    let after_delay_draw = session.phase().rng.state();
    session.idle(4);
    assert_eq!(session.phase().rng.state(), after_delay_draw);

    session.open_the_bag();
    session.close_the_bag();
    assert_eq!(
        session.phase().rng.state(),
        after_delay_draw,
        "frozen on the trip"
    );
    // B hides the menu and unfreezes the objects in the menu's task, before
    // that frame's sprite callbacks (`src/start_menu.c:629-633`,
    // `src/overworld.c:1465-1469`).
    session.frame(Buttons::B);
    assert!(session.phase().start_menu().is_none(), "B closes the menu");
    let mut one_draw_later = Rng::new(after_delay_draw);
    one_draw_later.next_u16();
    assert_eq!(session.phase().rng.state(), one_draw_later.state());
    session.idle(8);
    assert_eq!(session.phase().rng.state(), one_draw_later.state());
}
