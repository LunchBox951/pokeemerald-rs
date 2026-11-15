use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const EARLY_BIRD_KANGASKHAN: u16 = SpeciesId::KANGASKHAN.0;
const TACKLE_IMMUNE_GASTLY: u16 = SpeciesId::GASTLY.0;

const SLEEPER_LEVEL: u8 = 5;
const SLOWER_ENEMY_LEVEL: u8 = 4;
const ENEMY_SLEEP_ACTIONS: u8 = 5;

const BATTLE_START_DRAWS: [u16; 1] = [0];
const TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS: [u16; 2] = [0, 0];

const PASSING_ACCURACY_DRAW: u16 = 0;
const NON_CRITICAL_DRAW: u16 = 1;
const IMMUNE_HIT_DAMAGE_ROLL: u16 = 0;
const DISCARDED_EFFECT_CHANCE_DRAW: u16 = 0;
const TACKLE_INTO_GHOST_DRAWS: [u16; 4] = [
    PASSING_ACCURACY_DRAW,
    NON_CRITICAL_DRAW,
    IMMUNE_HIT_DAMAGE_ROLL,
    DISCARDED_EFFECT_CHANCE_DRAW,
];

fn sleeper(dex: &Dex, species: u16, turns: u8) -> BattlePokemon {
    let mut mon = max_iv_mon_with_personality(dex, species, SLEEPER_LEVEL, vec![TACKLE], 0);
    mon.set_status1(Status1::Asleep { turns });
    mon
}

fn slower_sleeping_tackle_immune_enemy(dex: &Dex) -> BattlePokemon {
    let mut mon = max_iv_mon_with_personality(
        dex,
        TACKLE_IMMUNE_GASTLY,
        SLOWER_ENEMY_LEVEL,
        vec![TACKLE],
        0,
    );
    mon.set_status1(Status1::Asleep {
        turns: ENEMY_SLEEP_ACTIONS,
    });
    mon
}

fn fast_asleep(by_player: bool) -> BattleEvent {
    BattleEvent::FastAsleep {
        by_player,
        move_id: TACKLE,
    }
}

#[test]
fn a_remaining_sleep_duration_cancels_the_move_without_spending_pp_or_a_draw() {
    let dex = Dex::new();
    let player = sleeper(&dex, TACKLE_IMMUNE_GASTLY, 3);
    let starting_pp = player.moves()[0].pp;
    let enemy = slower_sleeping_tackle_immune_enemy(&dex);

    let mut rng = SequenceRng::new(
        [
            &BATTLE_START_DRAWS[..],
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
        ]
        .concat(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(events, vec![fast_asleep(true), fast_asleep(false)]);
    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 2 });
    assert_eq!(battle.enemy().status1(), Status1::Asleep { turns: 4 });
    assert_eq!(battle.player().moves()[0].pp, starting_pp);
    rng.assert_exhausted();
}

#[test]
fn a_normal_sleeper_wakes_on_the_action_that_exhausts_its_duration_and_then_acts() {
    let dex = Dex::new();
    let player = sleeper(&dex, TACKLE_IMMUNE_GASTLY, 2);
    let starting_pp = player.moves()[0].pp;
    let enemy = slower_sleeping_tackle_immune_enemy(&dex);

    let mut rng = SequenceRng::new(
        [
            &BATTLE_START_DRAWS[..],
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
            &TACKLE_INTO_GHOST_DRAWS,
        ]
        .concat(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first, vec![fast_asleep(true), fast_asleep(false)]);
    assert_eq!(battle.player().status1(), Status1::Asleep { turns: 1 });

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        second,
        vec![
            BattleEvent::WokeUp { by_player: true },
            BattleEvent::UsedMove {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            fast_asleep(false),
        ],
        "expiry lets the selected move proceed in the same action: {second:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    assert_eq!(battle.player().moves()[0].pp, starting_pp - 1);
    rng.assert_exhausted();
}

#[test]
fn early_bird_loses_two_turns_per_action_and_wakes_sooner() {
    let dex = Dex::new();
    let player = sleeper(&dex, EARLY_BIRD_KANGASKHAN, 3);
    let enemy = slower_sleeping_tackle_immune_enemy(&dex);

    let mut rng = SequenceRng::new(
        [
            &BATTLE_START_DRAWS[..],
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
            &TACKLE_INTO_GHOST_DRAWS,
        ]
        .concat(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let first = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(first, vec![fast_asleep(true), fast_asleep(false)]);
    assert_eq!(
        battle.player().status1(),
        Status1::Asleep { turns: 1 },
        "Early Bird subtracts two"
    );
    assert_eq!(
        battle.enemy().status1(),
        Status1::Asleep { turns: 4 },
        "the Gastly without Early Bird subtracts one"
    );

    let second = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        second[0],
        BattleEvent::WokeUp { by_player: true },
        "one turn left is below the two-turn decrement: {second:?}"
    );
    assert_eq!(battle.player().status1(), Status1::Healthy);
    rng.assert_exhausted();
}

#[test]
fn sleep_cancels_ahead_of_confusion_and_full_paralysis_draws() {
    let dex = Dex::new();
    let mut player = sleeper(&dex, TACKLE_IMMUNE_GASTLY, 3);
    player.volatiles_mut().set_confusion(3);
    let enemy = slower_sleeping_tackle_immune_enemy(&dex);

    let mut rng = SequenceRng::new(
        [
            &BATTLE_START_DRAWS[..],
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
        ]
        .concat(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(events, vec![fast_asleep(true), fast_asleep(false)]);
    assert!(
        battle.player().volatiles().confused(),
        "sleep cancellation leaves confusion active"
    );
    assert_eq!(
        battle.player().volatiles().confusion_turns,
        3,
        "sleep cancellation must not tick confusion"
    );
    rng.assert_exhausted();
}

#[test]
fn a_waking_battler_goes_on_to_tick_its_confusion_in_the_same_action() {
    let dex = Dex::new();
    let mut player = sleeper(&dex, TACKLE_IMMUNE_GASTLY, 1);
    player.volatiles_mut().set_confusion(1);
    let enemy = slower_sleeping_tackle_immune_enemy(&dex);

    let mut rng = SequenceRng::new(
        [
            &BATTLE_START_DRAWS[..],
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
            &TACKLE_INTO_GHOST_DRAWS,
        ]
        .concat(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::WokeUp { by_player: true },
            BattleEvent::SnappedOutOfConfusion { by_player: true },
            BattleEvent::UsedMove {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            fast_asleep(false),
        ]
    );
    rng.assert_exhausted();
}

#[test]
fn serene_grace_poison_is_rejected_for_short_and_long_sleep_before_any_draw() {
    // Admission ignores sleep duration because an earlier action can wake the target.
    // Upstream: `src/battle_util.c:2030-2053`; `src/battle_script_commands.c:2334-2335,2912-2913`.
    for (species, turns) in [
        (SpeciesId::ZIGZAGOON, 1),
        (SpeciesId::KANGASKHAN, 2),
        (SpeciesId::ZIGZAGOON, 5),
    ] {
        let dex = Dex::new();
        let mut player = max_iv_mon(&dex, species.0, 10, vec![TACKLE]);
        player.set_status1(Status1::Asleep { turns });
        let enemy = max_iv_mon(&dex, SpeciesId::DUNSPARCE.0, 10, vec![MoveId::POISON_STING]);
        assert_eq!(enemy.ability(), AbilityId::SERENE_GRACE);

        let mut rng = SequenceRng::new([]);
        let refusal = Battle::new(dex, player, enemy, false, &mut rng).err();
        assert_eq!(
            refusal,
            Some(BattleError::UnportedAbilityInteraction(
                AbilityId::SERENE_GRACE
            )),
            "Serene Grace poison admission must conservatively reject {species:?} with {turns} sleep turns before drawing"
        );
        rng.assert_exhausted();
    }
}

#[test]
fn a_zero_turn_sleep_field_is_a_healthy_battler_and_never_wakes() {
    let dex = Dex::new();
    let player = sleeper(&dex, TACKLE_IMMUNE_GASTLY, 0);
    let enemy = slower_sleeping_tackle_immune_enemy(&dex);
    let mut rng = SequenceRng::new(
        [
            &BATTLE_START_DRAWS[..],
            &TURN_NUMBER_AND_ENEMY_SELECTION_DRAWS,
            &TACKLE_INTO_GHOST_DRAWS,
        ]
        .concat(),
    );
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert!(
        !battle.player().status1().is_asleep(),
        "a zero sleep duration is not asleep"
    );
    assert_eq!(
        battle.player().status1(),
        Status1::Healthy,
        "a zero sleep field must normalize to Healthy"
    );
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        !events.contains(&BattleEvent::WokeUp { by_player: true }),
        "a battler with no sleep left has nothing to wake from: {events:?}"
    );
}

const DEPLETED_SLOT: u16 = 0;
const CONFUSION_CONTINUES_DRAW: u16 = 1;
const CONFUSION_SELF_HIT_DRAW: u16 = 0;
const SELF_HIT_DAMAGE_ROLL_DRAW: u16 = 0;
const SLEEP_EXEMPT_ENEMY_LEVEL: u8 = 4;
const SLEEP_EXEMPT_TURNS: u8 = 5;
const SLEEP_EXEMPT_CONFUSION: u8 = 3;

/// A slower, sleeping, confused wild enemy whose first slot is `exempt_move`
/// with no PP left, beside a PP-positive Tackle.
fn sleeping_confused_enemy_with_depleted(dex: &Dex, exempt_move: MoveId) -> BattlePokemon {
    let mut enemy = max_iv_mon_with_personality(
        dex,
        SpeciesId::ZIGZAGOON.0,
        SLEEP_EXEMPT_ENEMY_LEVEL,
        vec![exempt_move, TACKLE],
        0,
    );
    while enemy.moves()[0].pp > 0 {
        enemy.deduct_pp(0).unwrap();
    }
    enemy.set_status1(Status1::Asleep {
        turns: SLEEP_EXEMPT_TURNS,
    });
    enemy.volatiles_mut().set_confusion(SLEEP_EXEMPT_CONFUSION);
    enemy
}

fn player_splash(dex: &Dex) -> BattlePokemon {
    max_iv_mon_with_personality(
        dex,
        SpeciesId::ZIGZAGOON.0,
        SLEEPER_LEVEL,
        vec![MoveId::SPLASH],
        0,
    )
}

fn depleted_slot_proceeds_past_the_sleep_canceller_to_the_no_pp_abort(exempt_move: MoveId) {
    let dex = Dex::new();
    let player = player_splash(&dex);
    let enemy = sleeping_confused_enemy_with_depleted(&dex, exempt_move);

    // Battle start, turn number, the wild slot draw, then the confusion coin.
    let mut rng = SequenceRng::new([
        BATTLE_START_DRAWS[0],
        0,
        DEPLETED_SLOT,
        CONFUSION_CONTINUES_DRAW,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::UsedMove {
                by_player: true,
                move_id: MoveId::SPLASH,
            },
            BattleEvent::NothingHappened {
                by_player: true,
                move_id: MoveId::SPLASH,
            },
            BattleEvent::Confused { by_player: false },
            BattleEvent::UsedMove {
                by_player: false,
                move_id: exempt_move,
            },
            BattleEvent::FailedNoPp {
                by_player: false,
                move_id: exempt_move,
            },
        ],
        "the exempt move skips FastAsleep, ticks confusion and stops at the no-PP abort: {events:?}"
    );
    assert_eq!(
        battle.enemy().status1(),
        Status1::Asleep {
            turns: SLEEP_EXEMPT_TURNS - 1
        },
        "the sleep decrement still runs"
    );
    assert_eq!(
        battle.enemy().volatiles().confusion_turns,
        SLEEP_EXEMPT_CONFUSION - 1
    );
    assert_eq!(battle.enemy().moves()[0].pp, 0);
    rng.assert_exhausted();
}

#[test]
fn a_sleeping_enemy_selecting_depleted_snore_ticks_confusion_then_aborts_for_no_pp() {
    depleted_slot_proceeds_past_the_sleep_canceller_to_the_no_pp_abort(MoveId::SNORE);
}

#[test]
fn a_sleeping_enemy_selecting_depleted_sleep_talk_ticks_confusion_then_aborts_for_no_pp() {
    depleted_slot_proceeds_past_the_sleep_canceller_to_the_no_pp_abort(MoveId::SLEEP_TALK);
}

#[test]
fn a_sleeping_enemy_s_depleted_snore_can_hurt_itself_in_confusion_before_the_no_pp_abort() {
    let dex = Dex::new();
    let player = player_splash(&dex);
    let enemy = sleeping_confused_enemy_with_depleted(&dex, MoveId::SNORE);
    let starting_hp = enemy.current_hp();

    let mut rng = SequenceRng::new([
        BATTLE_START_DRAWS[0],
        0,
        DEPLETED_SLOT,
        CONFUSION_SELF_HIT_DRAW,
        SELF_HIT_DAMAGE_ROLL_DRAW,
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[2..],
        [
            BattleEvent::Confused { by_player: false },
            BattleEvent::ConfusionSelfHit {
                by_player: false,
                damage: 4,
            },
        ],
        "{events:?}"
    );
    assert_eq!(
        battle.enemy().status1(),
        Status1::Asleep {
            turns: SLEEP_EXEMPT_TURNS - 1
        }
    );
    assert_eq!(
        battle.enemy().volatiles().confusion_turns,
        SLEEP_EXEMPT_CONFUSION - 1
    );
    assert_eq!(battle.enemy().current_hp(), starting_hp - 4);
    rng.assert_exhausted();
}
