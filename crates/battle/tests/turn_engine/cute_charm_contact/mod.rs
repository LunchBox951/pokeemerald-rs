//! Cute Charm's contact infatuation (`ABILITYEFFECT_ON_DAMAGE`,
//! `battle_util.c:2851`-`:2870`; `BattleScript_CuteCharmActivates`,
//! `data/battle_scripts_1.s:4193`-`:4197`): a `Random() % 3` roll for a living
//! contact attacker that damaged a surviving holder, then the Oblivious,
//! gender and existing-infatuation guards, which follow the roll.
//!
//! `activation` pins the roll and the state it writes, `guards` pins the
//! cases that draw no roll and the guards that still spend it, and
//! `multi_hit` pins the per-strike ordering. This module owns the scripted-draw
//! fixtures they share. Every script is exact: `SequenceRng::assert_exhausted`
//! fails on a missing or an extra draw.

mod activation;
mod guards;
mod multi_hit;

use crate::common::{max_iv_mon_with_personality, SequenceRng};
use assets::{MoveId, SpeciesId};
use battle::battle::BattlerSide;
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction};

const TACKLE: MoveId = MoveId::TACKLE;
const SPLASH: MoveId = MoveId::SPLASH;
const WATER_GUN: MoveId = MoveId::WATER_GUN;
const LEVEL: u8 = 20;

/// Cute Charm, mixed gender, base Speed 90: faster than [`RATTATA`] at the
/// same level and IVs, so it has acted before the attacker hits it.
const SKITTY: SpeciesId = SpeciesId::SKITTY;
/// Mixed gender, base Speed 72, ability slot 0 Run Away.
const RATTATA: SpeciesId = SpeciesId::RATTATA;

/// Low byte below every mixed ratio's threshold, and a neutral nature.
const FEMALE: u32 = 0;
/// Low byte above every mixed ratio's threshold, and a neutral nature.
const MALE: u32 = 200;

/// Construction's turn-number draw, the turn-start draw and the wild
/// opponent's move-slot pick.
const SETUP: [u16; 3] = [0; 3];
/// Accuracy, no critical hit, the lowest damage roll, and the discarded
/// secondary-effect draw of one landed Tackle.
const TACKLE_HITS: [u16; 4] = [0, 1, 0, 0];
/// `Random() % 3 == 0` triggers Cute Charm.
const CHARM_TRIGGERS: u16 = 0;
const CHARM_ONE: u16 = 1;
const CHARM_TWO: u16 = 2;
/// `Random() & 1` odd lets an infatuated battler's move continue.
const LOVE_LETS_MOVE_THROUGH: u16 = 1;
/// `Random() & 1` even immobilizes it.
const LOVE_IMMOBILIZES: u16 = 0;

fn mon(species: SpeciesId, personality: u32, moves: &[MoveId]) -> BattlePokemon {
    max_iv_mon_with_personality(&Dex::new(), species.0, LEVEL, moves.to_vec(), personality)
}

fn script(groups: &[&[u16]]) -> Vec<u16> {
    groups.concat()
}

/// The charm event for a contact `move_id` that infatuated the player.
fn charmed_player(move_id: MoveId) -> BattleEvent {
    BattleEvent::InfatuatedByCuteCharm {
        by_player: true,
        move_id,
    }
}

fn cute_charm_events(events: &[BattleEvent]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter(|event| matches!(event, BattleEvent::InfatuatedByCuteCharm { .. }))
        .collect()
}

fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

/// Runs the player's first move for one turn on an exactly scripted RNG.
fn player_turn(
    player: BattlePokemon,
    enemy: BattlePokemon,
    draws: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(draws.iter().copied());
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

/// The player's `attacker` hits a faster enemy [`SKITTY`] holder that spends
/// its turn on Splash ahead of it, with `contact` drawn after the hit.
fn attacker_hits_enemy_holder(
    attacker: BattlePokemon,
    holder: BattlePokemon,
    contact: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    player_turn(attacker, holder, &script(&[&SETUP, &TACKLE_HITS, contact]))
}

/// The enemy's `attacker` hits a faster player [`SKITTY`] holder that Splashes
/// ahead of it, with `contact` drawn after the hit.
fn enemy_attacker_hits_player_holder(
    attacker: BattlePokemon,
    holder: BattlePokemon,
    contact: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    player_turn(holder, attacker, &script(&[&SETUP, &TACKLE_HITS, contact]))
}

fn skitty_holder(personality: u32) -> BattlePokemon {
    mon(SKITTY, personality, &[SPLASH])
}

fn rattata_attacker(personality: u32, moves: &[MoveId]) -> BattlePokemon {
    mon(RATTATA, personality, moves)
}
