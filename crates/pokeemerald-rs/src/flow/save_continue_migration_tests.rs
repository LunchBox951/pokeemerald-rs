//! Continue-time migrations of legacy or malformed save data -- facing,
//! elevation, heal-location, and fainted-lead fallbacks -- split from
//! [`super::save_continue_tests`]'s round trip (`one module = one concept`
//! `(oop-boundaries)`).

use engine::overworld::Direction;
use engine::save::{SaveBlock1, SaveBlock2, WarpData};

use super::overworld_phase::OverworldPhase;
use super::save_continue_tests::{a_damaged_lead, dormant_party_member, new_game_phase};
use crate::new_game;

/// `saved_tile_placement`'s one substitution, pinned (issue #214 review):
/// a save standing on an `ELEVATION_MULTI_LEVEL` (15) tile resumes at
/// `ELEVATION_TRANSITION`, exactly the `ObjectEventUpdateElevation`
/// behaviour the warp path already applies -- never at the raw 15, which
/// no walking player can legitimately hold. The ordinary-tile case (the
/// fixture's uniform elevation 3) is pinned by the round-trip snapshot.
#[test]
fn continue_on_a_multi_level_tile_resumes_at_the_transition_elevation() {
    use engine::overworld::{ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};

    let bridge_tile = (4_u16, 4_u16);
    let scene = crate::overworld::tests::synthetic_scene_with_cell_elevation(
        10,
        10,
        bridge_tile,
        ELEVATION_MULTI_LEVEL,
    );
    let mut block1 = SaveBlock1::default();
    block1.pos.x = i16::try_from(bridge_tile.0).unwrap();
    block1.pos.y = i16::try_from(bridge_tile.1).unwrap();

    let resumed =
        OverworldPhase::from_saved(scene, new_game::SPAWN_MAP_ID, block1, SaveBlock2::default());
    assert_eq!(
        resumed.player.elevation(),
        ELEVATION_TRANSITION,
        "a multi-level tile must resume as a transition, not as raw {ELEVATION_MULTI_LEVEL}"
    );
}

/// A save block whose player object event was never written -- a zeroed
/// [`SaveBlock1`], or an image from before issue #232 modelled the field --
/// holds `DIR_NONE`, which is no walking direction at all. The continue
/// must fall back to the tile-derived `GetAdjustedInitialDirection`
/// (`DIR_SOUTH` on an ordinary tile) rather than face an arbitrary way.
#[test]
fn a_save_with_no_recorded_facing_falls_back_to_the_tile_derived_direction() {
    let block1 = SaveBlock1::default();
    assert_eq!(
        block1.player_object_event.facing_direction, 0,
        "a zeroed block holds DIR_NONE"
    );
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        block1,
        SaveBlock2::default(),
    );
    assert_eq!(resumed.player.facing(), Direction::South);
}

/// A save written before issue #261 had no writer for `last_heal_location`
/// at all, so every such image carries the zeroed [`WarpData::default`] --
/// which *resolves* (group 0/num 0 is a real generated-table entry), so
/// without migration the first white-out of an upgraded save would warp to
/// Petalburg City at `(0, 0)` instead of home (issue #261 review). The
/// continue must adopt the same gender default a fresh game gets, and must
/// leave a genuinely written heal location alone.
#[test]
fn a_save_with_a_legacy_zeroed_heal_location_adopts_the_gender_default() {
    let block1 = SaveBlock1::default();
    assert_eq!(
        block1.last_heal_location,
        WarpData::default(),
        "a zeroed block holds the legacy marker"
    );
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        block1,
        SaveBlock2::default(),
    );
    assert_eq!(
        resumed.save1.last_heal_location,
        new_game::default_last_heal_location(SaveBlock2::default().player_gender),
        "the legacy all-zero value migrates to the gender default"
    );

    // A modern save's genuinely written value survives untouched.
    let written = WarpData {
        map_group: 0,
        map_num: 9,
        warp_id: -1,
        x: 6,
        y: 8,
    };
    let block1 = SaveBlock1 {
        last_heal_location: written,
        ..SaveBlock1::default()
    };
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        block1,
        SaveBlock2::default(),
    );
    assert_eq!(resumed.save1.last_heal_location, written);
}

/// `VAR_ROUTE101_STATE` (`pokeemerald/include/constants/vars.h:116`) --
/// independently transcribed, this file's own copy for the legacy-save
/// signature below.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// A save matching the pre-#251 legacy signature -- a *single-member*
/// party whose lead is fainted with `VAR_ROUTE101_STATE` still at the
/// trigger-consumed `2` -- is the one image only a build between issues
/// #261 and #251 could serialize: a lost Route 101 first battle returned
/// the player to the field fainted (`CB2_EndFirstBattle` has no
/// `IsPlayerDefeated` branch) and the start menu saved it, while the
/// first-battle conclusion now heals every outcome (and writes `3`) the
/// frame the battle ends, and upstream cannot save a party-wide faint at
/// all. Such a save is healed on continue (PR #291 review): without the
/// migration, every eligible grass step would spend the encounter and
/// wild-mon RNG draws before `Battle::new` refused the fainted battler --
/// repeatable draws with no upstream counterpart. Everything *outside*
/// the signature must round-trip untouched, and the later halves pin each
/// boundary: a fainted slot 0 with a healthy member behind it (ordinary
/// upstream state, PR #291 review second round), a fainted single lead
/// whose var never reached `2`, and a merely damaged lead.
#[test]
fn a_save_with_a_fainted_lead_is_healed_on_continue() {
    fn fainted_lead() -> battle::BattlePokemon {
        let mut fainted = new_game::provisional_starter();
        fainted.apply_damage(u32::MAX);
        assert!(fainted.is_fainted(), "setup: the lead must start fainted");
        fainted
    }
    fn resume(mut seed: OverworldPhase, var: Option<u16>) -> OverworldPhase {
        if let Some(value) = var {
            seed.save1
                .event_data
                .var_set(VAR_ROUTE101_STATE, value)
                .expect("VAR_ROUTE101_STATE is an ordinary var");
        }
        OverworldPhase::from_saved(
            crate::overworld::tests::synthetic_scene(10, 10),
            seed.map_id,
            seed.save1,
            seed.save2,
        )
    }

    // The legacy signature itself: single member, var at 2, lead fainted.
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] =
        crate::party::to_save_pokemon(&battle::Dex::new(), &fainted_lead());
    let resumed = resume(seed, Some(2));
    assert!(
        !resumed
            .party_lead
            .as_ref()
            .expect("the migrated save still has its lead")
            .is_fainted(),
        "a fainted lead from a pre-#251 save is healed on load"
    );

    // Boundary one: a fainted slot 0 with a healthy dormant member behind
    // it is ordinary upstream state, not the legacy marker -- untouched.
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 2;
    seed.save1.player_party[0] =
        crate::party::to_save_pokemon(&battle::Dex::new(), &fainted_lead());
    seed.save1.player_party[1] = dormant_party_member(0x7777_8888);
    let resumed = resume(seed, Some(2));
    assert!(
        resumed
            .party_lead
            .as_ref()
            .expect("the multi-member save still has its lead")
            .is_fainted(),
        "a fainted lead backed by a healthy member is no legacy marker and stays fainted"
    );

    // Boundary two: the var outside the trigger-consumed value -- a state
    // no pre-#251 loss path produced -- is likewise untouched.
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] =
        crate::party::to_save_pokemon(&battle::Dex::new(), &fainted_lead());
    let resumed = resume(seed, None);
    assert!(
        resumed
            .party_lead
            .as_ref()
            .expect("the out-of-signature save still has its lead")
            .is_fainted(),
        "a fainted lead without the var-at-2 signature stays fainted"
    );

    // Boundary three: a damaged-but-standing lead is genuine
    // mid-playthrough state, not the legacy marker, and keeps its spent HP
    // even inside the rest of the signature.
    let damaged = a_damaged_lead();
    let expected_hp = damaged.current_hp();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &damaged);
    let resumed = resume(seed, Some(2));
    assert_eq!(
        resumed
            .party_lead
            .as_ref()
            .expect("the damaged save still has its lead")
            .current_hp(),
        expected_hp,
        "a standing lead's spent HP survives the continue untouched"
    );
}
