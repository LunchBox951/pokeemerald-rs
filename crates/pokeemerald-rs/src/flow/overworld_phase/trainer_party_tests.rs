//! The Route 103 rival hands over and files back the whole saved party: a lead that faints
//! with a healthy saved reserve behind it sends the reserve out, nothing whites out until
//! the party is exhausted, and every member that entered returns to its own saved slot
//! (`Cmd_checkteamslost`, `pokeemerald/src/battle_script_commands.c:3534-3564`;
//! `CB2_EndTrainerBattle`, `pokeemerald/src/battle_setup.c:1319-1346`). The sight-trainer
//! seam over the same fixtures is `sight_trainer_tests::party`.

use assets::{MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, Status1};
use engine::rng::Rng;
use engine::save::Pokemon;
use platform::{ButtonState, Buttons};

use super::route103_rival_test_support::{
    overwhelming_treecko_lead, route_103_phase_facing_the_rival, FLAG_HIDE_ROUTE_103_RIVAL,
    RIVAL_TILE, ROUTE_103,
};
use super::test_support::pressed;
use super::{ActiveBattle, OverworldPhase};
use crate::flow::save_continue_support::dormant_party_member;
use crate::party;

const TREECKO: SpeciesId = SpeciesId(277);
const MUDKIP: SpeciesId = SpeciesId(283);
const VOLTORB: SpeciesId = SpeciesId(100);
const SPLASH: MoveId = MoveId::SPLASH;
const SLASH: MoveId = MoveId::SLASH;
/// The seed the existing rival-loss fixture uses.
pub(super) const RIVAL_SEED: u32 = 2024;
/// `TRAINER_MAY_ROUTE_103_TREECKO`: the rival a fresh phase fights.
pub(super) const RIVAL_TRAINER: u16 = 532;
const IS_EGG_BIT: u32 = 1 << 30;

fn mon(species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), species, level, ivs, 0, moves)
        .expect("the fixture species and moves are in the dex")
}

/// A level-1 member on its last hit point, poisoned and knowing only Splash: it cannot win
/// and cannot survive its first turn.
pub(super) fn frail(species: SpeciesId) -> BattlePokemon {
    let mut member = mon(species, 1, vec![SPLASH]);
    member.set_status1(Status1::Poisoned);
    member.apply_damage(member.stats().max_hp - 1);
    member
}

/// A healthy level-50 Treecko with Slash and two of its PP spent.
pub(super) fn healthy_reserve() -> BattlePokemon {
    let mut member = mon(TREECKO, 50, vec![SLASH]);
    for _ in 0..2 {
        member.deduct_pp(0).unwrap();
    }
    member
}

pub(super) fn saved(member: &BattlePokemon) -> Pokemon {
    party::to_save_pokemon(&Dex::new(), member)
}

pub(super) fn decoded(record: &Pokemon) -> BattlePokemon {
    party::from_save_pokemon(&Dex::new(), record).expect("the record decodes")
}

fn into_egg(mut record: Pokemon) -> Pokemon {
    let mut substructures = record.box_data.substructures().unwrap();
    let word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap()) | IS_EGG_BIT;
    substructures.misc[4..8].copy_from_slice(&word.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    record
}

/// Files `records` as the saved party (count = their number) of `phase`, backing `lead` by
/// `lead_slot`, with the seeded stream and 2001 money.
pub(super) fn file_party(
    phase: &mut OverworldPhase,
    records: &[Pokemon],
    lead_slot: usize,
    lead: BattlePokemon,
) {
    phase.rng = Rng::new(RIVAL_SEED);
    phase.save1.money = 2001;
    for (slot, record) in records.iter().enumerate() {
        phase.save1.player_party[slot] = *record;
    }
    phase.save1.player_party_count = u8::try_from(records.len()).unwrap();
    phase.set_backed_lead_for_test(lead_slot, records[lead_slot], lead);
}

fn rival_phase(records: &[Pokemon], lead_slot: usize, lead: BattlePokemon) -> OverworldPhase {
    let mut phase = route_103_phase_facing_the_rival();
    file_party(&mut phase, records, lead_slot, lead);
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");
    phase
}

/// The trainer's prize for beating the fixture rival.
pub(super) fn rival_reward() -> u32 {
    battle::trainer_money(battle::trainer_data(assets::trainers::TrainerId(RIVAL_TRAINER)).unwrap())
}

/// The player-side position and member count of the rival battle, if one is active.
fn rival_roster(phase: &OverworldPhase) -> Option<(usize, usize)> {
    let Some(ActiveBattle::Rival { battle, .. }) = phase.active_battle.as_ref() else {
        return None;
    };
    Some((battle.player_position(), battle.player_members().count()))
}

/// Plays frames until the rival battle reports an outcome, returning the Splash PP slot 0
/// had left when the reserve replaced it.
fn play_to_the_end(phase: &mut OverworldPhase, expect_roster: usize) -> u8 {
    let mut slot_zero_pp = None;
    for _ in 0..64 {
        if let Some((position, count)) = rival_roster(phase) {
            assert_eq!(count, expect_roster, "the roster the battle began with");
            if position == 1 && slot_zero_pp.is_none() {
                assert_eq!(
                    phase.save1.money, 2001,
                    "a replacement is not a loss: nothing whites out"
                );
                let Some(ActiveBattle::Rival { battle, .. }) = phase.active_battle.as_ref() else {
                    unreachable!("just matched");
                };
                slot_zero_pp = battle.player_members().next().map(|m| m.moves()[0].pp);
            }
        }
        phase.step(ButtonState::new());
        if phase.rival_battle_outcome().is_some() {
            return slot_zero_pp.expect("the reserve replaced the fainted lead before the end");
        }
    }
    panic!("the headless driver must terminate");
}

/// The acceptance case: a 1 HP lead with a healthy saved reserve sends the reserve out,
/// neither halves money nor warps, and both slots return to their own records.
#[test]
fn a_fainted_rival_lead_sends_out_the_saved_reserve_and_files_both_slots() {
    let lead = frail(TREECKO);
    let reserve = healthy_reserve();
    let reserve_pp = reserve.moves()[0].pp;
    let reserve_experience = reserve.experience();
    let records = [saved(&lead), saved(&reserve)];
    let mut phase = rival_phase(&records, 0, lead);

    let slot_zero_pp = play_to_the_end(&mut phase, 2);

    assert_eq!(
        phase.rival_battle_outcome(),
        Some(BattleOutcome::PlayerWon),
        "the level-50 reserve beats the level-5 rival"
    );
    assert_eq!(phase.save1.money, 2001 + rival_reward());
    assert_eq!(phase.map_id, ROUTE_103, "and nothing warps");
    assert_eq!(phase.player.position(), (RIVAL_TILE.0 - 1, RIVAL_TILE.1));
    assert_eq!(
        phase.save1().event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(true)
    );
    let first = decoded(&phase.save1.player_party[0]);
    assert_eq!(first.current_hp(), 0, "slot 0 keeps the faint");
    assert_eq!(first.moves()[0].pp, slot_zero_pp, "and its spent PP");
    assert_eq!(first.personality(), 0);
    let second = decoded(&phase.save1.player_party[1]);
    assert!(second.current_hp() > 0, "the reserve is still standing");
    assert!(second.moves()[0].pp < reserve_pp, "and Slash was used");
    assert!(second.experience() > reserve_experience);
    assert_eq!(phase.save1.player_party_count, 2);
    assert_eq!(phase.lead_slot_for_test(), Some(1));
    assert_eq!(
        phase.lead_for_test().unwrap().personality(),
        second.personality()
    );
}

/// Upstream's loss is whole-party exhaustion: the first faint sends the second member out
/// with the money untouched, and only once that one faints too does the white-out halve it
/// and heal both stored members.
#[test]
fn an_exhausted_rival_party_still_whites_out() {
    let first = frail(TREECKO);
    // Unpoisoned, so the residual damage that faints the first cannot also take the member
    // that replaces it before a frame passes.
    let mut second = mon(MUDKIP, 1, vec![SPLASH]);
    second.apply_damage(second.stats().max_hp - 1);
    let records = [saved(&first), saved(&second)];
    let mut phase = route_103_phase_facing_the_rival();
    file_party(&mut phase, &records, 0, first);
    // No heal location resolves, so the white-out stops before its warp (no pack needed).
    phase.save1.last_heal_location.map_group = -1;
    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active());

    let mut replaced = false;
    for _ in 0..64 {
        replaced |= rival_roster(&phase).is_some_and(|(position, _)| position == 1);
        if replaced {
            assert_eq!(phase.save1.money, 2001, "the second member is out");
        }
        phase.step(ButtonState::new());
        if phase.rival_battle_outcome().is_some() {
            break;
        }
    }

    assert!(replaced, "the first faint sent the second member out");
    assert_eq!(
        phase.rival_battle_outcome(),
        Some(BattleOutcome::PlayerLost)
    );
    assert_eq!(phase.save1.money, 1000);
    for slot in 0..2 {
        let member = decoded(&phase.save1.player_party[slot]);
        assert_eq!(member.current_hp(), member.stats().max_hp, "slot {slot}");
    }
    assert_eq!(
        phase.save1().event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(false)
    );
}

/// Members the battle never sends out keep their records byte for byte, however the codec
/// would normalise them, like the egg, the undecodable record and the empty slots; and a
/// member that enters is filed under its own slot rather than the lead's.
#[test]
fn members_that_never_enter_a_rival_battle_keep_their_bytes() {
    let lead = overwhelming_treecko_lead();
    let mut unused = saved(&healthy_reserve());
    let mut substructures = unused.box_data.substructures().unwrap();
    substructures.attacks[8] = u8::MAX;
    unused.box_data.set_substructures(&substructures);
    let egg = into_egg(saved(&mon(TREECKO, 5, vec![SLASH])));
    let undecodable = dormant_party_member(0x5a5a_1234);
    let records = [saved(&lead), unused, egg, undecodable];
    let mut phase = rival_phase(&records, 0, lead);
    let before: Vec<_> = (1..6)
        .map(|slot| phase.save1.player_party[slot].to_bytes())
        .collect();

    for _ in 0..64 {
        phase.step(ButtonState::new());
        if phase.rival_battle_outcome().is_some() {
            break;
        }
    }

    assert_eq!(phase.rival_battle_outcome(), Some(BattleOutcome::PlayerWon));
    let after: Vec<_> = (1..6)
        .map(|slot| phase.save1.player_party[slot].to_bytes())
        .collect();
    assert_eq!(after, before);
    assert_eq!(phase.save1.player_party_count, 4);
    assert_eq!(phase.lead_slot_for_test(), Some(0));
}

/// The lead sits in slot 1 behind a fainted slot 0, with an egg between it and its reserve
/// in slot 3: the reserve is filed under slot 3, and the slots that did not enter keep
/// their bytes.
#[test]
fn a_reserve_is_filed_into_its_own_non_contiguous_slot() {
    let lead = frail(TREECKO);
    let mut fainted = healthy_reserve();
    fainted.apply_damage(fainted.stats().max_hp);
    let egg = into_egg(saved(&mon(TREECKO, 5, vec![SLASH])));
    let records = [
        saved(&fainted),
        saved(&lead),
        egg,
        saved(&healthy_reserve()),
    ];
    let mut phase = rival_phase(&records, 1, lead);
    let untouched = [0, 2].map(|slot| phase.save1.player_party[slot].to_bytes());

    let slot_one_pp = play_to_the_end(&mut phase, 2);

    assert_eq!(phase.rival_battle_outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(
        [0, 2].map(|slot| phase.save1.player_party[slot].to_bytes()),
        untouched
    );
    let lead_after = decoded(&phase.save1.player_party[1]);
    assert_eq!(lead_after.current_hp(), 0);
    assert_eq!(lead_after.moves()[0].pp, slot_one_pp);
    let reserve_after = decoded(&phase.save1.player_party[3]);
    assert!(reserve_after.current_hp() > 0);
    assert!(reserve_after.moves()[0].pp < healthy_reserve().moves()[0].pp);
    assert_eq!(phase.lead_slot_for_test(), Some(3));
}

/// A saved member the trainer's party cannot be fought with (a two-ability Voltorb under a
/// `CheckBadMove` trainer) is left out of the roster, keeps its bytes, and is a healthy
/// member the loss then cannot ignore: the fight starts rather than being refused.
#[test]
fn a_reserve_the_trainer_cannot_face_is_left_out_with_its_bytes() {
    let lead = overwhelming_treecko_lead();
    let voltorb = mon(VOLTORB, 20, vec![MoveId::TACKLE]);
    let records = [saved(&lead), saved(&voltorb)];
    let mut phase = rival_phase(&records, 0, lead);
    assert_eq!(
        rival_roster(&phase),
        Some((0, 1)),
        "only the lead is on the roster"
    );
    let before = phase.save1.player_party[1].to_bytes();

    for _ in 0..64 {
        phase.step(ButtonState::new());
        if phase.rival_battle_outcome().is_some() {
            break;
        }
    }

    assert_eq!(phase.rival_battle_outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(phase.save1.player_party[1].to_bytes(), before);
}

/// A lone lead has no reserve to file, but it is a participating member too: its saved slot
/// carries the fight's experience once the battle ends.
#[test]
fn a_lone_rival_lead_is_merged_into_its_saved_slot() {
    let lead = overwhelming_treecko_lead();
    let before = lead.experience();
    let records = [saved(&lead)];
    let mut phase = rival_phase(&records, 0, lead);
    for _ in 0..64 {
        phase.step(ButtonState::new());
        if phase.rival_battle_outcome().is_some() {
            break;
        }
    }
    assert_eq!(phase.rival_battle_outcome(), Some(BattleOutcome::PlayerWon));
    let live = phase.lead_for_test().unwrap().experience();
    assert!(live > before, "the live lead gained experience");
    assert_eq!(
        decoded(&phase.save1.player_party[0]).experience(),
        live,
        "the entered lead's saved slot carries the fight"
    );
}
