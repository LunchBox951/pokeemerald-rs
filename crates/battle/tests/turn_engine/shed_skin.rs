//! Shed Skin's end-turn cure draw through real turns. The draw itself is
//! `battle::status1::draws_shed_skin_cure`; these tests cover its turn wiring.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::status1::poison_residual_damage;
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, MoveLearnDecision, PlayerAction, Status1,
};

const TACKLE: MoveId = MoveId::TACKLE;
const SCRATCH: MoveId = MoveId::SCRATCH;
const GROWL: MoveId = MoveId::GROWL;
const LEER: MoveId = MoveId::LEER;
const POUND: MoveId = MoveId::POUND;
/// [`SEVIPER`]'s level-16 learnset entry.
const POISON_TAIL: MoveId = MoveId::POISON_TAIL;

/// Shed Skin in its only ability slot; at level 10 faster than [`ZIGZAGOON`]
/// and [`DRATINI`].
const SEVIPER: u16 = 379;
/// Has no ability the residual pass reads; slower than [`SEVIPER`].
const ZIGZAGOON: u16 = 288;
/// Shed Skin in its only ability slot; slower than [`SEVIPER`].
const DRATINI: u16 = 147;
/// A level-5 Rattata that a level-50 [`SEVIPER`] knocks out in one Tackle.
const RATTATA: u16 = 19;
const TREECKO: u16 = 277;
/// Serene Grace in its primary ability slot.
const DUNSPARCE: u16 = 206;
/// A 30% poison secondary that Serene Grace doubles to 60%. The doubling is
/// unmodelled, so `secondary::ensure_admissible` refuses the pairing whenever
/// the poison could newly land.
const POISON_STING: MoveId = MoveId::POISON_STING;
/// Its party is replaced with two [`TREECKO`] so the bench survives the first
/// knockout.
const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

/// Draws for a turn where both battlers use Tackle: construction's turn-number
/// draw, the turn-start draw, the enemy's move selection, then per acting
/// battler accuracy, crit, damage roll, and a discarded secondary-effect draw.
/// The `1`s are the crit draws, which do not crit. No speed or order tie draws.
const BOTH_BATTLERS_TACKLE: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];
/// [`BOTH_BATTLERS_TACKLE`] truncated after the player's hit, which knocks the
/// enemy out before it acts.
const PLAYER_ACTS_ALONE: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];

/// The cure succeeds exactly when the draw is a multiple of three.
const CURE_MISS_DRAW: u16 = 1;
const CURE_HIT_DRAW: u16 = 0;

#[test]
fn a_healthy_shed_skin_holder_draws_nothing_and_reports_no_cure() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    assert_eq!(player.ability(), assets::AbilityId::SHED_SKIN);
    assert_eq!(player.status1(), Status1::Healthy);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    // No spare draws: a Shed Skin draw here would exhaust the sequence and panic.
    let mut rng = SequenceRng::new(BOTH_BATTLERS_TACKLE);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("a healthy Shed Skin holder must not touch the RNG for its own residual");

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })),
        "{events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

#[test]
fn a_statused_shed_skin_holder_that_misses_the_cure_draw_still_takes_its_poison_tick() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let player_max_hp = player.stats().max_hp;
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(
        BOTH_BATTLERS_TACKLE
            .into_iter()
            .chain([CURE_MISS_DRAW])
            .collect::<Vec<_>>(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })),
        "the scripted miss must not cure the status: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::HurtByPoison {
            by_player: true,
            damage: poison_residual_damage(player_max_hp),
        }),
        "a missed cure draw leaves the same pass's own poison tick to run: {events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Poisoned);
}

#[test]
fn a_statused_shed_skin_holder_that_hits_the_cure_draw_clears_its_status_before_its_own_poison_tick(
) {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    let mut rng = SequenceRng::new(
        BOTH_BATTLERS_TACKLE
            .into_iter()
            .chain([CURE_HIT_DRAW])
            .collect::<Vec<_>>(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::ShedSkinCured {
            by_player: true,
            status: Status1::Poisoned,
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::HurtByPoison { .. })),
        "Shed Skin (`ABILITYEFFECT_ENDTURN`) runs before `ENDTURN_POISON` in a battler's \
         pass, so a cured status never ticks: {events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
}

#[test]
fn both_battlers_shed_skin_cures_resolve_in_the_turns_own_order() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, DRATINI, 10, vec![TACKLE]);
    enemy.set_status1(Status1::Poisoned);
    assert_eq!(enemy.ability(), assets::AbilityId::SHED_SKIN);

    let mut rng = SequenceRng::new(
        BOTH_BATTLERS_TACKLE
            .into_iter()
            .chain([CURE_HIT_DRAW, CURE_HIT_DRAW])
            .collect::<Vec<_>>(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let player_cure_index = events
        .iter()
        .position(|event| {
            event
                == &BattleEvent::ShedSkinCured {
                    by_player: true,
                    status: Status1::Poisoned,
                }
        })
        .unwrap_or_else(|| panic!("the player's cure must be reported: {events:?}"));
    let enemy_cure_index = events
        .iter()
        .position(|event| {
            event
                == &BattleEvent::ShedSkinCured {
                    by_player: false,
                    status: Status1::Poisoned,
                }
        })
        .unwrap_or_else(|| panic!("the enemy's cure must be reported: {events:?}"));
    assert!(
        player_cure_index < enemy_cure_index,
        "the faster Seviper's own residual pass runs before Dratini's: {events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.enemy().status1(), Status1::Healthy);
}

/// A won battle takes `HandleEndTurn_BattleWon`, skipping
/// `DoBattlerEndTurnEffects` (`src/battle_main.c:4937`-`:4952`).
#[test]
fn a_direct_hit_wild_ko_ends_the_battle_before_the_winners_shed_skin_cure_can_draw() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, SEVIPER, 50, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);

    let mut rng = SequenceRng::new(PLAYER_ACTS_ALONE);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("the fixture must not need a draw beyond the fixed array");

    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the fixture must knock the wild mon out with a direct hit: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })),
        "a knockout that already won the battle leaves no turn for the winner's \
         own residual to run in: {events:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Poisoned);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}

/// The residual pass held back by a move-learn prompt runs when the prompt is
/// answered (`src/battle_util.c:1912`-`:1923`, `src/battle_main.c:3960`-`:3968`),
/// so `resolve_move_learn`'s `rng` must feed the deferred cure draw.
#[test]
fn resolve_move_learn_threads_rng_into_the_deferred_shed_skin_cure_draw() {
    let dex = Dex::new();
    let growth_rate = dex.species(SpeciesId(SEVIPER)).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    let mut player = max_iv_mon(&dex, SEVIPER, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    player.set_status1(Status1::Poisoned);
    let party = vec![
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
        max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]),
    ];

    // `u16::MAX` is a multiple of 3, so every Shed Skin draw cures.
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
        events.contains(&BattleEvent::MoveLearnPrompt {
            move_id: POISON_TAIL
        }),
        "the knockout's award must ask about Poison Tail: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::ShedSkinCured { .. })),
        "the residual pass waits on the answer with everything else: {events:?}"
    );

    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline, &mut rng)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined {
                move_id: POISON_TAIL
            },
            BattleEvent::TrainerSentOut {
                species: SpeciesId(TREECKO),
                bench_remaining: 0,
            },
            BattleEvent::ShedSkinCured {
                by_player: true,
                status: Status1::Poisoned,
            },
        ],
        "the answer releases the replacement, then the deferred cure draw \
         the caller's own rng argument must feed: {answered:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.outcome(), None, "the battle plays on");
}

/// Construction admits Poison Sting only because the statused player cannot be
/// poisoned. The draw 48 is a multiple of 3 (cures Shed Skin) and lies in
/// `30..60`, so it would poison under Serene Grace's doubled chance but not the
/// executor's undoubled one.
#[test]
fn serene_grace_poison_sting_after_a_shed_skin_cure_is_not_silently_undoubled() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, DRATINI, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let enemy = max_iv_mon(&dex, DUNSPARCE, 10, vec![POISON_STING]);
    assert_eq!(enemy.ability(), assets::AbilityId::SERENE_GRACE);

    let mut rng = SequenceRng::new([48u16; 256]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .expect("a statused target makes the constructor admit Serene Grace");
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        first.contains(&BattleEvent::ShedSkinCured {
            by_player: true,
            status: Status1::Poisoned
        }),
        "{first:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);

    let draws_before_rejected_turn = rng.draws();
    let turn_error = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect_err(
            "the cure must make the next turn re-screen Poison Sting, not silently reuse \
             construction's now-stale admission",
        );
    assert_eq!(
        rng.draws(),
        draws_before_rejected_turn,
        "a pre-turn admission rejection consumes no RNG"
    );
    assert_eq!(
        turn_error.error(),
        BattleError::UnportedAbilityInteraction(assets::AbilityId::SERENE_GRACE)
    );
    assert!(
        turn_error.events().is_empty(),
        "the re-screen runs before this turn's own RNG or events, like construction's own \
         admission check: {turn_error:?}"
    );
    assert_eq!(
        battle.player().status1(),
        Status1::Healthy,
        "a pre-turn rejection runs no move"
    );
}

/// A faster player's wild escape always succeeds before the enemy acts
/// (`src/battle_util.c:463`-`:472`), so the cure must not refuse `Run`.
#[test]
fn faster_player_can_still_run_after_a_shed_skin_cure_flips_serene_grace_admission() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, DRATINI, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let enemy = max_iv_mon(&dex, DUNSPARCE, 10, vec![POISON_STING]);
    assert!(player.stats().speed >= enemy.stats().speed);

    let mut rng = SequenceRng::new([48u16; 256]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(battle.player().status1(), Status1::Healthy);

    let ran = battle.take_turn(PlayerAction::Run, &mut rng);
    assert!(ran.is_ok(), "a guaranteed escape is refused: {ran:?}");
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
}

/// A slower player's escape can fail, letting Poison Sting execute undoubled.
#[test]
fn slower_player_run_after_a_shed_skin_cure_still_refuses_serene_grace() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, DRATINI, 10, vec![TACKLE]);
    player.set_status1(Status1::Poisoned);
    let mut enemy = max_iv_mon(&dex, DUNSPARCE, 10, vec![POISON_STING]);
    while enemy.stats().speed <= player.stats().speed {
        enemy = max_iv_mon(&dex, DUNSPARCE, enemy.level() + 5, vec![POISON_STING]);
    }

    let mut rng = SequenceRng::new([48u16; 256]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(battle.player().status1(), Status1::Healthy);

    let draws = rng.draws();
    let err = battle
        .take_turn(PlayerAction::Run, &mut rng)
        .expect_err("a run that can fail must still be re-screened");
    assert_eq!(rng.draws(), draws);
    assert_eq!(
        err.error(),
        BattleError::UnportedAbilityInteraction(assets::AbilityId::SERENE_GRACE)
    );
    assert!(err.events().is_empty());
}
