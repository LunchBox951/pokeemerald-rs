//! The pre-PP obedience gate (`IsMonDisobedient`,
//! `pokeemerald/src/battle_util.c:3923`-`:4038`, called from
//! `Cmd_attackcanceler`, `src/battle_script_commands.c:944`-`:957`), driven
//! through real turns: ownership, badge limit, RNG consumption, and PP on a
//! cancelled action.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::battle::BattlerSide;
use battle::{
    Battle, BattleError, BattleEvent, Dex, LoafMessage, PlayerAction, PlayerObedience, Status1,
};

const TACKLE: MoveId = MoveId::TACKLE;
/// A second ordinary hit for the alternate-move branch to land on.
const SCRATCH: MoveId = MoveId::SCRATCH;
/// `SPECIES_GASTLY`: base Speed 80, so it outruns the Zigzagoon below and
/// acts first, putting the obedience draws ahead of the enemy's.
const GASTLY: u16 = 92;
/// `SPECIES_ZIGZAGOON`: base Speed 60.
const ZIGZAGOON: u16 = 288;

const PLAYER_ID: u32 = 0x1111_2222;
const OTHER_ID: u32 = 0x3333_4444;
/// "ASH" in game text, terminator-padded.
const PLAYER_NAME: [u8; 8] = [0xBB, 0xCD, 0xC2, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

fn no_badges() -> PlayerObedience {
    PlayerObedience::new(PLAYER_ID, PLAYER_NAME, [false; 4])
}

struct Run {
    events: Vec<BattleEvent>,
    draws: usize,
    /// PP left in each of the player's slots after the turn.
    player_pps: Vec<u8>,
    player_status: Status1,
    enemy_last_hitter: Option<BattlerSide>,
}

impl Run {
    /// PP left in the player's first slot.
    fn player_pp(&self) -> u8 {
        self.player_pps[0]
    }
}

/// A level-`level` Gastly owned by `ot_id` against a level-5 Zigzagoon;
/// `script` begins with the two turn-number draws and the enemy's
/// selection, then feeds the actions in speed order, padded with zeros.
fn run(level: u8, ot_id: u32, obedience: Option<PlayerObedience>, gate_draws: &[u16]) -> Run {
    run_with(vec![TACKLE], 0, level, ot_id, obedience, gate_draws)
}

/// [`run`] with the player's known `moves`, choosing `action_slot`.
fn run_with(
    moves: Vec<MoveId>,
    action_slot: usize,
    level: u8,
    ot_id: u32,
    obedience: Option<PlayerObedience>,
    gate_draws: &[u16],
) -> Run {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, level, moves).with_original_trainer_id(ot_id);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    let mut values = vec![0, 0, 0];
    values.extend_from_slice(gate_draws);
    values.extend([0; 32]);
    let mut rng = SequenceRng::new(values);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    if let Some(obedience) = obedience {
        battle = battle.with_player_obedience(obedience);
    }
    let events = battle
        .take_turn(PlayerAction::UseMove(action_slot), &mut rng)
        .unwrap();
    Run {
        events,
        draws: rng.draws(),
        player_pps: battle.player().moves().iter().map(|slot| slot.pp).collect(),
        player_status: battle.player().status1(),
        enemy_last_hitter: battle.battler_state(BattlerSide::Enemy).last_hitter(),
    }
}

/// The same fixture with no obedience gate at all, fed the same script.
fn baseline(level: u8, script: &[u16]) -> Run {
    run(level, PLAYER_ID, None, script)
}

#[test]
fn a_domestic_pokemon_above_the_limit_draws_nothing() {
    let base = baseline(20, &[255, 255, 255]);
    let domestic = run(20, PLAYER_ID, Some(no_badges()), &[255, 255, 255]);
    assert_eq!(domestic.draws, base.draws);
    assert_eq!(domestic.events, base.events);
}

#[test]
fn a_traded_pokemon_at_or_below_the_limit_draws_nothing() {
    let base = baseline(10, &[255, 255, 255]);
    let traded = run(10, OTHER_ID, Some(no_badges()), &[255, 255, 255]);
    assert_eq!(traded.draws, base.draws);
    assert_eq!(traded.events, base.events);
}

#[test]
fn the_eighth_badge_ends_obedience_checks() {
    let base = baseline(20, &[255, 255, 255]);
    let badged = PlayerObedience::new(PLAYER_ID, PLAYER_NAME, [true; 4]);
    let traded = run(20, OTHER_ID, Some(badged), &[255, 255, 255]);
    assert_eq!(traded.draws, base.draws);
}

#[test]
fn badge_two_raises_the_limit_past_the_pokemons_level() {
    let base = baseline(20, &[255, 255, 255]);
    let badged = PlayerObedience::new(PLAYER_ID, PLAYER_NAME, [true, false, false, false]);
    let traded = run(20, OTHER_ID, Some(badged), &[255, 255, 255]);
    assert_eq!(traded.draws, base.draws);
}

#[test]
fn a_same_id_but_different_name_pokemon_is_traded() {
    let base = baseline(20, &[]);
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, 20, vec![TACKLE])
        .with_original_trainer_id(PLAYER_ID)
        .with_original_trainer_name([0xBB, 0xCD, 0xC3, 0xFF, 0xFF, 0xFF, 0xFF]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(vec![0; 40]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(rng.draws(), base.draws + 1, "one obedience draw, then obey");
}

#[test]
fn a_traded_pokemon_above_the_limit_draws_once_then_obeys_on_a_low_roll() {
    let base = baseline(20, &[0]);
    let traded = run(20, OTHER_ID, Some(no_badges()), &[0, 0]);
    assert_eq!(traded.draws, base.draws + 1);
    assert_eq!(traded.events, base.events);
    assert_eq!(traded.player_pp(), base.player_pp());
}

#[test]
fn a_loafing_pokemon_keeps_its_pp_and_draws_four_times() {
    let base = baseline(20, &[]);
    // 255 > limit on all three tests; the loaf roll 1 is "won't obey".
    let traded = run(20, OTHER_ID, Some(no_badges()), &[255, 255, 255, 1]);
    assert_eq!(
        traded.events[0],
        BattleEvent::Disobeyed {
            message: LoafMessage::WontObey,
        }
    );
    assert_eq!(
        traded.player_pp(),
        base.player_pp() + 1,
        "PP of the cancelled action is kept"
    );
    // Three setup draws, four gate draws (obey, branch, nap/self-hit, loaf
    // text), then the enemy's immune Tackle (4 draws); the player's own
    // move draws never happen.
    assert_eq!(traded.draws, 3 + 4 + 4);
}

#[test]
fn a_loafing_action_records_its_attacker_as_the_targets_last_hitter() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, 20, vec![TACKLE]).with_original_trainer_id(OTHER_ID);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    let mut values = vec![0, 0, 0, 255, 255, 255, 1];
    values.extend([0; 32]);
    let mut rng = SequenceRng::new(values);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(matches!(events[0], BattleEvent::Disobeyed { .. }));
    let state = battle.battler_state(BattlerSide::Enemy);
    assert_eq!(
        state.last_hitter(),
        Some(BattlerSide::Player),
        "`moveendto MOVEEND_NEXT_TARGET` runs MOVEEND_UPDATE_LAST_MOVES"
    );
}

/// The alternate-move branch (`battle_util.c:3975`-`:3998`) with another
/// usable slot: the drawn move runs as the action and spends its own PP.
#[test]
fn a_disobedient_pokemon_uses_another_move_and_spends_that_moves_pp() {
    // The ungated battler choosing Scratch directly, fed the same script
    // the alternate move will see.
    let base = run_with(vec![TACKLE, SCRATCH], 1, 20, PLAYER_ID, None, &[]);
    // A=255 disobeys; B=0 selects the alternate-move branch. Slot draws
    // (% 4): 0 is the chosen Tackle and is rejected, 1 is Scratch. Target
    // draws (% 2): 0 is the attacker itself and is rejected, 1 is the foe.
    let traded = run_with(
        vec![TACKLE, SCRATCH],
        0,
        20,
        OTHER_ID,
        Some(no_badges()),
        &[255, 0, 0, 1, 0, 1],
    );
    assert_eq!(traded.events[0], BattleEvent::IgnoredOrders);
    assert_eq!(
        traded.events[1],
        BattleEvent::UsedMove {
            by_player: true,
            move_id: SCRATCH,
        }
    );
    assert_eq!(traded.events[1..], base.events[..]);
    assert_eq!(
        traded.draws,
        base.draws + 2 + 2 + 2,
        "two gate, two slot, two target draws"
    );
    assert_eq!(
        traded.player_pps[0], base.player_pps[0],
        "the chosen Tackle keeps its PP"
    );
    assert_eq!(
        traded.player_pps[1], base.player_pps[1],
        "Scratch's PP is spent"
    );
    assert_eq!(traded.enemy_last_hitter, base.enemy_last_hitter);
}

/// The alternate-move branch when every other slot is unusable
/// (`battle_util.c:3978`-`:3984`): a lone known move loafs after one
/// message draw, with no slot or target draws.
#[test]
fn a_disobedient_pokemon_with_no_other_usable_move_loafs_instead() {
    let base = baseline(20, &[]);
    // A=255 disobeys; B=0 selects the alternate-move branch; the loaf roll 2
    // is "turned away".
    let traded = run(20, OTHER_ID, Some(no_badges()), &[255, 0, 2]);
    assert_eq!(
        traded.events[0],
        BattleEvent::Disobeyed {
            message: LoafMessage::TurnedAway,
        }
    );
    assert_eq!(traded.player_pp(), base.player_pp() + 1);
    // Three setup, three gate draws (obey, branch, loaf text), the enemy's
    // four.
    assert_eq!(traded.draws, 3 + 3 + 4);
    assert_eq!(traded.enemy_last_hitter, Some(BattlerSide::Player));
}

/// A depleted second slot is `MOVE_LIMITATION_PP` and never drawn
/// (`battle_util.c:1100`-`:1101`).
#[test]
fn a_depleted_slot_is_not_an_alternate_move() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, 20, vec![TACKLE, SCRATCH])
        .with_original_trainer_id(OTHER_ID)
        .with_saved_pp(1, 0)
        .unwrap();
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    // A=255, B=0, loaf roll 3: "pretended not to notice".
    let mut values = vec![0, 0, 0, 255, 0, 3];
    values.extend([0; 32]);
    let mut rng = SequenceRng::new(values);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events[0],
        BattleEvent::Disobeyed {
            message: LoafMessage::PretendedNotToNotice,
        }
    );
    assert_eq!(rng.draws(), 3 + 3 + 4);
}

/// The nap (`battle_util.c:4004`-`:4016`; `BattleScript_IgnoresAndFallsAsleep`):
/// the sleep lands with `SetMoveEffect`'s own duration draw and the chosen
/// move keeps its PP.
#[test]
fn a_napping_pokemon_falls_asleep_for_the_drawn_duration_and_keeps_its_pp() {
    let base = baseline(20, &[]);
    // A=255, B=255 disobey into the nap/self-hit branch; C=9 is below the
    // excess 10 on a healthy Gastly (Levitate), so it naps. The duration
    // draw 1 gives (1 & 3) + 2 = 3 turns.
    let traded = run(20, OTHER_ID, Some(no_badges()), &[255, 255, 9, 1]);
    assert_eq!(traded.events[0], BattleEvent::DisobedientNap);
    assert_eq!(traded.player_status, Status1::Asleep { turns: 3 });
    assert_eq!(traded.player_pp(), base.player_pp() + 1);
    // Three setup, three gate draws, the duration draw, the enemy's four.
    assert_eq!(traded.draws, 3 + 3 + 1 + 4);
    assert_eq!(traded.enemy_last_hitter, Some(BattlerSide::Player));
}

#[test]
fn a_self_hit_keeps_pp_and_draws_one_variance_roll() {
    let base = baseline(20, &[]);
    // A=255, B=255 disobey into the nap/self-hit branch; C=15 is not below
    // the excess 10 (no nap) and 15 - 10 < 10 (self-hit).
    let traded = run(20, OTHER_ID, Some(no_badges()), &[255, 255, 15]);
    assert!(matches!(
        traded.events[0],
        BattleEvent::DisobedientSelfHit {
            by_player: true,
            ..
        }
    ));
    assert_eq!(traded.player_pp(), base.player_pp() + 1);
    // Three setup, three gate, one variance draw, the enemy's four.
    assert_eq!(traded.draws, 3 + 3 + 1 + 4);
}

#[test]
fn an_ot_name_that_is_a_prefix_of_the_players_name_is_not_traded() {
    // `IsOtherTrainer` walks only the Pokemon's name to its terminator
    // (`pokemon.c:6619`-`:6622`), so "A" matches the player "ASH".
    let base = baseline(20, &[]);
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, 20, vec![TACKLE])
        .with_original_trainer_id(PLAYER_ID)
        .with_original_trainer_name([0xBB, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(vec![0; 40]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(rng.draws(), base.draws);
}

#[test]
fn a_traded_mew_above_its_limit_is_refused_before_any_obedience_draw() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 151, 20, vec![TACKLE]).with_original_trainer_id(OTHER_ID);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new(vec![0; 40]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    let before = rng.draws();
    let error = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(error.error(), BattleError::UnportedDisobedience);
    // Only the turn-number draw and the enemy's selection ran.
    assert_eq!(rng.draws() - before, 2);
}

/// A Mew the player owns above the badge limit obeys with no obedience draw.
/// Upstream tests the Mew/Deoxys guard before ownership
/// (`battle_util.c:3913-3934`), but the port keeps it after the exemptions on
/// purpose: every legitimate source of a Mew sets the fateful-encounter bit
/// (`pokemon.c:2633-2639`), trades refuse one without it (`trade.c:1577-1582`),
/// and the port has no field for the bit, so a guard-first order would refuse
/// every legitimately caught Mew.
#[test]
fn a_domestic_mew_above_its_limit_obeys_because_the_port_does_not_model_the_fateful_bit() {
    let draws = |gated: bool| {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 151, 20, vec![TACKLE]).with_original_trainer_id(PLAYER_ID);
        let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
        let mut rng = SequenceRng::new(vec![0; 40]);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        if gated {
            battle = battle.with_player_obedience(no_badges());
        }
        battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        (rng.draws(), battle.player().moves()[0].pp)
    };
    let (ungated_draws, ungated_pp) = draws(false);
    let (gated_draws, gated_pp) = draws(true);
    assert_eq!(gated_draws, ungated_draws);
    assert_eq!(gated_pp, ungated_pp);
}

/// A traded Gastly knowing Tackle and `alternate`, choosing Tackle against a
/// level-5 `enemy_species` whose first ability slot is active, fed the
/// alternate-move script: A=255 disobeys, B=0 picks the alternate branch,
/// slot draws 0 (the chosen Tackle, rejected) then 1, target draws 0 (the
/// attacker, rejected) then 1.
fn alternate_move_turn(
    alternate: MoveId,
    enemy_species: u16,
) -> (
    Battle,
    Result<Vec<BattleEvent>, battle::TurnError>,
    SequenceRng,
) {
    let dex = Dex::new();
    let player =
        max_iv_mon(&dex, GASTLY, 20, vec![TACKLE, alternate]).with_original_trainer_id(OTHER_ID);
    let enemy = max_iv_mon(&dex, enemy_species, 5, vec![TACKLE]).with_ability_slot(0);
    let mut values = vec![0, 0, 0, 255, 0, 0, 1, 0, 1];
    values.extend([0; 32]);
    let mut rng = SequenceRng::new(values);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    let result = battle.take_turn(PlayerAction::UseMove(0), &mut rng);
    (battle, result, rng)
}

/// The alternate move gets the same ability-interaction screen a directly
/// selected one does: Flame Wheel's burn against a healthy Synchronize
/// holder would be reflected by `MOVEEND_SYNCHRONIZE_TARGET`
/// (`battle_util.c:2971`-`:2986`), which is unmodelled, so the turn is
/// refused before the alternate move is announced or spends PP.
#[test]
fn an_alternate_move_with_an_unmodelled_ability_interaction_is_refused() {
    let ralts = assets::SpeciesId::RALTS.0;
    let dex = Dex::new();
    let enemy = max_iv_mon(&dex, ralts, 5, vec![TACKLE]).with_ability_slot(0);
    assert_eq!(enemy.ability(), assets::AbilityId::SYNCHRONIZE);
    // Selecting Flame Wheel directly is refused before any draw.
    let player = max_iv_mon(&dex, GASTLY, 20, vec![TACKLE, MoveId::FLAME_WHEEL]);
    let mut rng = SequenceRng::new(vec![0; 8]);
    let mut direct = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let draws_before = rng.draws();
    let refused = direct
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap_err();
    assert_eq!(
        refused.error(),
        BattleError::UnportedAbilityInteraction(assets::AbilityId::SYNCHRONIZE)
    );
    assert_eq!(rng.draws(), draws_before);

    let (battle, result, rng) = alternate_move_turn(MoveId::FLAME_WHEEL, ralts);
    let refused = result.unwrap_err();
    assert_eq!(
        refused.error(),
        BattleError::UnportedAbilityInteraction(assets::AbilityId::SYNCHRONIZE)
    );
    assert_eq!(refused.events(), [BattleEvent::IgnoredOrders]);
    assert_eq!(
        rng.draws(),
        3 + 2 + 2 + 2,
        "setup, gate, slot and target draws"
    );
    let full_pp: Vec<u8> = max_iv_mon(&Dex::new(), GASTLY, 20, vec![TACKLE, MoveId::FLAME_WHEEL])
        .moves()
        .iter()
        .map(|slot| slot.pp)
        .collect();
    let pps: Vec<u8> = battle.player().moves().iter().map(|slot| slot.pp).collect();
    assert_eq!(pps, full_pp, "neither slot spends PP");
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

/// An alternate move with no executable pipeline is refused with the error a
/// direct selection gets, before it is announced or spends PP.
#[test]
fn an_alternate_move_without_an_executable_pipeline_is_refused() {
    let (battle, result, _) = alternate_move_turn(MoveId::HORN_DRILL, ZIGZAGOON);
    let refused = result.unwrap_err();
    assert_eq!(
        refused.error(),
        BattleError::UnsupportedMoveEffect(MoveId::HORN_DRILL)
    );
    assert_eq!(refused.events(), [BattleEvent::IgnoredOrders]);
    let full_pp = max_iv_mon(&Dex::new(), GASTLY, 20, vec![MoveId::HORN_DRILL]).moves()[0].pp;
    assert_eq!(battle.player().moves()[1].pp, full_pp);
}

/// The screen admits an alternate move whose interaction is modelled: the
/// same Flame Wheel against a non-Synchronize foe runs and spends its PP.
#[test]
fn an_admissible_alternate_move_still_runs() {
    let (battle, result, _) = alternate_move_turn(MoveId::FLAME_WHEEL, ZIGZAGOON);
    let events = result.unwrap();
    assert_eq!(events[0], BattleEvent::IgnoredOrders);
    assert_eq!(
        events[1],
        BattleEvent::UsedMove {
            by_player: true,
            move_id: MoveId::FLAME_WHEEL,
        }
    );
    let full_pp = max_iv_mon(&Dex::new(), GASTLY, 20, vec![MoveId::FLAME_WHEEL]).moves()[0].pp;
    assert_eq!(battle.player().moves()[1].pp, full_pp - 1);
}
