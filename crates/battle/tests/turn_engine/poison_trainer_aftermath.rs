//! Poison residual ticks that interleave with trainer-battle aftermath:
//! reward payout, trainer defeat, and level-up prompts.

use crate::poison_support::*;
use assets::MoveId;
use battle::status1::poison_residual_damage;
use battle::{Battle, BattleEvent, BattleOutcome, Dex, PlayerAction, Status1};

/// `HandleFaintedMonActions`' `BattleScript_GiveExp`
/// (`src/battle_util.c:1912`-`:1923`) runs at the end of the killing move's
/// own script, while `DoBattlerEndTurnEffects` waits for `BattleTurnPassed`
/// (`src/battle_main.c:3960`-`:3968`).
#[test]
fn a_direct_hit_kos_reward_is_paid_before_the_residual_tick_that_fells_the_winner() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_lethal = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - player_lethal);
    let evs_before = player.evs();
    let exp_before = player.experience();

    let lead = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let benched = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    // No draw in this turn decides anything the assertions below read.
    let mut rng = SequenceRng::new([0; 40]);
    let mut battle = Battle::new_trainer(
        dex,
        player,
        MAY_ROUTE_103_MUDKIP,
        vec![lead, benched],
        &mut rng,
    )
    .unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let exp_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::ExpGained(_)))
        .unwrap_or_else(|| panic!("the direct-hit knockout pays out: {events:?}"));
    let BattleEvent::ExpGained(exp_gained) = events[exp_index] else {
        unreachable!("position() matched ExpGained")
    };
    let sent_out_index = events
        .iter()
        .position(|event| matches!(event, BattleEvent::TrainerSentOut { .. }))
        .unwrap_or_else(|| panic!("the bench replaces the fallen lead: {events:?}"));
    let tick_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::HurtByPoison {
                    by_player: true,
                    ..
                }
            )
        })
        .unwrap_or_else(|| panic!("the poisoned winner still ticks: {events:?}"));

    assert!(
        exp_index < tick_index && sent_out_index < tick_index,
        "the knockout is settled in full before the residual pass: {events:?}"
    );
    assert_eq!(
        events[tick_index],
        BattleEvent::HurtByPoison {
            by_player: true,
            damage: player_lethal,
        },
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: true }),
        "the fixture's tick must be lethal: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert_eq!(
        battle.player().experience(),
        exp_before + exp_gained,
        "losing the turn must not take back the experience already awarded"
    );
    assert_ne!(
        battle.player().evs(),
        evs_before,
        "losing the turn must not take back the EVs already awarded"
    );
}

/// `MOVE_LEER`, non-damaging: with [`GROWL`], no direct action can change
/// HP, so the residual pass alone decides the battle.
const LEER: MoveId = MoveId::LEER;
const GROWL: MoveId = MoveId::GROWL;
const MAY_ROUTE_103_MUDKIP: assets::trainers::TrainerId = assets::trainers::TrainerId(529);

/// `BattleScript_DoTurnDmgEnd`'s `checkteamslost`
/// (`data/battle_scripts_1.s:3746`) runs inside the residual script that
/// fainted the trainer's last mon, so `BattleTurnPassed`'s outcome guard
/// (`battle_main.c:3960`-`:3966`) never reaches the player's own tick.
#[test]
fn a_trainer_last_mons_lethal_residual_tick_ends_the_battle_before_the_players_own_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, ZIGZAGOON, 20, vec![GROWL]);
    player.set_status1(Status1::Poisoned);
    let player_lethal = poison_residual_damage(player.stats().max_hp);
    player.apply_damage(player.stats().max_hp - player_lethal);

    let mut enemy = max_iv_mon(&dex, RATTATA, 20, vec![LEER]);
    enemy.set_status1(Status1::Poisoned);
    let enemy_lethal = poison_residual_damage(enemy.stats().max_hp);
    enemy.apply_damage(enemy.stats().max_hp - enemy_lethal);
    assert!(
        enemy.stats().speed > player.stats().speed,
        "fixture sanity -- the trainer's mon must be processed first in residual"
    );

    // No draw in this turn decides anything the assertions below read.
    let mut rng = SequenceRng::new([0; 40]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, vec![enemy], &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: false,
            damage: enemy_lethal,
        }),
        "the faster trainer mon takes its residual tick first: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "that tick alone must faint the trainer's last mon: {events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::HurtByPoison {
                by_player: true,
                ..
            }
        )),
        "the player's own residual tick must never run once the trainer's \
         last mon has already lost the battle for them: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Fainted { by_player: true })),
        "the player must not faint after the battle is already won: {events:?}"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// `SPECIES_TORCHIC`: its level-16 learnset entry is Peck.
const TORCHIC: u16 = 280;
const TREECKO: u16 = 277;
const SCRATCH: MoveId = MoveId::SCRATCH;
const POUND: MoveId = MoveId::POUND;
/// `MOVE_PECK`, the level-16 entry Torchic has no free slot for.
const PECK: MoveId = MoveId::PECK;

/// A level-up prompt interrupts the residual pass without dropping it:
/// upstream answers the box inside `BattleScript_GiveExp` before
/// `DoBattlerEndTurnEffects` runs (`src/battle_util.c:1912-1923`,
/// `src/battle_main.c:3960-3968`), so the deferred tick arrives with
/// [`Battle::resolve_move_learn`]'s answer, after the deferred replacement.
#[test]
fn a_level_up_prompt_defers_the_residual_tick_to_the_answer_rather_than_dropping_it() {
    let dex = Dex::new();
    let growth_rate = dex.species(assets::SpeciesId(TORCHIC)).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    let mut player = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    player.set_status1(Status1::Poisoned);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
    ];

    let mut rng = SequenceRng::new([u16::MAX; 128]);
    let mut battle =
        Battle::new_trainer(dex, player, MAY_ROUTE_103_MUDKIP, party, &mut rng).unwrap();

    let mut events = Vec::new();
    for _ in 0..8 {
        events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        if battle.pending_move_learn().is_some() {
            break;
        }
    }
    assert!(
        events.contains(&BattleEvent::MoveLearnPrompt { move_id: PECK }),
        "the knockout's award must ask about Peck: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByPoison { .. })),
        "the residual pass waits on the answer with everything else: {events:?}"
    );

    let hp_before = battle.player().current_hp();
    let expected = poison_residual_damage(battle.player().stats().max_hp);
    let answered = battle
        .resolve_move_learn(battle::MoveLearnDecision::Decline, &mut rng)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined { move_id: PECK },
            BattleEvent::TrainerSentOut {
                species: assets::SpeciesId(TREECKO),
                bench_remaining: 0,
            },
            BattleEvent::HurtByPoison {
                by_player: true,
                damage: expected,
            },
        ],
        "the answer releases the replacement, then the deferred tick"
    );
    assert_eq!(battle.player().current_hp(), hp_before - expected);
    assert_eq!(battle.outcome(), None, "the battle plays on");
}
