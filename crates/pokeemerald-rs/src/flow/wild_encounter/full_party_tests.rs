//! The wild handoff and write-back of the whole saved party: every eligible member enters
//! the battle in its saved slot's order, each one that entered is merged back into its own
//! slot, and the loss only reaches white-out once the whole party is exhausted
//! (`SetBattlePartyIds`, `pokeemerald/src/battle_controllers.c:585-606`;
//! `CB2_EndWildBattle`, `pokeemerald/src/battle_setup.c:602-616`).

use super::test_support::{
    held, player_mon, route_101_phase_with_grass, walk_east_and_land, ENCOUNTER_SEED, ROUTE_101,
};
use crate::flow::overworld_phase::{ActiveBattle, OverworldPhase};
use crate::flow::save_continue_support::dormant_party_member;
use crate::party;
use assets::MoveId;
use battle::{BattlePokemon, Dex};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use engine::save::Pokemon;
use platform::Buttons;

/// `SPECIES_SHUCKLE`: slower at level 2 than the level-2 Wurmple the seeded roll
/// produces, so the driver's run attempt can fail and the Wurmple's reply faints a 1 HP one.
const SHUCKLE: u16 = 213;
/// `SPECIES_TREECKO`: at level 50 it outruns that Wurmple, so its run succeeds.
const TREECKO: u16 = 277;
const IS_EGG_BIT: u32 = 1 << 30;

fn base_pound_pp() -> u8 {
    Dex::new().move_data(MoveId::POUND).unwrap().pp
}

/// A level-2 Shuckle on its last hit point with one Pound PP spent.
fn frail_shuckle() -> BattlePokemon {
    let mut mon = player_mon(SHUCKLE, 2, vec![MoveId::POUND]);
    mon.apply_damage(mon.stats().max_hp - 1);
    mon.deduct_pp(0).unwrap();
    mon
}

/// A healthy level-50 Treecko with `spent` Pound PP used.
fn healthy_treecko(spent: u8) -> BattlePokemon {
    let mut mon = player_mon(TREECKO, 50, vec![MoveId::POUND]);
    for _ in 0..spent {
        mon.deduct_pp(0).unwrap();
    }
    mon
}

fn fainted(mut mon: BattlePokemon) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp);
    mon
}

fn saved(mon: &BattlePokemon) -> Pokemon {
    party::to_save_pokemon(&Dex::new(), mon)
}

fn into_egg(mut record: Pokemon) -> Pokemon {
    let mut substructures = record.box_data.substructures().unwrap();
    let word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap()) | IS_EGG_BIT;
    substructures.misc[4..8].copy_from_slice(&word.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    record
}

fn decoded(record: &Pokemon) -> BattlePokemon {
    party::from_save_pokemon(&Dex::new(), record).expect("the record decodes")
}

/// A Route 101 room with the seeded grass at `(7, 5)` and money 2001, whose saved party
/// is `records` (count = their number) and whose lead is `lead` backed by `lead_slot`.
fn phase_with_party(records: &[Pokemon], lead_slot: usize, lead: BattlePokemon) -> OverworldPhase {
    let mut phase = route_101_phase_with_grass(
        20,
        10,
        PlayerState::new((2, 5), 3, Direction::East),
        &[(7, 5)],
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.save1.money = 2001;
    for (slot, record) in records.iter().enumerate() {
        phase.save1.player_party[slot] = *record;
    }
    phase.save1.player_party_count = u8::try_from(records.len()).unwrap();
    phase.set_backed_lead_for_test(lead_slot, records[lead_slot], lead);
    phase
}

fn walk_into_the_battle(phase: &mut OverworldPhase) {
    walk_east_and_land(phase, 5);
    assert_eq!(phase.player.position(), (7, 5));
    assert!(phase.is_wild_battle_active(), "the seeded roll fires");
}

fn finish_the_battle(phase: &mut OverworldPhase) {
    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
}

/// The lead faints with a healthy saved reserve behind it. The
/// party is not defeated, so the battle goes on with the reserve and nothing whites out.
#[test]
fn a_lead_faint_with_a_healthy_reserve_neither_halves_money_nor_warps() {
    let base_pp = base_pound_pp();
    let lead = frail_shuckle();
    let reserve = healthy_treecko(2);
    let mut phase = phase_with_party(&[saved(&lead), saved(&reserve)], 0, lead);
    let reserve_experience = reserve.experience();

    walk_into_the_battle(&mut phase);
    finish_the_battle(&mut phase);

    assert_eq!(phase.save1.money, 2001, "no white-out halves the money");
    assert_eq!(
        phase.player.position(),
        (7, 5),
        "and no warp moves the player"
    );
    assert_eq!(phase.map_id, ROUTE_101);
    let first = decoded(&phase.save1.player_party[0]);
    assert_eq!(
        first.current_hp(),
        0,
        "slot 0 keeps the faint that sent the reserve out"
    );
    assert_eq!(first.moves()[0].pp, base_pp - 1, "and its spent PP");
    let second = decoded(&phase.save1.player_party[1]);
    assert_eq!(second.current_hp(), second.stats().max_hp);
    assert_eq!(second.moves()[0].pp, base_pp - 2);
    assert_eq!(second.experience(), reserve_experience);
    assert_eq!(
        phase.lead_slot_for_test(),
        Some(1),
        "the field lead is the first usable member"
    );
    assert_eq!(
        phase.lead_for_test().unwrap().personality(),
        reserve.personality()
    );
}

/// Upstream's loss is whole-party exhaustion: the first faint sends the second member out
/// with the money untouched, and once that one faints too the battle is `PlayerLost` and the
/// white-out halves the money and heals both stored members.
///
/// The second member's run attempt gets its own stream: `RESERVE_RUN_SEED` is one whose
/// attempt fails (`TryRunFromBattle`'s RNG branch), so the Wurmple's reply faints it.
#[test]
fn an_exhausted_party_still_whites_out() {
    const RESERVE_RUN_SEED: u32 = 1;
    let base_pp = base_pound_pp();
    let first = frail_shuckle();
    let mut second = player_mon(SHUCKLE, 2, vec![MoveId::POUND]);
    second.apply_damage(second.stats().max_hp - 1);
    let mut phase = phase_with_party(&[saved(&first), saved(&second)], 0, first);
    phase.save1.last_heal_location.map_group = -1;

    walk_into_the_battle(&mut phase);
    loop {
        phase.step(held(Buttons::RIGHT));
        let Some(ActiveBattle::Wild(wild)) = phase.active_battle.as_ref() else {
            panic!("the first faint must not end the battle");
        };
        if wild.battle.player().moves()[0].pp == base_pp {
            break;
        }
    }
    assert_eq!(
        phase.save1.money, 2001,
        "the second member is out, nothing whited out"
    );
    phase.rng = Rng::new(RESERVE_RUN_SEED);
    finish_the_battle(&mut phase);

    assert_eq!(phase.save1.money, 1000);
    for slot in 0..2 {
        let member = decoded(&phase.save1.player_party[slot]);
        assert_eq!(
            member.current_hp(),
            member.stats().max_hp,
            "slot {slot} is healed"
        );
    }
    assert_eq!(decoded(&phase.save1.player_party[0]).moves()[0].pp, base_pp);
}

/// A member that never leaves the party screen keeps its record byte for byte, however the
/// codec would normalise it -- like the egg, the undecodable record and the empty slots.
#[test]
fn members_that_never_enter_the_battle_keep_their_bytes() {
    let lead = healthy_treecko(0);
    let mut unused = saved(&frail_shuckle());
    let mut substructures = unused.box_data.substructures().unwrap();
    substructures.attacks[8] = u8::MAX;
    unused.box_data.set_substructures(&substructures);
    let egg = into_egg(saved(&healthy_treecko(0)));
    let undecodable = dormant_party_member(0x5a5a_1234);
    let records = [saved(&lead), unused, egg, undecodable];
    let mut phase = phase_with_party(&records, 0, lead);
    let before: Vec<_> = (1..6)
        .map(|slot| phase.save1.player_party[slot].to_bytes())
        .collect();

    walk_into_the_battle(&mut phase);
    finish_the_battle(&mut phase);

    let after: Vec<_> = (1..6)
        .map(|slot| phase.save1.player_party[slot].to_bytes())
        .collect();
    assert_eq!(after, before);
    assert_eq!(phase.save1.player_party_count, 4);
    assert_eq!(phase.lead_slot_for_test(), Some(0));
}

/// The battle's order is the active member first, then the reserves in slot order, so the
/// write-back follows the position, not the slot index: the active member is slot 1 here and
/// the reserve that enters is slot 3, with a fainted slot 0 and an egg at slot 2 between.
#[test]
fn an_entering_reserve_merges_into_its_own_slot() {
    let base_pp = base_pound_pp();
    let first = fainted(frail_shuckle());
    let lead = frail_shuckle();
    let egg = into_egg(saved(&healthy_treecko(0)));
    let reserve = healthy_treecko(1);
    let records = [saved(&first), saved(&lead), egg, saved(&reserve)];
    let mut phase = phase_with_party(&records, 1, lead);

    walk_into_the_battle(&mut phase);
    finish_the_battle(&mut phase);

    assert_eq!(
        phase.save1.player_party[0].to_bytes(),
        records[0].to_bytes()
    );
    assert_eq!(
        phase.save1.player_party[2].to_bytes(),
        records[2].to_bytes()
    );
    let entered_first = decoded(&phase.save1.player_party[1]);
    assert_eq!(entered_first.current_hp(), 0);
    let entered_reserve = decoded(&phase.save1.player_party[3]);
    assert_eq!(entered_reserve.moves()[0].pp, base_pp - 1);
    assert_eq!(phase.lead_slot_for_test(), Some(3));
    assert_eq!(phase.save1.money, 2001);
}

/// A reserve the battle engine cannot play yet (a Truant Slakoth) is left out of the roster
/// rather than refusing the encounter the playable lead can fight, and keeps its bytes.
#[test]
fn an_unplayable_reserve_is_left_out_without_refusing_the_battle() {
    let lead = healthy_treecko(0);
    let slakoth = player_mon(assets::SpeciesId::SLAKOTH.0, 50, vec![MoveId::SCRATCH]);
    assert!(battle::ensure_participant_admissible(&slakoth).is_err());
    let records = [saved(&lead), saved(&slakoth)];
    let mut phase = phase_with_party(&records, 0, lead);

    walk_into_the_battle(&mut phase);
    let Some(ActiveBattle::Wild(wild)) = phase.active_battle.as_ref() else {
        panic!("the battle starts");
    };
    assert_eq!(wild.battle.player_members().count(), 1);
    finish_the_battle(&mut phase);

    assert_eq!(
        phase.save1.player_party[1].to_bytes(),
        records[1].to_bytes()
    );
    assert_eq!(phase.save1.money, 2001);
}

/// A saved member beyond the stored count is still inspected, as `SetBattlePartyIds`
/// scans all six slots, so a healthy record there is a reserve like any other.
#[test]
fn a_reserve_beyond_the_stored_count_enters() {
    let first = frail_shuckle();
    let beyond = healthy_treecko(0);
    let mut phase = phase_with_party(&[saved(&first)], 0, first);
    phase.save1.player_party[3] = saved(&beyond);

    walk_into_the_battle(&mut phase);
    finish_the_battle(&mut phase);

    assert_eq!(phase.save1.money, 2001);
    assert_eq!(phase.save1.player_party_count, 1);
    assert_eq!(phase.lead_slot_for_test(), Some(3));
}

/// Upstream's party HP total (`Cmd_checkteamslost`, `src/battle_script_commands.c:3534-3564`)
/// still counts a healthy member the roster left out, so the lead's faint is not a defeat:
/// nothing whites out, and that member becomes the field lead.
#[test]
fn a_lead_faint_with_only_an_unplayable_member_left_does_not_white_out() {
    let lead = frail_shuckle();
    let slakoth = player_mon(assets::SpeciesId::SLAKOTH.0, 50, vec![MoveId::SCRATCH]);
    let records = [saved(&lead), saved(&slakoth)];
    let mut phase = phase_with_party(&records, 0, lead);

    walk_into_the_battle(&mut phase);
    finish_the_battle(&mut phase);

    assert_eq!(phase.save1.money, 2001);
    assert_eq!(phase.player.position(), (7, 5));
    assert_eq!(decoded(&phase.save1.player_party[0]).current_hp(), 0);
    assert_eq!(
        phase.save1.player_party[1].to_bytes(),
        records[1].to_bytes()
    );
    assert_eq!(phase.lead_slot_for_test(), Some(1));
}

/// A reserve whose secure checksum fails is a Bad Egg upstream (`GetBoxMonData`,
/// `pokeemerald/src/pokemon.c:3742-3747`), which `Cmd_checkteamslost` leaves out of its
/// HP total, so once the lead faints the party is defeated and whites out.
#[test]
fn a_checksum_corrupted_reserve_does_not_prevent_white_out() {
    let lead = frail_shuckle();
    let mut bytes = saved(&healthy_treecko(0)).to_bytes();
    bytes[28] ^= 0xff;
    let corrupt = Pokemon::from_bytes(bytes);
    assert!(corrupt.box_data.has_species() && corrupt.hp != 0);
    assert!(corrupt.box_data.substructures().is_err());
    let mut phase = phase_with_party(&[saved(&lead), corrupt], 0, lead);
    phase.save1.last_heal_location.map_group = -1;

    walk_into_the_battle(&mut phase);
    finish_the_battle(&mut phase);

    assert_eq!(phase.save1.money, 1000, "the defeated party whites out");
}
