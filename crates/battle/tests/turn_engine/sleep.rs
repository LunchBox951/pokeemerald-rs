use crate::common::{max_iv_mon, max_iv_mon_with_personality, SequenceRng};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleEvent, BattlePokemon, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const EARLY_BIRD_KANGASKHAN: SpeciesId = SpeciesId::KANGASKHAN;
const TACKLE_IMMUNE_GASTLY: SpeciesId = SpeciesId::GASTLY;

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

fn sleeper(dex: &Dex, species: SpeciesId, turns: u8) -> BattlePokemon {
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
        let mut player = max_iv_mon(&dex, species, 10, vec![TACKLE]);
        player.set_status1(Status1::Asleep { turns });
        let enemy = max_iv_mon(&dex, SpeciesId::DUNSPARCE, 10, vec![MoveId::POISON_STING]);
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
