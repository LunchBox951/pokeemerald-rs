//! Battle lifecycle, terminal outcomes, and experience-award behavior.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleError, BattleEvent, BattleOutcome, Dex, PlayerAction};

#[test]
fn take_turn_after_the_battle_ended_is_an_error() {
    let dex = Dex::new();
    // Level 50 Charmander (fast, strong Tackle) vs level 2 Rattata: the
    // player one-shots it, so one turn reaches a terminal state.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]); // Rattata

    // One RNG for the whole battle: battle-start turn number, then the
    // turn's own turn number, the opponent's move pick, and the player's
    // hit (accuracy / no crit / best roll / effect chance). No speed-tie
    // draw at this gap.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let _ = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(battle.outcome().is_some());
    assert_eq!(rng.draws(), 7);
    // The rejected call must not draw: the sequence is exhausted, so a
    // stray draw would panic rather than silently pass.
    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::BattleAlreadyOver);
    assert!(
        rejected.events().is_empty(),
        "a call rejected before the turn began has no events to report"
    );
    assert_eq!(rng.draws(), 7, "an already-over battle draws nothing");
}

#[test]
fn full_wild_battle_runs_to_a_faint_and_reports_victory() {
    let dex = Dex::new();
    // A genuinely multi-turn, evenly matched fight, hand computed from
    // the same formulas the unit tests pin:
    //
    //   player Rattata L5 max-IV Hardy: atk 12, def 10, speed 13, hp 19
    //   enemy Bulbasaur L5 max-IV Hardy: atk 11, def 11, speed 11, hp 21
    //
    // Rattata is faster, so it moves first every turn.
    //   Rattata's Tackle: 12*35=420, *4=1680, /11=152, /50=3, +2=5,
    //     STAB (Normal on a Normal-type) *15/10 = 7 per hit.
    //   Bulbasaur's Tackle: 11*35=385, *4=1540, /10=154, /50=3, +2=5,
    //     no STAB (Grass/Poison), Normal is neutral into both = 5.
    // So Bulbasaur (21 hp) falls on the third player hit (7/14/21) while
    // Rattata (19 hp) has taken two 5s and is at 9.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    // One scripted RNG for the entire battle, in the module docs' order.
    // Per full turn: turn number, opponent's move pick, then two hits of
    // (accuracy / no crit / best roll / effect chance) -- no speed-tie
    // draw, the speeds differ. The last turn stops after the player's
    // hit: the enemy faints, so the second mover never acts and never
    // draws (the effect-chance draw still lands, ahead of tryfaintmon).
    let mut rng = SequenceRng::new([
        0, // Battle::new: battle-start turn number
        0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 1
        0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 2
        0, 0, 0, 1, 0, 0, // turn 3: player's hit faints the enemy
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let mut turns = 0;
    let mut won = false;
    // Cap the loop so a logic bug fails the test instead of hanging.
    for _ in 0..20 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        turns += 1;
        if let Some(BattleEvent::Ended(outcome)) = events.last() {
            assert_eq!(*outcome, BattleOutcome::PlayerWon);
            won = true;
            break;
        }
    }
    assert!(won, "battle did not conclude within 20 turns");
    assert_eq!(turns, 3, "three player hits of 7 to drop a 21-hp Bulbasaur");
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.player().current_hp(),
        9,
        "two enemy hits of 5 from 19"
    );
    assert_eq!(
        rng.draws(),
        27,
        "1 (battle start) + 10 + 10 (full turns) + 6 (final turn)"
    );
}

#[test]
fn losing_the_battle_reports_defeat_and_awards_no_exp() {
    let dex = Dex::new();
    // Slow L5 Rattata against a fast L50 Charmander: the enemy moves
    // first and its Tackle overkills the 19-HP player, so the battle
    // ends in defeat before the player's own queued move ever executes.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let player_max_hp = player.stats().max_hp;

    // battle start, turn number, enemy pick, enemy hit (accuracy / no
    // crit / best roll / effect chance). The script is exhausted: the
    // player's move drawing anything after the loss would panic.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: player_max_hp, // overkill capped at the HP bar
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "defeat reports the faint and the loss -- and no ExpGained: \
         exp is the winner's, and the player lost"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert_eq!(battle.player().current_hp(), 0);
    assert_eq!(
        battle.player().moves()[0].pp,
        35,
        "the fainted player's queued move never executed, so no PP moved"
    );
    assert_eq!(rng.draws(), 7);
}

#[test]
fn a_max_level_player_gains_no_exp_and_no_exp_event_on_victory() {
    let dex = Dex::new();
    // Cmd_getexp case 2 (battle_script_commands.c:3351-:3356): a
    // MAX_LEVEL recipient gets gBattleMoveDamage = 0 and the state
    // machine jumps past the "gained EXP" string -- no exp, no message,
    // so no ExpGained event here either.
    let player = max_iv_mon(&dex, 4, 100, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]);

    // battle start, turn number, enemy pick, the player's one-shot hit.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the win itself is unchanged: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::ExpGained(_))),
        "a level-100 player gains no exp and sees no exp event: {events:?}"
    );
    assert_eq!(
        battle.player().evs(),
        battle::Evs::default(),
        "issue #415: a MAX_LEVEL recipient is excluded from `MonGainEVs` \
         too -- the same `Cmd_getexp` case 2 guard that zeroes the exp \
         award skips the whole body, `gain_evs` included"
    );
    assert_eq!(rng.draws(), 7);
}

#[test]
fn a_fainted_battler_is_rejected_before_the_battle_start_draw() {
    let dex = Dex::new();
    // `apply_damage` is public, so a 0-HP mon is constructible — but
    // upstream never starts a wild battle around one, and `take_turn`
    // checks HP only after a hit, so `Battle::new` refuses it.
    let mut fainted = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    fainted.apply_damage(fainted.stats().max_hp);
    let healthy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);

    // Empty scripts: a draw before the rejection panics the SequenceRng
    // rather than silently passing.
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(
            dex.clone(),
            fainted.clone(),
            healthy.clone(),
            false,
            &mut rng
        )
        .unwrap_err(),
        BattleError::FaintedBattler(true)
    );
    assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex, healthy, fainted, false, &mut rng).unwrap_err(),
        BattleError::FaintedBattler(false)
    );
    assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");
}

/// A wild battle's final knockout normally ends it on the spot — but a
/// level-up move-learn prompt raised by that knockout's award holds the end
/// back: upstream runs the whole level-up script, yes/no box included,
/// before anything after the faint (`HandleFaintedMonActions` completes
/// `BattleScript_GiveExp` in its case 1 before case 4's
/// `BattleScript_HandleFaintedMon`, `battle_util.c:1894`-`:1951`). The
/// `Ended` event and the outcome arrive only with the answer.
#[test]
fn a_wild_knockouts_prompt_defers_the_battles_end_until_it_is_answered() {
    use battle::MoveLearnDecision;

    const TORCHIC: u16 = 280;
    const RATTATA: u16 = 19;
    const SCRATCH: MoveId = MoveId(10);
    const GROWL: MoveId = MoveId(45);
    const TACKLE: MoveId = MoveId(33);
    const LEER: MoveId = MoveId(43);
    /// `MOVE_PECK` — Torchic's level-16 learnset entry.
    const PECK: MoveId = MoveId(64);

    let dex = Dex::new();
    // A full-moveset Torchic one experience point short of level 16, so the
    // knockout's award crosses the threshold and offers Peck with nowhere
    // to put it.
    let mut player = max_iv_mon(&dex, TORCHIC, 15, vec![SCRATCH, GROWL, TACKLE, LEER]);
    let growth_rate = dex.species(player.species()).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    assert!(player
        .apply_experience(&dex, level_16 - 1 - player.experience())
        .unwrap()
        .is_none());
    let enemy = max_iv_mon(&dex, RATTATA, 2, vec![TACKLE]);

    // Battle-start turn number; turn number, opponent pick, player's hit
    // (accuracy / crit / roll / effect chance). No tie draws.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::MoveLearnPrompt { move_id: PECK }),
        "the one-hit knockout's award must ask about Peck: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::Ended(_))),
        "the battle's end waits on the answer: {events:?}"
    );
    assert_eq!(
        battle.outcome(),
        None,
        "no outcome while the question is open"
    );
    assert_eq!(battle.player().level(), 16);

    let answered = battle
        .resolve_move_learn(MoveLearnDecision::Decline)
        .unwrap();
    assert_eq!(
        answered,
        vec![
            BattleEvent::MoveLearnDeclined { move_id: PECK },
            // The deferred end, and nothing else: a wild battle pays no
            // money (`Cmd_getmoneyreward` is trainer-only).
            BattleEvent::Ended(BattleOutcome::PlayerWon),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}
