//! Sight-cone geometry, the fainted-lead screen, the honest cuts (held-item and
//! double-battle parties), the no-lead and off-Route-103 refusals, and the
//! real-pack terrain check, all through the real trigger.

use assets::MapId;
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use super::super::test_support::pressed;
use super::super::{OverworldPhase, SyntheticStartMenu};
use super::support::*;

// -- Sight-cone geometry, through the real trigger -------------------------

/// The trigger itself (issue #264): standing within a real sight trainer's
/// cone attempts the battle on an ordinary frame -- **no button press at
/// all** (unlike the rival's own A-press interaction trigger), matching
/// upstream's `CheckForTrainersWantingBattle` running unconditionally ahead
/// of every other per-frame check. The attempt itself currently fails to
/// construct (`begin_sight_trainer_approach_if_seen`'s own docs: Rhett's
/// real level-up moveset includes a move this battle engine does not yet
/// implement) -- so this pins the *honest current* observable behaviour:
/// the cone genuinely fires, the real handoff is genuinely attempted against
/// the real extracted party, it genuinely fails, and the failure is logged
/// rather than silently swallowed or soft-locking the player. Not a
/// contradiction of that issue's own "starts the battle" framing so much as a
/// gap this port's own tests should not paper over -- see
/// `battle::winning_sets_the_defeated_flag_and_the_fight_cannot_restart` and
/// the rest of `battle` for how the win/loss/defeated-flag *driver* half is
/// still pinned, with a stand-in constructible party.
#[test]
fn standing_in_a_real_trainers_cone_attempts_the_real_handoff_which_currently_fails_to_construct() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());

    assert!(
        !phase.is_sight_trainer_battle_active(),
        "setup: no battle yet"
    );
    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Rhett's real moveset currently fails construction (the trigger's own \
         `Refusals cost nothing, forever`) -- \
         update this test once move coverage grows enough for it to succeed"
    );
    assert!(
        phase.party_lead.is_some(),
        "a refused handoff must not consume the lead -- no soft lock"
    );
}

/// Issue #436 regression, driven through [`OverworldPhase::step`] itself:
/// the cone scan claims its trigger frame ahead of a fresh `START`, and
/// falls through to it when the cone refuses. Upstream reaches
/// `pressedStartButton` (`field_control_avatar.c:182`) only past
/// `CheckForTrainersWantingBattle` (`:150`).
///
/// Both halves stand in Rhett's own real cone with a menu that really
/// builds; only the claiming half's party is the borrowed
/// [`STAND_IN_TRAINER`] (`sight_trainer_tests` docs, "The stand-in party").
#[test]
fn start_does_not_preempt_the_sight_trainer_scan_on_its_trigger_frame() {
    let (rx, ry) = RHETT_TILE;

    let mut claimed = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    claimed.party_lead = Some(overwhelming_lead());
    claimed.synthetic_start_menu = SyntheticStartMenu::Builds;
    claimed.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));
    claimed.step(pressed(Buttons::START));
    assert!(
        claimed.sight_approach.is_some(),
        "setup: the cone must really have claimed this frame"
    );
    assert!(
        claimed.start_menu().is_none(),
        "the scan owns its trigger frame -- START must not open the menu"
    );

    let mut refused = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    refused.party_lead = Some(overwhelming_lead());
    refused.synthetic_start_menu = SyntheticStartMenu::Builds;
    refused.step(pressed(Buttons::START));
    assert!(
        refused.sight_approach.is_none(),
        "setup: Rhett's own real party must still refuse to construct"
    );
    assert!(
        refused.start_menu().is_some(),
        "a refused scan must fall through to pressedStartButton"
    );
}

/// Issue #264's review finding F1, pinned: a cone whose battle *cannot be
/// constructed* is re-reached on every single frame the player stands in it
/// (there is no button gate on this check), so the refusal has to cost
/// nothing at all -- not "nothing on the first frame", nothing ever. Before
/// the pre-flight screen (`crate::flow::npc_trainer_battle`'s module docs)
/// this leaked `CreateNPCTrainerParty`'s per-mon OT-id draws sixty times a
/// second off the one stream every wild encounter and battle turn shares.
///
/// All three refusal shapes are covered: an unimplemented moveset (Rhett,
/// the trigger's own "Refusals cost nothing, forever"), a held-item party
/// (Miguel, same), and a double battle refused before construction is even
/// attempted (Amy, `trainer_data_wants_double_battle`).
#[test]
fn standing_in_a_cone_for_many_frames_never_touches_the_rng_stream() {
    let (rx, ry) = RHETT_TILE;
    let (mx, my) = MIGUEL_TILE;
    let (ax, ay) = AMY_TILE;
    let cases = [
        ("Rhett (unimplemented moveset)", (rx, ry + 1)),
        ("Miguel (held-item party)", (mx + 2, my)),
        ("Amy (double battle)", (ax, ay + 1)),
    ];
    for (name, tile) in cases {
        let mut phase = route_103_phase(PlayerState::new(tile, 3, Direction::North));
        phase.party_lead = Some(overwhelming_lead());
        let before = phase.rng.state();
        for frame in 0..FRAMES_STANDING_STILL {
            phase.step(ButtonState::new());
            assert!(
                !phase.is_sight_trainer_battle_active(),
                "{name}: frame {frame} must still refuse"
            );
            assert_eq!(
                phase.rng.state(),
                before,
                "{name}: frame {frame} moved the shared RNG stream -- a refused cone must \
                 draw nothing, on every frame, forever"
            );
        }
        assert!(
            phase.party_lead.is_some(),
            "{name}: the lead is never consumed by a refusal"
        );
    }
}

/// A player one tile beyond a trainer's own sight range must not trigger.
///
/// Uses the constructible [`STAND_IN_TRAINER`] (`sight_trainer_tests` docs,
/// "The stand-in party") rather than Andrew's own real party, so a
/// false-positive cone hit would be observable as a started approach instead
/// of masked by every real trainer's own construction refusal.
#[test]
fn a_player_beyond_range_does_not_trigger() {
    let (ax, ay) = ANDREW_TILE;
    // Andrew's own range is 3; four tiles south is one past it.
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 4), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));
    phase.step(ButtonState::new());
    assert!(
        phase.sight_approach.is_none(),
        "a cone genuinely out of range must not start an approach, even against a party \
         that could actually construct"
    );
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(phase.party_lead.is_some(), "the lead must be untouched");
}

/// A player off a trainer's own facing axis must not trigger, even standing
/// right beside them.
///
/// See [`a_player_beyond_range_does_not_trigger`] for why [`STAND_IN_TRAINER`]
/// stands in here too.
#[test]
fn a_player_off_the_facing_axis_does_not_trigger() {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax + 1, ay), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));
    phase.step(ButtonState::new());
    assert!(
        phase.sight_approach.is_none(),
        "a cone genuinely off the facing axis must not start an approach, even against a \
         party that could actually construct"
    );
    assert!(!phase.is_sight_trainer_battle_active());
}

/// The sight check itself draws nothing -- pinned against a refused trigger
/// (out of range) so the RNG state is directly comparable before/after.
#[test]
fn a_refused_sight_check_draws_nothing() {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 4), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();
    phase.step(ButtonState::new());
    assert_eq!(
        phase.rng.state(),
        before,
        "the geometry check must draw nothing"
    );
}

// -- The fainted-lead fail-closed screen ------------------------------------
//
// Formerly pinned here as a caller-side test of this trigger's own
// `is_fainted` guard -- retired along with that guard (issue #347):
// `begin_sight_trainer_battle_if_seen` was the *only* in-tree caller still
// carrying its own copy by the time of this issue -- `route103_rival_trigger`
// retired its own equivalent guard back at issue #251 (that module's own
// docs), and this module's guard just never got the same follow-up. A
// per-caller test gave a false sense that the property was caller-specific
// when the actual guarantee belongs to `start_npc_trainer_battle` itself.
// The equivalent -- and strictly stronger, since it now covers every caller
// including the one that had already lost its own screen -- coverage is
// `npc_trainer_battle::tests::a_fainted_player_lead_is_refused_before_any_draw`.
// `standing_in_a_cone_for_many_frames_never_touches_the_rng_stream` above
// still proves this trigger's own multi-frame no-draw property end to end.
// The integration half below re-drives the trigger with a fainted lead
// through the shared refusal arm, so the caller-level coverage is not the
// only pin on this shape.

/// The fainted-lead refusal, end to end through the trigger: the shared
/// constructor's screen (issue #347) refuses inside `step`, no battle
/// starts, the lead stays in the party, and the stream never moves.
#[test]
fn a_fainted_lead_is_refused_through_the_trigger_without_a_draw() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    let mut fainted = overmatched_lead();
    fainted.apply_damage(u32::MAX);
    assert!(fainted.is_fainted(), "setup: the lead really is fainted");
    phase.party_lead = Some(fainted);

    let before = phase.rng.state();
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(
        phase.party_lead.is_some(),
        "the refused handoff leaves the lead in the party"
    );
    assert_eq!(phase.rng.state(), before);
}

// -- Honest cuts: Miguel's held item, Amy & Liv's double battle -------------

/// The held-item cut (issue #264): Miguel's cone reaches, but his real
/// held-item party refuses to construct -- no battle starts, the lead is
/// untouched, and the refusal draws nothing (the held-item error is raised
/// before any RNG draw, `crate::flow::npc_trainer_battle`'s own docs).
#[test]
fn miguels_held_item_party_refuses_to_construct() {
    let (mx, my) = MIGUEL_TILE;
    // Miguel faces east; two tiles east of him is within his own range-5 cone.
    let mut phase = route_103_phase(PlayerState::new((mx + 2, my), 3, Direction::West));
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Miguel's held-item party must refuse to construct (not modelled)"
    );
    assert!(
        phase.party_lead.is_some(),
        "the refused handoff leaves the lead in the party"
    );
    assert_eq!(
        phase.rng.state(),
        before,
        "the held-item refusal must draw nothing"
    );
}

/// The doubles cut (issue #264): Amy's cone reaches, but her real party
/// is a double battle this port cannot field (no doubles support, and at
/// most one tracked party mon) -- no battle starts.
#[test]
fn amys_double_battle_party_is_refused() {
    let (ax, ay) = AMY_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Amy & Liv's shared double-battle party must never be selected"
    );
    assert!(phase.party_lead.is_some());
}

// -- No party lead ------------------------------------------------------------

/// No party lead at all -- the same defensive `None` arm every other
/// battle-trigger test file pins.
#[test]
fn the_trigger_with_no_party_lead_logs_and_starts_nothing() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    assert!(phase.party_lead.is_none(), "setup");
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

// -- The trigger never fires off Route 103 -----------------------------------

/// A synthetic phase on a different map, at coordinates that would match
/// Rhett's own tile numerically, must never start a sight-trainer battle --
/// `MapEventsTable::resolve` hands back that map's own (empty, in this
/// fixture) object events, not Route 103's.
#[test]
fn the_trigger_never_fires_off_route_103() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(80, 16),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((rx, ry + 1), 3, Direction::North),
        None,
    );
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

// -- Real-pack terrain: real collision and elevation (issue #264) ----------

/// The geometry tests above run over a synthetic, fully open room; this
/// confirms the same cone fires against Route 103's own *real* extracted
/// terrain -- collision bits, elevation, **and object-event occupancy**, not
/// just declared coordinates -- and that standing in it costs the shared RNG
/// stream nothing, frame after frame.
///
/// The tile matters, and vetting it means all three (issue #264 review, F5):
/// this test originally stood the player at `(67, 7)`, whose real decoded
/// cell is indeed open ground at elevation 3 -- but Route 103 declares an
/// `OBJ_EVENT_GFX_CUTTABLE_TREE` object event standing on it
/// (`data/maps/Route103/map.json`), so no player can ever be there without
/// HM01. `(67, 6)`, one tile south of Rhett, is genuinely occupiable: open
/// ground, elevation 3, and no object event of any kind. It is also
/// distance **1**, which after this same review's F3 fix is the newly
/// guarded case -- the whole `GetCollisionAtCoords` chain applies to the
/// player's own tile with no intermediate tile ahead of it to catch
/// anything first (`engine::overworld::trainer_sight`'s own docs).
///
/// The positive "the geometry really fired" signal is the geometry itself,
/// asked directly over the real runtime: it cannot be the RNG any more,
/// because a cone that reaches and refuses must now leave the stream exactly
/// where it found it (that is the property under test).
/// `#[ignore]`d like this crate's other real-pack tests: run
/// `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_rhetts_cone_reaches_the_player_over_real_terrain_and_draws_nothing() {
    let scene = crate::overworld::load_room(
        ROUTE_103,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let (rx, ry) = RHETT_TILE;
    let tile = (rx, ry + 1);
    let player = PlayerState::new(tile, 3, Direction::North);

    // The cone genuinely reaches, over the real decoded grid -- asked of the
    // same `engine` geometry the trigger uses, with the real object events.
    let header = assets::MapHeaderTable::new()
        .header(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let events = assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let rhett = events
        .object_events
        .iter()
        .find(|event| event.script == "Route103_EventScript_Rhett")
        .expect("Route 103 declares Rhett's own object event");
    assert!(
        engine::overworld::trainer_can_see_player(
            rhett,
            &scene.runtime(ROUTE_103, header, events),
            &player,
            &engine::event_data::EventData::new(),
        ),
        "Rhett's real cone must reach a player standing one tile south of him over real \
         terrain -- including the final tile's own collision/impassability arms"
    );

    let mut phase = OverworldPhase::for_test(scene, ROUTE_103, player, None);
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    for frame in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
        assert!(
            !phase.is_sight_trainer_battle_active(),
            "frame {frame}: the real handoff still fails to construct (the trigger's own docs)"
        );
        assert_eq!(
            phase.rng.state(),
            before,
            "frame {frame}: standing in a real cone whose battle cannot start must leave the \
             shared stream byte-identical (issue #264 review, F1)"
        );
    }
    assert!(
        phase.party_lead.is_some(),
        "a refused handoff must not consume the lead -- no soft lock"
    );
}
