//! Shed Skin's end-turn cure draw, driven through real turns.
//!
//! The draw shape lives in `battle::status1::draws_shed_skin_cure`; what is
//! pinned here is the turn wiring that draw cannot reach on its own: a
//! healthy holder draws nothing, a statused holder's miss and cure outcomes,
//! multi-battler residual order, the cure mutation and its event, that the
//! cure precedes the same battler's poison tick, and that a battle already
//! decided before residual never reaches the draw at all.

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::status1::poison_residual_damage;
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, MoveLearnDecision, PlayerAction, Status1,
};

/// `MOVE_TACKLE`.
const TACKLE: MoveId = MoveId(33);
/// `MOVE_SCRATCH`.
const SCRATCH: MoveId = MoveId(10);
/// `MOVE_GROWL`.
const GROWL: MoveId = MoveId(45);
/// `MOVE_LEER`.
const LEER: MoveId = MoveId(43);
/// `MOVE_POUND`.
const POUND: MoveId = MoveId(1);
/// `MOVE_POISON_TAIL`, [`SEVIPER`]'s level-16 learnset entry.
const POISON_TAIL: MoveId = MoveId(342);

/// `SPECIES_SEVIPER`: Poison, Shed Skin in its only ability slot, raw Speed
/// 21 at level 10 -- faster than [`ZIGZAGOON`] and [`DRATINI`].
const SEVIPER: u16 = 379;
/// `SPECIES_ZIGZAGOON`: an ordinary Normal-type opponent with no ability
/// this residual pass reads, slower than [`SEVIPER`].
const ZIGZAGOON: u16 = 288;
/// `SPECIES_DRATINI`: Dragon, Shed Skin in its only ability slot, slower
/// than [`SEVIPER`].
const DRATINI: u16 = 147;
/// `SPECIES_RATTATA`, a weak level-5 fixture [`SEVIPER`] one-shots with
/// Tackle at level 50, so the wild win decides the battle before residual
/// ever runs.
const RATTATA: u16 = 19;
/// `SPECIES_TREECKO`, [`MAY_ROUTE_103_MUDKIP`]'s two-mon bench.
const TREECKO: u16 = 277;
/// `SPECIES_DUNSPARCE`: Serene Grace in its primary ability slot, the
/// ability `secondary::ensure_admissible` refuses to admit once a Poison
/// Sting could newly land.
const DUNSPARCE: u16 = 206;
/// `MOVE_POISON_STING`, a 30% `EFFECT_POISON_HIT` move Serene Grace doubles
/// to 60%; this crate does not model that doubling, so
/// `secondary::ensure_admissible` refuses it instead.
const POISON_STING: MoveId = MoveId(40);
/// May's Route 103 starter-rival trainer, whose party this fixture replaces
/// with two [`TREECKO`] so the bench survives the first knockout.
const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

/// Both battlers use Tackle, every roll on its default branch: turn number,
/// the (absent) speed-tie draw, the enemy's selection, the (absent) order
/// tie, the player's hit (accuracy / crit / roll / discarded effect chance),
/// then the enemy's hit the same way.
const BOTH_BATTLERS_TACKLE: [u16; 11] = [0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0];
/// The player's Tackle fells the enemy before it acts, leaving no residual
/// pass for the turn to reach.
const PLAYER_ACTS_ALONE: [u16; 7] = [0, 0, 0, 0, 1, 0, 0];

/// A one-in-three draw that misses (`1 % 3 != 0`).
const CURE_MISS_DRAW: u16 = 1;
/// A one-in-three draw that cures (`0 % 3 == 0`).
const CURE_HIT_DRAW: u16 = 0;

#[test]
fn a_healthy_shed_skin_holder_draws_nothing_and_reports_no_cure() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, SEVIPER, 10, vec![TACKLE]);
    assert_eq!(player.ability(), assets::AbilityId::SHED_SKIN);
    assert_eq!(player.status1(), Status1::Healthy);
    let enemy = max_iv_mon(&dex, ZIGZAGOON, 10, vec![TACKLE]);

    // Exactly the draws an ordinary Tackle exchange needs: if a healthy
    // holder's Shed Skin drew anyway, this array would run out and the turn
    // would panic instead of completing.
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
        "ABILITYEFFECT_ENDTURN's Shed Skin case precedes ENDTURN_POISON within the same \
         battler's pass, so a cured status never ticks the same turn: {events:?}"
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

/// A won battle routes through `HandleEndTurn_BattleWon`, never
/// `HandleEndTurn_ContinueBattle`'s `DoBattlerEndTurnEffects`
/// (`src/battle_main.c:4937`-`:4952`), so a surviving statused winner's own
/// Shed Skin draw never runs the turn its win is decided.
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

/// `Battle::resolve_move_learn` releases the residual pass a pending prompt
/// held back (`src/battle_util.c:1912`-`:1923`, `:3960`-`:3968`), so the
/// caller's `rng` must still feed a Shed Skin holder's own cure draw once
/// the deferred pass finally runs -- exactly the boundary
/// `crate::flow::move_learn::settle_move_learn_prompts` threads its own
/// `rng` across in `crates/pokeemerald-rs`.
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

    // `u16::MAX` clears every accuracy/crit/damage-roll branch's default
    // arm and is itself a multiple of 3, so every Shed Skin draw it reaches
    // cures.
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

/// Construction admitted Serene Grace's Poison Sting only because the
/// statused player could not be poisoned; once Shed Skin cures the player,
/// the pre-turn re-screen must refuse the next turn before its own RNG runs,
/// not let Poison Sting silently reach the executor's undoubled 30% chance.
/// A draw of 48 would land under upstream's doubled 60% threshold.
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

    let turn_error = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect_err(
            "the cure must make the next turn re-screen Poison Sting, not silently reuse \
             construction's now-stale admission",
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
