//! The pre-PP obedience gate (`IsMonDisobedient`,
//! `pokeemerald/src/battle_util.c:3923`-`:4038`, called from
//! `Cmd_attackcanceler`, `src/battle_script_commands.c:944`-`:957`), driven
//! through real turns: ownership, badge limit, RNG consumption, and PP on a
//! cancelled action.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleError, BattleEvent, Dex, LoafMessage, PlayerAction, PlayerObedience};

const TACKLE: MoveId = MoveId::TACKLE;
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
    player_pp: u8,
}

/// A level-`level` Gastly owned by `ot_id` against a level-5 Zigzagoon;
/// `script` begins with the two turn-number draws and the enemy's
/// selection, then feeds the actions in speed order, padded with zeros.
fn run(level: u8, ot_id: u32, obedience: Option<PlayerObedience>, gate_draws: &[u16]) -> Run {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, level, vec![TACKLE]).with_original_trainer_id(ot_id);
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
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    Run {
        events,
        draws: rng.draws(),
        player_pp: battle.player().moves()[0].pp,
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
    assert_eq!(traded.player_pp, base.player_pp);
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
        traded.player_pp,
        base.player_pp + 1,
        "PP of the cancelled action is kept"
    );
    // Three setup draws, four gate draws (obey, branch, nap/self-hit, loaf
    // text), then the enemy's immune Tackle (4 draws); the player's own
    // move draws never happen.
    assert_eq!(traded.draws, 3 + 4 + 4);
}

#[test]
fn an_unmodelled_disobedient_outcome_is_refused_not_approximated() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, GASTLY, 20, vec![TACKLE]).with_original_trainer_id(OTHER_ID);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 5, vec![TACKLE]);
    // A=255 disobeys; B=0 selects the alternate-move branch.
    let mut rng = SequenceRng::new(vec![0, 0, 0, 255, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .unwrap()
        .with_player_obedience(no_badges());
    let error = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(error.error(), BattleError::UnportedDisobedience);
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
    assert_eq!(traded.player_pp, base.player_pp + 1);
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
