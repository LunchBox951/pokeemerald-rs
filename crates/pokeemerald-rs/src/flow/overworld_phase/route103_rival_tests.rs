//! `route103_rival_trigger` (issue #248, I-5): rival-sprite setup, the
//! A-press interaction trigger, and real-pack reachability. The battle
//! driver and outcome tests live in `route103_rival_driver_tests`, the late
//! frame-order regressions in `route103_rival_frame_order_tests`, and the
//! shared fixtures in `route103_rival_test_support`.
//!
//! A fabricated flat room paired with a *real* `MAP_ROUTE103` id, so
//! `assets::MapEventsTable::resolve` hands back the real rival object event
//! (`local_id` 2, `(10, 3)`, `MOVEMENT_TYPE_FACE_RIGHT`,
//! `"Route103_EventScript_Rival"`, `FLAG_HIDE_ROUTE_103_RIVAL`) without
//! needing a local pack for anything except the two real-pack tests that
//! are explicitly about pack-decoded reachability (the connection-crossing
//! walk and the rendered sprite).

use assets::MapId;
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use engine::save::PlayerGender;
use platform::{ButtonState, Buttons};

use crate::flow::tests::{held, pressed};

use super::route103_rival_test_support::{
    lead, overwhelming_treecko_lead, route_103_phase, route_103_phase_facing_the_rival,
    FLAG_HIDE_ROUTE_103_RIVAL, RIVAL_BRENDAN_NORMAL_GFX_ID, RIVAL_MAY_NORMAL_GFX_ID, RIVAL_TILE,
    ROUTE_103, VAR_OBJ_GFX_ID_0,
};
use super::route103_rival_trigger::{is_rival_trigger, setup_rival_gfx_id_on_transition};
use super::OverworldPhase;

// -- `setup_rival_gfx_id_on_transition` (module docs, step 1) ---------------

/// `Common_EventScript_SetupRivalGfxId`'s own `checkplayergender` pairing
/// (module docs): a male player's `VAR_OBJ_GFX_ID_0` becomes May's numeric
/// graphics id, a female player's becomes Brendan's.
#[test]
fn setup_rival_gfx_id_writes_the_opposite_genders_rival_id() {
    let mut male = EventData::new();
    setup_rival_gfx_id_on_transition(ROUTE_103, &mut male, PlayerGender::Male);
    assert_eq!(
        male.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_MAY_NORMAL_GFX_ID),
        "a male player's rival is May"
    );

    let mut female = EventData::new();
    setup_rival_gfx_id_on_transition(ROUTE_103, &mut female, PlayerGender::Female);
    assert_eq!(
        female.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_BRENDAN_NORMAL_GFX_ID),
        "a female player's rival is Brendan"
    );
}

/// Upstream's own no-op: `checkplayergender`'s two `goto_if_eq`s never
/// match a raw byte outside `MALE`/`FEMALE`, so the var is left untouched
/// when [`Rival::for_gender`] returns `None`.
#[test]
fn setup_rival_gfx_id_leaves_an_unmodelled_gender_untouched() {
    let mut data = EventData::new();
    setup_rival_gfx_id_on_transition(ROUTE_103, &mut data, PlayerGender::Other(9));
    assert_eq!(data.var_get(VAR_OBJ_GFX_ID_0), Ok(0));
}

/// Entering any other map must never touch `VAR_OBJ_GFX_ID_0` -- gated on
/// the map, matching upstream's own map-scoped `MAP_SCRIPT_ON_TRANSITION`.
#[test]
fn setup_rival_gfx_id_is_a_no_op_off_route_103() {
    let mut data = EventData::new();
    setup_rival_gfx_id_on_transition(MapId("MAP_LITTLEROOT_TOWN"), &mut data, PlayerGender::Male);
    assert_eq!(data.var_get(VAR_OBJ_GFX_ID_0), Ok(0));
}

/// The var really is set the instant a synthetic phase is built on Route
/// 103, through the real [`OverworldPhase::new`]/[`OverworldPhase::for_test`]
/// construction path -- not just through the free function directly (the
/// other tests in this section). Item (b) of the issue's own test list.
/// `for_test`/`new` build a fresh save, whose default gender is always
/// [`PlayerGender::Male`] (`crate::new_game::DEFAULT_PLAYER_GENDER`); the
/// other genders are exercised directly against
/// [`setup_rival_gfx_id_on_transition`] above, the same "helper reachable
/// through the real construction path once, exhaustively at the helper
/// itself" split `first_battle_trigger`'s own tests use.
#[test]
fn a_phase_entering_route_103_carries_the_gfx_var_for_its_own_gender() {
    let phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    assert_eq!(
        phase.save2().player_gender,
        PlayerGender::Male,
        "setup: a fresh save's default gender"
    );
    assert_eq!(
        phase.save1().event_data.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_MAY_NORMAL_GFX_ID),
        "a male player's rival is May, set the instant the phase enters Route 103"
    );
}

// -- `is_rival_trigger` (module docs, step 2) --------------------------------

/// Only Route 103's own rival script, only on Route 103 -- item (g) of the
/// issue's own test list: the trigger must not fire on other maps or
/// scripts.
#[test]
fn is_rival_trigger_only_matches_route_103s_own_rival_script() {
    assert!(is_rival_trigger(ROUTE_103, "Route103_EventScript_Rival"));
    assert!(
        !is_rival_trigger(MapId("MAP_LITTLEROOT_TOWN"), "Route103_EventScript_Rival"),
        "the same script name on a different map must not match"
    );
    assert!(
        !is_rival_trigger(ROUTE_103, "Route103_EventScript_Man"),
        "a different object event's script on Route 103 must not match"
    );
    assert!(
        !is_rival_trigger(ROUTE_103, "0x0"),
        "the no-script sentinel must not match"
    );
}

// -- The interaction trigger end to end (module docs, step 2) ---------------

/// Item (d) of the issue's own test list: facing the rival and pressing A
/// starts the trainer battle through the real [`OverworldPhase::step`]
/// path, consumes the frame correctly, and the hide flag is not yet set.
#[test]
fn facing_the_rival_and_pressing_a_starts_the_trainer_battle() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overwhelming_treecko_lead());

    assert!(!phase.is_rival_battle_active(), "setup: no battle yet");
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "an A press facing the rival must start the battle"
    );
    assert!(
        phase.party_lead.is_none(),
        "the lead moves into the battle, matching every other battle handoff"
    );
    assert_eq!(
        phase.save1().event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(false),
        "the hide flag is a post-battle effect, not a trigger-time one"
    );
    assert_eq!(
        phase.rival_battle_outcome(),
        None,
        "the battle just started -- no outcome yet"
    );
}

/// Issue #435 regression, the [`InteractionOutcome::RivalBattle`] arm of the
/// same fix (see `step_tests::a_pressed_with_a_perpendicular_direction_finds_mom_and_does_not_turn_the_player`
/// and [`OverworldPhase::step`]'s own "NPC dialog routing" section for the
/// upstream citations): a perpendicular direction held alongside the A
/// press must not turn the player away from the rival before the
/// pre-movement interaction lookup runs.
#[test]
fn facing_the_rival_with_a_perpendicular_direction_starts_the_battle_without_turning() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.party_lead = Some(overwhelming_treecko_lead());
    let start_position = phase.player.position();

    // North is perpendicular to the East facing `route_103_phase_facing_the_rival`
    // sets up -- a step in that direction would turn the player away from
    // the rival if movement ran before the interaction lookup.
    phase.step(pressed(Buttons::A | Buttons::UP));

    assert!(
        phase.is_rival_battle_active(),
        "the pre-movement facing (still East) must still find the rival"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::East,
        "the interaction must preempt the turn, not just the step"
    );
    assert_eq!(phase.player.position(), start_position, "and the step too");
}

/// Holding A through the frame after the battle starts must not re-run the
/// interaction lookup or start a second battle. Note what this does and
/// does not prove: a *held* A is not a fresh edge, so
/// [`OverworldPhase::interaction_tokens_this_frame`]'s `is_newly_pressed`
/// gate already discards it regardless of the battle -- the
/// battle-ownership short-circuit itself is pinned by
/// `input_during_the_rival_battle_neither_moves_the_player_nor_re_triggers`
/// below, which uses inputs that gate alone would otherwise act on.
#[test]
fn a_held_a_after_the_battle_starts_does_not_re_trigger_it() {
    let mut phase = route_103_phase_facing_the_rival();
    // Even levels on both sides (module docs' own
    // `one_turn_does_not_immediately_end_a_fresh_battle` precedent): the
    // point here is "no re-trigger", not "how long the fight lasts", so
    // the fixture must not itself end the battle on the very next turn.
    phase.party_lead = Some(lead(277, 5, 1));
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "setup: the fresh edge fired"
    );

    phase.step(held(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "the same battle instance must still be the one running"
    );
}

/// [`OverworldPhase::step`]'s battle-ownership check (its module docs: an
/// in-progress rival battle "owns the frame outright") really does
/// short-circuit everything below it --
/// `advance_route103_rival_battle_frame`'s place in the `||` chain, the
/// same coverage the sibling first-battle and wild-battle gates already
/// have. A held direction during the battle would otherwise turn or move
/// the player ([`super::input::held_direction`] runs on any ordinary
/// frame), and a fresh A edge would re-run the interaction lookup; with
/// the battle owning the frame, neither may observably happen.
#[test]
fn input_during_the_rival_battle_neither_moves_the_player_nor_re_triggers() {
    let mut phase = route_103_phase_facing_the_rival();
    // Even levels (the `one_turn_does_not_immediately_end_a_fresh_battle`
    // precedent): the battle must survive the frames this test steps.
    phase.party_lead = Some(lead(277, 5, 1));
    let position = phase.player.position();
    let facing = phase.player.facing();
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");

    // A held direction away from the current (East) facing: on an ordinary
    // frame this would at least turn the player in place.
    phase.step(held(Buttons::UP));
    assert!(phase.is_rival_battle_active());
    assert_eq!(
        phase.player.position(),
        position,
        "the battle owns the frame -- a held direction must not move the player"
    );
    assert_eq!(phase.player.facing(), facing, "nor even turn them in place");

    // A fresh A edge: on an ordinary frame this would re-run the
    // interaction lookup against the still-un-hidden rival.
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "a fresh A edge during the battle must not reach the interaction lookup"
    );
    assert_eq!(phase.player.position(), position);
    assert_eq!(phase.player.facing(), facing);
}

/// Facing away from the rival must not start anything -- the ordinary
/// facing gate [`engine::overworld::facing_object_event`] already
/// enforces, unaffected by this trigger.
#[test]
fn facing_away_from_the_rival_does_not_start_the_battle() {
    let (rx, ry) = RIVAL_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx - 1, ry), 3, Direction::West));
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
    assert!(phase.party_lead.is_some(), "the lead must be untouched");
}

/// No party lead at all -- the same defensive `None` arm
/// [`OverworldPhase::begin_wild_battle`]/`begin_first_battle` document for a
/// bare test phase. Production always has one; this only matters here.
#[test]
fn the_trigger_with_no_party_lead_logs_and_starts_nothing() {
    let mut phase = route_103_phase_facing_the_rival();
    assert!(phase.party_lead.is_none(), "setup");
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
}

/// The trigger does not fire on other maps: a synthetic phase built on
/// Littleroot Town, with a fabricated lead, pressing A while facing empty
/// ground, must never start a rival battle -- and Mom's own dialog path
/// (already pinned in `overworld_phase::frame_tests`) is untouched
/// by this trigger's addition, since [`is_rival_trigger`] gates on
/// [`ROUTE_103`] before anything else runs.
#[test]
fn the_trigger_never_fires_off_route_103() {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((4, 4), 3, Direction::South),
        None,
    );
    phase.party_lead = Some(overwhelming_treecko_lead());
    phase.step(pressed(Buttons::A));
    assert!(!phase.is_rival_battle_active());
}

// -- Real-pack reachability --------------------------------------------------

/// Item (a) of the issue's own test list: walking off Route 101's own
/// north edge crosses into Oldale Town, and walking off Oldale's own north
/// edge in turn crosses into Route 103 -- completing the chain
/// `walking_off_littlerootss_north_edge_crosses_into_route_101_and_back`
/// (`crate::flow::overworld_phase::connections_tests`) already proves the
/// first leg of.
///
/// Starts one ordinary tile south of Route 101's own north edge rather than
/// walking that map's whole interior: Route 101's own `x = 10`/`11` column
/// (the one the Littleroot crossing above lands on) runs into real,
/// unrelated collision around `y = 6`/`13` (Birch/rescue-scene set
/// dressing this map carries -- checked against the real extracted grid,
/// not assumed), so a single held-direction walk the length of the map
/// cannot reach the edge. That is Route 101's own pre-existing layout, no
/// part of this issue's own scope; starting adjacent to each edge (the
/// same convention `route_103_phase_facing_the_rival`, and this file's own
/// sibling `first_battle_trigger` tests' `route_101_trigger_phase`, already
/// use) still exercises the real crossing math and the real grid data at
/// both new boundaries, which is this test's actual subject. Oldale's own
/// `x = 10` column *is* fully walkable end to end (also checked against
/// the real grid), so that whole leg is walked for real.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_north_from_route_101_crosses_oldale_town_into_route_103() {
    let route101 = MapId("MAP_ROUTE101");
    let oldale = MapId("MAP_OLDALE_TOWN");
    let scene = crate::overworld::load_room(
        route101,
        crate::overworld::PlayerCharacter::Brendan,
        &EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // One ordinary tile south of Route 101's own north edge (module docs).
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, route101, player, None);

    let walk_north_one_tile = |phase: &mut OverworldPhase| {
        phase.step(held(Buttons::UP));
        for _ in 1..engine::overworld::WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    // One ordinary step north lands on the map's own last interior row.
    walk_north_one_tile(&mut phase);
    assert_eq!(phase.map_id, route101, "still on Route 101's own grid");
    assert_eq!(phase.player.position(), (10, 0));

    // The second step crosses Route 101's own north edge into Oldale Town.
    walk_north_one_tile(&mut phase);
    assert_eq!(
        phase.map_id, oldale,
        "walking off Route 101's north edge must cross into Oldale Town"
    );
    assert_eq!(
        phase.player.position(),
        (10, 19),
        "offset 0 carries x straight across, landing on Oldale's own south edge \
         (height 20, so y = 19)"
    );

    // 19 more steps walk the length of Oldale's own column (y: 19 -> 0),
    // for real -- this column has no interior collision.
    for _ in 0..19 {
        walk_north_one_tile(&mut phase);
        assert_eq!(phase.map_id, oldale, "still on Oldale's own grid");
    }
    assert_eq!(phase.player.position(), (10, 0));

    // A stale temp flag "from the departed map": `ClearTempFieldEventData`
    // (`overworld.c:798`, `LoadMapFromCameraTransition`) runs on a
    // connection crossing exactly like on a warp, so Route 103's
    // cuttable-tree flags (`FLAG_TEMP_12`/`_13`,
    // `assets::object_event_flags`) can never arrive pre-set and keep a
    // tree hidden. `0x12` is `FLAG_TEMP_12`.
    phase.save1.event_data.flag_set(0x12).unwrap();

    // The final step crosses Oldale's own north edge into Route 103 -- I-5's
    // own traversal target.
    walk_north_one_tile(&mut phase);
    assert_eq!(
        phase.map_id, ROUTE_103,
        "walking off Oldale's north edge must cross into Route 103"
    );
    assert_eq!(
        phase.player.position(),
        (10, 21),
        "offset 0 carries x straight across, landing on Route 103's own south edge \
         (height 22, so y = 21)"
    );
    assert_eq!(phase.player.elevation(), 3);
    assert!(!phase.player.in_transit());
    // The temp flag set on Oldale's grid above did not survive the
    // crossing's map load.
    assert_eq!(
        phase.save1().event_data.flag_get(0x12),
        Ok(false),
        "a connection crossing is a map load -- `ClearTempFieldEventData`'s port must \
         clear the temp flag range before the entered map's transition effects run"
    );
    // The crossing also ran this port's on-transition effects for Route
    // 103, so the rival's own gfx var is already primed on arrival.
    assert_eq!(
        phase.save1().event_data.var_get(VAR_OBJ_GFX_ID_0),
        Ok(RIVAL_MAY_NORMAL_GFX_ID),
        "a fresh (default-gender) phase's rival is May the instant the crossing lands"
    );
    // ... and `cross_connection` decoded the arrival scene against the
    // *transitioned* store, not the pre-transition one -- the ordering
    // `warp_to`/`cross_connection` (`super::connections`) were structured
    // around. `OBJ_EVENT_GFX_VAR_0` binds a sprite only when the var
    // already held a rival id at decode time, so this is the one
    // production-path probe that the rival is actually drawable the instant
    // the player walks in, not just that the var got written.
    assert!(
        phase.scene.binds_sprite("OBJ_EVENT_GFX_VAR_0"),
        "the post-crossing rebind must decode against the transitioned event data -- \
         the rival binds a sprite the instant the crossing lands"
    );
}

// The rendered *contents* of the `OBJ_EVENT_GFX_VAR_0` binding -- which
// sheet and palette bank the rival draws from -- are pinned in
// `crate::overworld::tests::real_pack_route_103_rival_binds_to_the_opposite_protagonists_sheet`,
// not here: `OverworldScene`'s sprite/OAM internals are private to the
// `overworld` module tree, and `flow::overworld_phase` sees only the
// yes/no `OverworldScene::binds_sprite` probe the crossing walk above
// uses.
