//! The confusion self-hit's `EFFECT_EXPLOSION` Defense halving and burn
//! damage halving, driven through complete turns with a confused wild
//! Koffing.
//!
//! `SequenceRng` panics once its scripted values run out, so a script sized
//! to exactly the expected draws is itself the proof that nothing further
//! was drawn.

use crate::common::{max_iv_mon, slow_runner_rattata, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, PlayerAction, StatStage, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
/// `SPECIES_KOFFING`: Hardy L20 with all IVs at 31 has Attack 37 and Defense 49.
const KOFFING: u16 = 109;

/// `FAILED_ESCAPE_ROLL` for the slow Rattata runner: the player's Run fails,
/// so the confused wild Koffing still takes its own action.
const FAILED_ESCAPE_ROLL: u16 = 65_000;

/// What one confused wild Koffing turn left behind.
struct KoffingSelfHit {
    events: Vec<BattleEvent>,
    enemy_hp: u32,
    enemy_pp: [u8; 2],
    draws: usize,
}

/// Runs one turn in which a confusion-ticking wild Koffing (Tackle in slot 0,
/// `second_move` in slot 1, which is drained to 0 PP when `deplete_second`)
/// selects `selected_slot` and takes the even confusion coin. The slow
/// Rattata player's Run fails first, so the enemy's self-hit is the only
/// damage. `prepare` adjusts the Koffing before the battle is built.
///
/// The six scripted draws are two setup draws, the wild slot pick, the
/// failed escape roll, the confusion coin (even -> self-hit) and the
/// maximum-residue-0 damage roll; `SequenceRng` panics on a seventh.
fn confused_koffing_turn(
    second_move: MoveId,
    deplete_second: bool,
    selected_slot: u16,
    prepare: impl FnOnce(&mut battle::BattlePokemon),
) -> KoffingSelfHit {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, KOFFING, 20, vec![TACKLE, second_move]);
    enemy.volatiles_mut().set_confusion(3);
    if deplete_second {
        for _ in 0..enemy.moves()[1].pp {
            enemy.deduct_pp(1).unwrap();
        }
    }
    prepare(&mut enemy);

    let mut rng = SequenceRng::new([0, 0, selected_slot, FAILED_ESCAPE_ROLL, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    KoffingSelfHit {
        events,
        enemy_hp: battle.enemy().current_hp(),
        enemy_pp: [battle.enemy().moves()[0].pp, battle.enemy().moves()[1].pp],
        draws: rng.draws(),
    }
}

fn self_hit_damage(events: &[BattleEvent]) -> u32 {
    let damages: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::ConfusionSelfHit {
                by_player: false,
                damage,
            } => Some(*damage),
            _ => None,
        })
        .collect();
    assert_eq!(damages.len(), 1, "exactly one enemy self-hit: {events:?}");
    damages[0]
}

fn assert_self_hit_branch_only(events: &[BattleEvent]) {
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: false,
                ..
            } | BattleEvent::FailedNoPp { .. }
        )),
        "the self-hit ends the action before any ordinary hit or no-PP abort: {events:?}"
    );
}

/// `CalculateBaseDamage` tests the selected `gCurrentMove`, not the Pound
/// that the confusion branch passes, for `EFFECT_EXPLOSION` and halves the raw
/// Defense (`pokemon.c:3228-3230`). The wild slot pick never skips a depleted
/// slot, so a 0-PP Selfdestruct or Explosion reaches the confusion canceller
/// ahead of the no-PP abort (`battle_script_commands.c:930-939`).
/// Tackle control: ((2 * 20 / 5 + 2) * 40 * 37 / 49) / 50 + 2 = 8; halved
/// Defense 24 gives 14.
#[test]
fn a_confusion_self_hit_halves_defense_for_a_depleted_explosion_effect_selection() {
    for move_id in [MoveId::SELF_DESTRUCT, MoveId::EXPLOSION] {
        let control = confused_koffing_turn(move_id, true, 0, |_| {});
        assert_eq!(self_hit_damage(&control.events), 8, "{move_id:?} control");

        let depleted = confused_koffing_turn(move_id, true, 1, |_| {});
        assert_eq!(
            self_hit_damage(&depleted.events),
            14,
            "a selected {move_id:?} halves the self-hit's Defense: {:?}",
            depleted.events
        );
        assert_self_hit_branch_only(&depleted.events);
        assert_eq!(depleted.enemy_pp, [35, 0], "a self-hit spends no PP");
        assert_eq!(depleted.draws, 6);
        assert_eq!(depleted.enemy_hp, 52 - 14);
        assert_eq!(control.draws, 6);
        assert_eq!(control.enemy_hp, 52 - 8);
    }
}

/// An ordinary non-Explosion slot keeps the unhalved Defense whether it has
/// PP or not.
#[test]
fn a_confusion_self_hit_keeps_full_defense_for_a_non_explosion_selection() {
    let spent_scratch = confused_koffing_turn(MoveId::SCRATCH, true, 1, |_| {});
    assert_eq!(self_hit_damage(&spent_scratch.events), 8);
    let live_scratch = confused_koffing_turn(MoveId::SCRATCH, false, 1, |_| {});
    assert_eq!(self_hit_damage(&live_scratch.events), 8);
}

/// The HP cap and faint still apply to the larger hit, unlike the control that
/// survives at 2 HP.
#[test]
fn a_halved_defense_self_hit_is_capped_at_current_hp_and_faints_the_user() {
    let ten_hp = |enemy: &mut battle::BattlePokemon| {
        let lost = enemy.stats().max_hp - 10;
        enemy.apply_damage(lost);
    };
    let control = confused_koffing_turn(MoveId::SELF_DESTRUCT, true, 0, ten_hp);
    assert_eq!(self_hit_damage(&control.events), 8);
    assert_eq!(control.enemy_hp, 2);
    assert!(!control
        .events
        .contains(&BattleEvent::Fainted { by_player: false }));

    let depleted = confused_koffing_turn(MoveId::SELF_DESTRUCT, true, 1, ten_hp);
    assert_eq!(
        self_hit_damage(&depleted.events),
        10,
        "the 14-point hit is capped at the 10 HP left: {:?}",
        depleted.events
    );
    assert_eq!(depleted.enemy_hp, 0);
    assert!(depleted
        .events
        .contains(&BattleEvent::Fainted { by_player: false }));
}

/// `defense /= 2` runs on the raw stat before `APPLY_STAT_MOD` scales it by
/// the stage (`pokemon.c:3228-3230`, `:3248-3260`). Attack +1 and Defense +2
/// give 11; scaling first (49 * 2 = 98, halved 49) would give 10.
#[test]
fn the_explosion_halving_applies_before_the_defense_stage() {
    let staged = |enemy: &mut battle::BattlePokemon| {
        enemy.stages_mut().attack = StatStage::new(1).unwrap();
        enemy.stages_mut().defense = StatStage::new(2).unwrap();
    };
    let depleted = confused_koffing_turn(MoveId::EXPLOSION, true, 1, staged);
    assert_eq!(
        self_hit_damage(&depleted.events),
        11,
        "halve the raw Defense, then apply the stage: {:?}",
        depleted.events
    );
    let control = confused_koffing_turn(MoveId::EXPLOSION, true, 0, staged);
    assert_eq!(self_hit_damage(&control.events), 6);
}

/// `CalculateBaseDamage` halves a burned attacker's damage after the division
/// by 50 (`pokemon.c:3262-3264`), and the confusion branch hands it the
/// confused battler as both attacker and defender (`battle_util.c:2173`).
/// Tackle control: ((2 * 20 / 5 + 2) * 40 * 37 / 49) / 50 = 6, +2 = 8; burned,
/// 6 / 2 + 2 = 5.
#[test]
fn a_burned_battlers_confusion_self_hit_halves_its_damage() {
    let burned = |enemy: &mut battle::BattlePokemon| enemy.set_status1(Status1::Burned);
    let control = confused_koffing_turn(MoveId::SCRATCH, false, 0, |_| {});
    assert_eq!(self_hit_damage(&control.events), 8, "unburned control");

    let hit = confused_koffing_turn(MoveId::SCRATCH, false, 0, burned);
    assert_eq!(
        self_hit_damage(&hit.events),
        5,
        "a burned self-hit is halved: {:?}",
        hit.events
    );
    assert_self_hit_branch_only(&hit.events);
    assert_eq!(hit.enemy_pp, [35, 35], "a self-hit spends no PP");
    assert_eq!(hit.draws, 6);
}

/// The burn halving is independent of the raw-stat Defense halving: a burned
/// Explosion selection gives (37 * 40 * 10 / 24) / 50 = 12, halved to 6, +2 =
/// 8, against the unburned 14.
#[test]
fn the_burn_halving_stacks_with_the_explosion_defense_halving() {
    let burned = |enemy: &mut battle::BattlePokemon| enemy.set_status1(Status1::Burned);
    let unburned = confused_koffing_turn(MoveId::EXPLOSION, true, 1, |_| {});
    assert_eq!(self_hit_damage(&unburned.events), 14);
    let hit = confused_koffing_turn(MoveId::EXPLOSION, true, 1, burned);
    assert_eq!(self_hit_damage(&hit.events), 8, "{:?}", hit.events);
}
