//! Attract (S-6): `Cmd_tryinfatuating`'s apply-time guards, the
//! `CANCELER_IN_LOVE` coin at its place among the cancellers, the Rollout
//! lock it releases, clearing when either battler leaves, and Daisy's
//! `AI_CBM_Attract` scoring. Upstream: `Cmd_tryinfatuating`
//! (`src/battle_script_commands.c:7654`-`:7697`), `AtkCanceler_UnableToUseMove`
//! (`src/battle_util.c:2200`-`:2219`), `BattleScript_EffectAttract`
//! (`data/battle_scripts_1.s:1645`-`:1655`). Every script is exact:
//! `SequenceRng::assert_exhausted` fails on a missing or an extra draw.

use crate::common::{max_iv_mon_with_personality, SequenceRng};
use assets::{MoveId, SpeciesId};
use battle::battle::BattlerSide;
use battle::{Battle, BattleEvent, BattlePokemon, Dex, PlayerAction};

pub(super) const ATTRACT: MoveId = MoveId::ATTRACT;
pub(super) const SPLASH: MoveId = MoveId::SPLASH;
pub(super) const TACKLE: MoveId = MoveId::TACKLE;
pub(super) const LEVEL: u8 = 20;

/// Fast, mixed gender: faster than every other fixture here, so it has
/// acted before any Attract lands on it.
pub(super) const RATTATA: SpeciesId = SpeciesId::RATTATA;
/// Slower than [`RATTATA`], mixed gender, one ability.
pub(super) const MUDKIP: SpeciesId = SpeciesId::MUDKIP;
/// Genderless and slower than [`RATTATA`].
pub(super) const BELDUM: SpeciesId = SpeciesId::BELDUM;
/// Always female, Oblivious, and faster than [`MUDKIP`].
pub(super) const ILLUMISE: SpeciesId = SpeciesId::ILLUMISE;
/// Slower than [`RATTATA`], mixed gender.
pub(super) const ZIGZAGOON: SpeciesId = SpeciesId::ZIGZAGOON;

/// Low byte below every mixed ratio's threshold, and a neutral nature.
pub(super) const FEMALE: u32 = 0;
/// Low byte above every mixed ratio's threshold, and a neutral nature.
pub(super) const MALE: u32 = 200;

/// Construction's turn-number draw, the turn-start draw and a wild
/// opponent's move-slot pick, with unequal Speeds so no tie draw follows.
pub(super) const PREFIX: [u16; 3] = [0; 3];

/// `Random() % 100 + 1` against Attract's 100 accuracy: even the top roll hits.
pub(super) const ACCURACY_HITS: u16 = 99;

/// `Random() & 1` odd lets an infatuated battler's move continue.
pub(super) const LOVE_LETS_MOVE_THROUGH: u16 = 1;
/// `Random() & 1` even immobilizes it.
pub(super) const LOVE_IMMOBILIZES: u16 = 0;

pub(super) fn mon(species: SpeciesId, personality: u32, moves: &[MoveId]) -> BattlePokemon {
    max_iv_mon_with_personality(&Dex::new(), species.0, LEVEL, moves.to_vec(), personality)
}

pub(super) fn infatuated_by(mut mon: BattlePokemon, attractor: BattlerSide) -> BattlePokemon {
    mon.volatiles_mut().set_infatuated_by(attractor);
    mon
}

/// Runs one turn in which the player uses move slot 0 against a wild enemy
/// that picks its only move.
pub(super) fn wild_turn(
    player: BattlePokemon,
    enemy: BattlePokemon,
    groups: &[&[u16]],
) -> (Battle, Vec<BattleEvent>, SequenceRng) {
    let mut rng = SequenceRng::new(groups.concat());
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (battle, events, rng)
}

pub(super) fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

pub(super) fn in_love(by_player: bool) -> BattleEvent {
    BattleEvent::InLove { by_player }
}

pub(super) fn immobilized(by_player: bool, move_id: MoveId) -> BattleEvent {
    BattleEvent::ImmobilizedByLove { by_player, move_id }
}

pub(super) fn used_move(by_player: bool, move_id: MoveId) -> BattleEvent {
    BattleEvent::UsedMove { by_player, move_id }
}

mod application;
mod cancellation;
mod lifecycle;
mod trainer;
