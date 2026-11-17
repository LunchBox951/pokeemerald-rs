//! Pickup after a won battle: the saved party's facts, the exact draws, the
//! item landing in the saved record and the live lead, and the endings that
//! must not roll.
//!
//! The draws are pinned against `Rng::new(0)`, whose first values are `0`,
//! `59774`, `21105`, `12720` (`engine::rng`'s own known sequence): a hit with
//! percentile `74`, a miss (`21105 % 10 == 5`), then a hit again. At percentile
//! `74` the lookup reads the sixth common item of the level's tier.

use assets::items::ItemId;
use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs};
use engine::rng::Rng;
use engine::save::Pokemon;

use crate::flow::overworld_phase::OverworldPhase;
use crate::flow::save_continue_support::{new_game_phase, save_from_the_start_menu};
use crate::flow::tests::TempSave;
use crate::party;

const TREECKO: SpeciesId = SpeciesId(277);
const ZIGZAGOON: SpeciesId = SpeciesId(288);
/// Abilities `[RUN_AWAY, PICKUP]`: the only species here whose Pickup is its
/// second ability.
const AIPOM: SpeciesId = SpeciesId(190);

/// The state of a stream that has made `draws` draws from `seed`.
fn rng_after(seed: u32, draws: usize) -> u32 {
    let mut rng = Rng::new(seed);
    for _ in 0..draws {
        rng.next_u16();
    }
    rng.state()
}

fn mon(species: SpeciesId, level: u8, trainer_id: u32) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), species, level, ivs, 0, vec![MoveId::POUND])
        .expect("the species and move are in the dex")
        .with_original_trainer_id(trainer_id)
}

fn record(mon: &BattlePokemon) -> Pokemon {
    party::to_save_pokemon(&Dex::new(), mon)
}

/// Rewrites a record's secure region through `edit`.
fn edit_secure(
    mut record: Pokemon,
    edit: impl FnOnce(&mut engine::save::PokemonSubstructures),
) -> Pokemon {
    let mut substructures = record.box_data.substructures().expect("a valid record");
    edit(&mut substructures);
    record.box_data.set_substructures(&substructures);
    record
}

fn with_item(record: Pokemon, item: ItemId) -> Pokemon {
    edit_secure(record, |s| {
        s.growth[2..4].copy_from_slice(&item.0.to_le_bytes());
    })
}

fn held_item_of(record: &Pokemon) -> ItemId {
    let substructures = record.box_data.substructures().expect("a valid record");
    ItemId(u16::from_le_bytes([
        substructures.growth[2],
        substructures.growth[3],
    ]))
}

fn with_ability_slot(record: Pokemon, second: bool) -> Pokemon {
    edit_secure(record, |s| {
        let word = u32::from_le_bytes(s.misc[4..8].try_into().unwrap());
        let word = if second {
            word | (1 << 31)
        } else {
            word & !(1 << 31)
        };
        s.misc[4..8].copy_from_slice(&word.to_le_bytes());
    })
}

fn with_egg_flag(record: Pokemon) -> Pokemon {
    edit_secure(record, |s| {
        let word = u32::from_le_bytes(s.misc[4..8].try_into().unwrap()) | (1 << 30);
        s.misc[4..8].copy_from_slice(&word.to_le_bytes());
    })
}

fn trainer_id(phase: &OverworldPhase) -> u32 {
    u32::from_le_bytes(phase.save2.player_trainer_id)
}

/// A phase whose saved party is `party` (slot 0 first) and whose lead is
/// `lead`, backed by slot `lead_slot`; the stream is `Rng::new(seed)`.
fn phase_with_party(
    party: &[Pokemon],
    lead_slot: usize,
    lead: &BattlePokemon,
    seed: u32,
) -> OverworldPhase {
    let mut phase = new_game_phase();
    phase.save1.player_party = Default::default();
    for (slot, record) in party.iter().enumerate() {
        phase.save1.player_party[slot] = *record;
    }
    phase.save1.player_party_count = u8::try_from(party.len()).unwrap();
    phase.set_backed_lead_for_test(lead_slot, party[lead_slot], lead.clone());
    phase.rng = Rng::new(seed);
    phase
}

#[test]
fn a_lone_lead_that_finds_an_item_holds_it_through_save_and_continue() {
    let temp = TempSave::new("pickup-lone-lead");
    let mut slot = temp.slot();
    let mut phase = new_game_phase();
    let owner = trainer_id(&phase);
    phase.set_fresh_lead_for_test(mon(ZIGZAGOON, 5, owner));
    phase.rng = Rng::new(0);

    phase.roll_pickup_after_win();

    // Draw 0 is a hit, draw 1 is percentile 74: tier 0's sixth common item.
    assert_eq!(
        phase.lead_for_test().unwrap().held_item(),
        ItemId::ESCAPE_ROPE
    );
    assert_eq!(phase.save1.player_party_count, 1);
    assert_eq!(
        held_item_of(&phase.save1.player_party[0]),
        ItemId::ESCAPE_ROPE
    );
    assert_eq!(phase.rng.state(), rng_after(0, 2));

    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    assert_eq!(
        held_item_of(&saved.block1.player_party[0]),
        ItemId::ESCAPE_ROPE
    );

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(phase.map_id),
        phase.map_id,
        saved.block1,
        saved.block2,
    );
    assert_eq!(
        resumed.lead_for_test().unwrap().held_item(),
        ItemId::ESCAPE_ROPE,
        "Continue brings the item back on the member"
    );
    // The next save writes the live member back over its record; it must not
    // write the old, empty hand.
    save_from_the_start_menu(&mut resumed, &mut slot);
    assert_eq!(
        held_item_of(&slot.load().block1.player_party[0]),
        ItemId::ESCAPE_ROPE
    );
}

#[test]
fn a_fainted_reserve_that_never_fought_finds_an_item_in_its_own_record() {
    let temp = TempSave::new("pickup-reserve");
    let mut slot = temp.slot();
    let owner = trainer_id(&new_game_phase());
    let lead = mon(TREECKO, 20, owner);
    let mut reserve = mon(ZIGZAGOON, 50, owner);
    reserve.apply_damage(u32::MAX);
    assert!(reserve.is_fainted());
    let lead_record = record(&lead);
    let reserve_record = record(&reserve);
    let mut phase = phase_with_party(
        &[lead_record, Pokemon::default(), reserve_record],
        0,
        &lead,
        0,
    );

    phase.roll_pickup_after_win();

    // The Treecko lead has no Pickup and draws nothing; the fainted level-50
    // Zigzagoon draws twice. Level 50 is tier 4, so the sixth step is
    // sPickupItems[4 + 5], Hyper Potion.
    assert_eq!(phase.rng.state(), rng_after(0, 2));
    assert_eq!(
        held_item_of(&phase.save1.player_party[2]),
        ItemId::HYPER_POTION
    );
    assert_eq!(
        phase.save1.player_party[0], lead_record,
        "the lead is untouched"
    );
    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::NONE);
    // Everything but the held item survives, and the checksum is valid.
    let mut expected = reserve_record;
    expected = with_item(expected, ItemId::HYPER_POTION);
    assert_eq!(phase.save1.player_party[2], expected);

    save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        held_item_of(&slot.load().block1.player_party[2]),
        ItemId::HYPER_POTION
    );
}

#[test]
fn slots_draw_in_party_order_and_a_miss_does_not_stop_later_slots() {
    let owner = trainer_id(&new_game_phase());
    let lead = mon(ZIGZAGOON, 5, owner);
    let second = mon(ZIGZAGOON, 5, owner);
    let mut phase = phase_with_party(&[record(&lead), record(&second)], 0, &lead, 0);

    phase.roll_pickup_after_win();

    // Slot 0: draws 0 (hit), 59774 (percentile 74). Slot 1: draw 21105, a miss.
    assert_eq!(phase.rng.state(), rng_after(0, 3));
    assert_eq!(
        held_item_of(&phase.save1.player_party[0]),
        ItemId::ESCAPE_ROPE
    );
    assert_eq!(held_item_of(&phase.save1.player_party[1]), ItemId::NONE);
}

#[test]
fn every_one_of_the_six_slots_rolls_whatever_the_stored_party_count() {
    let owner = trainer_id(&new_game_phase());
    let lead = mon(TREECKO, 5, owner);
    let mut phase = phase_with_party(&[record(&lead)], 0, &lead, 0);
    phase.save1.player_party[5] = record(&mon(ZIGZAGOON, 5, owner));

    phase.roll_pickup_after_win();

    assert_eq!(phase.rng.state(), rng_after(0, 2));
    assert_eq!(
        held_item_of(&phase.save1.player_party[5]),
        ItemId::ESCAPE_ROPE
    );
}

#[test]
fn ineligible_members_draw_nothing_and_change_nothing() {
    let owner = trainer_id(&new_game_phase());
    let lead = mon(TREECKO, 5, owner);
    let pickup = record(&mon(ZIGZAGOON, 5, owner));
    let mut broken = pickup;
    // Corrupt one secure byte so the checksum no longer matches: upstream
    // reads such a slot as an egg.
    let mut bytes = broken.box_data.to_bytes();
    bytes[40] ^= 0xFF;
    broken.box_data = engine::save::BoxPokemon::from_bytes(bytes);
    let mut bad_egg = pickup;
    let mut bytes = bad_egg.box_data.to_bytes();
    bytes[19] |= 1 << 0; // the header's `isBadEgg`
    bad_egg.box_data = engine::save::BoxPokemon::from_bytes(bytes);

    let party = [
        record(&lead),
        with_egg_flag(pickup),
        with_item(pickup, ItemId::POTION),
        broken,
        bad_egg,
        // A second ability slot of a species with no second ability.
        with_ability_slot(pickup, true),
    ];
    let mut phase = phase_with_party(&party, 0, &lead, 0);
    let before = phase.save1.player_party;

    phase.roll_pickup_after_win();

    assert_eq!(phase.rng.state(), Rng::new(0).state(), "not one draw");
    assert_eq!(phase.save1.player_party, before);
    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::NONE);
}

#[test]
fn the_ability_is_the_species_entry_at_the_stored_ability_slot() {
    let owner = trainer_id(&new_game_phase());
    let lead = mon(TREECKO, 5, owner);
    let aipom = record(&mon(AIPOM, 5, owner));

    // Aipom's abilities are [Run Away, Pickup]: the first slot does not roll,
    let mut first = phase_with_party(
        &[record(&lead), with_ability_slot(aipom, false)],
        0,
        &lead,
        0,
    );
    first.roll_pickup_after_win();
    assert_eq!(first.rng.state(), Rng::new(0).state());

    // and the second does.
    let mut second = phase_with_party(
        &[record(&lead), with_ability_slot(aipom, true)],
        0,
        &lead,
        0,
    );
    second.roll_pickup_after_win();
    assert_eq!(second.rng.state(), rng_after(0, 2));
    assert_eq!(
        held_item_of(&second.save1.player_party[1]),
        ItemId::ESCAPE_ROPE
    );
}

#[test]
fn a_party_with_no_pickup_holder_leaves_the_stream_and_the_save_alone() {
    let mut phase = new_game_phase();
    let before = phase.save1.player_party;
    let count = phase.save1.player_party_count;
    phase.rng = Rng::new(0x1234_5678);

    phase.roll_pickup_after_win();

    assert_eq!(phase.rng.state(), Rng::new(0x1234_5678).state());
    assert_eq!(phase.save1.player_party, before);
    assert_eq!(phase.save1.player_party_count, count);
}

#[test]
fn a_lead_that_leveled_in_battle_rolls_in_its_new_tier() {
    let owner = trainer_id(&new_game_phase());
    // The saved record is level 10 (tier 0); the battle brought the live lead
    // to level 11 (tier 1), where percentile 74 is sPickupItems[1 + 5].
    let saved = mon(ZIGZAGOON, 10, owner);
    let live = mon(ZIGZAGOON, 11, owner);
    let mut phase = phase_with_party(&[record(&saved)], 0, &live, 0);

    phase.roll_pickup_after_win();

    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::X_ATTACK);
    assert_eq!(held_item_of(&phase.save1.player_party[0]), ItemId::X_ATTACK);
}

#[test]
fn a_lead_whose_battle_item_was_used_up_can_pick_up_again() {
    let owner = trainer_id(&new_game_phase());
    let saved = with_item(record(&mon(ZIGZAGOON, 5, owner)), ItemId::POTION);
    // The live battler no longer holds the item the record still lists.
    let live = mon(ZIGZAGOON, 5, owner);
    let mut phase = phase_with_party(&[saved], 0, &live, 0);

    phase.roll_pickup_after_win();

    assert_eq!(
        phase.lead_for_test().unwrap().held_item(),
        ItemId::ESCAPE_ROPE
    );
}

// -- The endings that roll, and the ones that do not --

/// A wild battle between a level-50 `lead` and a level-2 Wurmple, driven on a
/// private stream to the ending `run` chooses (a fought-out win, or a run),
/// and handed to the phase's own wild-battle frame with the phase's stream at
/// `Rng::new(0)`.
///
/// The ended battle's frame takes one more turn on it, which fails without a
/// draw, and then files it as production does.
fn finish_wild_battle(phase: &mut OverworldPhase, run: bool) -> Option<battle::BattleOutcome> {
    use crate::flow::overworld_phase::WildBattle;
    use crate::flow::wild_encounter::{start_wild_battle, SharedRng};

    let owner = trainer_id(phase);
    let lent = phase
        .take_lead_for_battle()
        .expect("the phase has a lead to lend");
    let mut private = Rng::new(0x00C0_FFEE);
    let mut battle = start_wild_battle(
        lent,
        engine::overworld::WildEncounter {
            species: SpeciesId(290),
            level: 2,
            slot: 0,
        },
        owner,
        &mut private,
    )
    .expect("a Wurmple is fightable");
    let action = if run {
        battle::PlayerAction::Run
    } else {
        battle::PlayerAction::UseMove(0)
    };
    for _ in 0..100 {
        if battle.outcome().is_some() {
            break;
        }
        battle
            .take_turn(action, &mut SharedRng::new(&mut private))
            .expect("the turn resolves");
    }
    let outcome = battle.outcome();
    phase.rng = Rng::new(0);
    let wild = WildBattle::without_reserves(battle);
    assert!(phase
        .advance_wild_battle_frame(wild, platform::ButtonState::new())
        .is_none());
    outcome
}

fn wild_phase() -> OverworldPhase {
    let mut phase = new_game_phase();
    let owner = trainer_id(&phase);
    phase.set_fresh_lead_for_test(mon(ZIGZAGOON, 50, owner));
    phase
}

#[test]
fn a_won_wild_battle_rolls_pickup_for_the_party() {
    let mut phase = wild_phase();

    let outcome = finish_wild_battle(&mut phase, false);

    assert_eq!(outcome, Some(battle::BattleOutcome::PlayerWon));
    // Seed 0: a hit, then percentile 74 at level 50 (tier 4).
    assert_eq!(phase.rng.state(), rng_after(0, 2));
    assert_eq!(
        phase.lead_for_test().unwrap().held_item(),
        ItemId::HYPER_POTION
    );
    assert_eq!(
        held_item_of(&phase.save1.player_party[0]),
        ItemId::HYPER_POTION
    );
}

#[test]
fn a_fled_wild_battle_rolls_nothing() {
    let mut phase = wild_phase();

    let outcome = finish_wild_battle(&mut phase, true);

    assert_eq!(outcome, Some(battle::BattleOutcome::PlayerRan));
    assert_eq!(phase.rng.state(), Rng::new(0).state(), "not one draw");
    assert_eq!(phase.lead_for_test().unwrap().held_item(), ItemId::NONE);
}

// -- The trainer endings --
//
// Each runs the same battle twice, once with the lead holding a Potion, which
// makes it ineligible, and once with an empty hand. The two battles draw
// identically, so the stream the Potion run ends on is the one Pickup starts
// from, and the other run must differ from it by Pickup's draws alone.

use super::super::route103_rival_test_support::{
    play_out_rival_battle, route_103_phase_facing_the_rival, route_103_phase_facing_the_rival_repo,
};
use crate::flow::tests::pressed;

/// Plays the Route 103 rival battle for a level-`level` Zigzagoon lead from
/// `Rng::new(seed)`, and returns the phase once the battle has ended.
fn rival_battle(
    level: u8,
    holding: ItemId,
    seed: u32,
    repo_pack: bool,
) -> (OverworldPhase, battle::BattleOutcome) {
    let mut phase = if repo_pack {
        route_103_phase_facing_the_rival_repo()
    } else {
        route_103_phase_facing_the_rival()
    };
    let owner = trainer_id(&phase);
    phase.set_fresh_lead_for_test(mon(ZIGZAGOON, level, owner).with_held_item(holding));
    phase.rng = Rng::new(seed);
    phase.step(pressed(platform::Buttons::A));
    let outcome = play_out_rival_battle(&mut phase, 64).expect("the battle ends");
    (phase, outcome)
}

fn after_draws(rng: &Rng, draws: usize) -> u32 {
    let mut rng = rng.clone();
    for _ in 0..draws {
        rng.next_u16();
    }
    rng.state()
}

#[test]
fn a_won_rival_battle_rolls_pickup_after_the_prize_money() {
    // Seed 12 ends the battle on a stream whose next two draws are 1320 (a hit)
    // and 34005 (percentile 5): level 50 is tier 4, first step, Repel.
    let (without, outcome) = rival_battle(50, ItemId::POTION, 12, false);
    assert_eq!(outcome, battle::BattleOutcome::PlayerWon);
    assert_eq!(without.lead_for_test().unwrap().held_item(), ItemId::POTION);

    let (with, outcome) = rival_battle(50, ItemId::NONE, 12, false);
    assert_eq!(outcome, battle::BattleOutcome::PlayerWon);
    assert_eq!(with.rng.state(), after_draws(&without.rng, 2));
    assert_eq!(with.lead_for_test().unwrap().held_item(), ItemId::REPEL);
    assert_eq!(held_item_of(&with.save1.player_party[0]), ItemId::REPEL);
    assert_eq!(
        with.save1.money, without.save1.money,
        "Pickup does not touch the prize money"
    );
}

#[test]
fn a_won_rival_battle_whose_pickup_misses_draws_once() {
    // Seed 0 ends on a stream whose next draw is 39229, a miss.
    let (without, _) = rival_battle(50, ItemId::POTION, 0, false);
    let (with, outcome) = rival_battle(50, ItemId::NONE, 0, false);
    assert_eq!(outcome, battle::BattleOutcome::PlayerWon);
    assert_eq!(with.rng.state(), after_draws(&without.rng, 1));
    assert_eq!(with.lead_for_test().unwrap().held_item(), ItemId::NONE);
}

#[test]
fn a_lost_rival_battle_rolls_nothing() {
    // Seed 0 ends on a stream whose next draw, 16580, would be a hit.
    let (without, outcome) = rival_battle(1, ItemId::POTION, 0, true);
    assert_eq!(outcome, battle::BattleOutcome::PlayerLost);
    let (with, outcome) = rival_battle(1, ItemId::NONE, 0, true);
    assert_eq!(outcome, battle::BattleOutcome::PlayerLost);
    assert_eq!(with.rng.state(), without.rng.state(), "not one draw");
    assert_eq!(with.lead_for_test().unwrap().held_item(), ItemId::NONE);
    assert_eq!(held_item_of(&with.save1.player_party[0]), ItemId::NONE);
}

#[test]
fn the_first_battle_conclusion_rolls_pickup_only_for_a_win() {
    use battle::BattleOutcome;
    for (outcome, rolls) in [
        (BattleOutcome::PlayerWon, true),
        (BattleOutcome::PlayerLost, false),
        (BattleOutcome::WildFled, false),
    ] {
        let mut phase = wild_phase();
        phase.rng = Rng::new(0);
        phase.first_battle_outcome = Some(outcome);
        phase.conclude_first_battle();
        assert_eq!(
            phase.rng.state(),
            rng_after(0, if rolls { 2 } else { 0 }),
            "{outcome:?}"
        );
        let expected = if rolls {
            ItemId::HYPER_POTION
        } else {
            ItemId::NONE
        };
        assert_eq!(
            phase.lead_for_test().unwrap().held_item(),
            expected,
            "{outcome:?}"
        );
    }
}

// -- Items a battle cannot carry yet are withheld, not stored --

/// King's Rock, White Herb and Leftovers: the rare table entries
/// `battle::ensure_held_item_playable` refuses today, each with the
/// `(level, percentile)` pairs that select it (`rare[tier + 99 - percentile]`,
/// tier `(level - 1) / 10`).
const WITHHELD: [(ItemId, [(u8, u16); 2]); 3] = [
    (ItemId::KINGS_ROCK, [(15, 98), (25, 99)]),
    (ItemId::WHITE_HERB, [(45, 98), (55, 99)]),
    (ItemId::LEFTOVERS, [(85, 98), (95, 99)]),
];

/// The streams whose first two draws are a Pickup hit and the named percentile:
/// `Rng::new(195)` draws 6670, 11098 and `Rng::new(3453)` draws 12240, 13499.
fn seed_for_percentile(percentile: u16) -> u32 {
    match percentile {
        98 => 195,
        99 => 3453,
        other => panic!("no pinned seed for percentile {other}"),
    }
}

#[test]
fn withheld_items_are_exactly_those_the_battle_refuses() {
    // When a battle learns an item's effect it is admitted, and this fails:
    // drop that item from `WITHHELD` and from the module docs.
    let tables = assets::pickup::PickupTables::new();
    let mut refused = std::collections::BTreeMap::new();
    for tier in 0..10u8 {
        for percentile in 0..100u8 {
            let item = tables.lookup(tier, percentile).unwrap().item();
            if battle::ensure_held_item_playable(item).is_err() {
                refused
                    .entry(item)
                    .or_insert_with(Vec::new)
                    .push((tier * 10 + 1, u16::from(percentile)));
            }
        }
    }
    let named: std::collections::BTreeMap<_, _> = WITHHELD
        .iter()
        .map(|(item, pairs)| (*item, pairs.to_vec()))
        .collect();
    // `refused` holds the first level of each tier (11, 21, ...); compare the
    // tier and percentile of every pair named here.
    let tier_of = |level: u8| (level - 1) / 10 * 10 + 1;
    for (item, pairs) in &named {
        let seen = refused.get(item).expect("the item is refused");
        let named_pairs: Vec<_> = pairs.iter().map(|&(l, p)| (tier_of(l), p)).collect();
        assert_eq!(seen, &named_pairs, "{item:?}");
    }
    assert_eq!(
        refused.keys().collect::<Vec<_>>(),
        named.keys().collect::<Vec<_>>(),
        "every refused item is named"
    );
}

#[test]
fn a_found_item_a_battle_cannot_carry_is_withheld_after_both_draws() {
    for (item, pairs) in WITHHELD {
        for (level, percentile) in pairs {
            let seed = seed_for_percentile(percentile);
            let owner = trainer_id(&new_game_phase());
            let lead = mon(TREECKO, 20, owner);
            let reserve_record = record(&mon(ZIGZAGOON, level, owner));
            let mut phase = phase_with_party(&[record(&lead), reserve_record], 0, &lead, seed);

            phase.roll_pickup_after_win();

            assert_eq!(
                phase.rng.state(),
                rng_after(seed, 2),
                "{item:?}: both draws are made"
            );
            assert_eq!(
                phase.save1.player_party[1], reserve_record,
                "{item:?} (level {level}, percentile {percentile}): nothing is stored"
            );
        }
    }
}

#[test]
fn a_lead_that_found_a_refused_item_still_enters_battle() {
    // Level 15, percentile 98: King's Rock. Stored, it would refuse the lead
    // every trainer battle and disable wild encounters.
    let mut phase = new_game_phase();
    let owner = trainer_id(&phase);
    phase.set_fresh_lead_for_test(mon(ZIGZAGOON, 15, owner));
    phase.rng = Rng::new(seed_for_percentile(98));

    phase.roll_pickup_after_win();

    let lead = phase.lead_for_test().unwrap();
    assert_eq!(lead.held_item(), ItemId::NONE);
    assert_eq!(
        crate::flow::lead_screen::ensure_lead_can_battle(lead),
        Ok(())
    );
    assert!(phase.wild_lead_admissible());
}

#[test]
fn a_reserve_that_found_an_item_is_still_playable_after_a_reload() {
    // A reserve is left out of a wild roster when it holds a refused item, so
    // whatever Pickup stores must pass the same screen after the save round
    // trip: level 100, percentile 98 is TM Earthquake, which it does carry.
    let owner = trainer_id(&new_game_phase());
    let lead = mon(TREECKO, 20, owner);
    let reserve_record = record(&mon(ZIGZAGOON, 100, owner));
    let mut phase = phase_with_party(&[record(&lead), reserve_record], 0, &lead, 195);

    phase.roll_pickup_after_win();

    let stored = phase.save1.player_party[1];
    assert_eq!(held_item_of(&stored), ItemId::TM_EARTHQUAKE);
    let reloaded = crate::party::from_save_pokemon(&Dex::new(), &stored).unwrap();
    assert_eq!(
        crate::flow::lead_screen::ensure_lead_can_battle(&reloaded),
        Ok(())
    );
}
