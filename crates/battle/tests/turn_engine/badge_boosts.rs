//! Earned-badge stat boosts (S-6): `pokeemerald/src/pokemon.c:3158`-`:3168`
//! (damage stats) and `pokeemerald/src/battle_main.c:4639`-`:4651` (Speed).

use crate::common::{max_iv_mon, SequenceRng};
use assets::trainers::TrainerId;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{
    damage_core, BadgeBoosts, Battle, BattleEvent, BattlePokemon, Dex, HitOutcome, PlayerAction,
    PlayerObedience, Stats, Status1,
};

const ABRA: u16 = SpeciesId::ABRA.0;
const ZIGZAGOON: u16 = SpeciesId::ZIGZAGOON.0;
const MARILL: u16 = SpeciesId::MARILL.0;
const CORSOLA: u16 = SpeciesId::CORSOLA.0;
const MILOTIC: u16 = SpeciesId::MILOTIC.0;

const NO_BADGES: [bool; 4] = [false; 4];
const BADGE_1: [bool; 4] = [true, false, false, false];
const BADGE_3: [bool; 4] = [false, true, false, false];
const BADGE_5: [bool; 4] = [false, false, true, false];
const BADGE_7: [bool; 4] = [false, false, false, true];
const ALL_STAT_BADGES: [bool; 4] = [true; 4];

fn planted(
    dex: &Dex,
    species: u16,
    level: u8,
    moves: Vec<MoveId>,
    attack_defense: (u32, u32),
    special: (u32, u32),
    speed: u32,
) -> BattlePokemon {
    max_iv_mon(dex, species, level, moves).with_cached_stats(
        Stats {
            max_hp: 1000,
            attack: attack_defense.0,
            defense: attack_defense.1,
            speed,
            sp_attack: special.0,
            sp_defense: special.1,
        },
        1000,
    )
}

fn with_badges(mon: BattlePokemon, badges: [bool; 4]) -> BattlePokemon {
    mon.with_stat_badges(BadgeBoosts::from_badges(badges))
}

fn damage(dex: &Dex, move_id: MoveId, attacker: &BattlePokemon, defender: &BattlePokemon) -> u32 {
    let mut rng = SequenceRng::new([0]);
    let outcome = damage_core(dex, move_id, attacker, defender, true, &mut rng).unwrap();
    rng.assert_exhausted();
    let HitOutcome::Hit { damage, .. } = outcome else {
        panic!("expected a landed hit, got {outcome:?}");
    };
    damage
}

/// L50, power 40, 101 in every stat: 19 plain, 21 at Attack 111, 18 at
/// Defense 111.
fn matrix_damage(
    dex: &Dex,
    move_id: MoveId,
    attacker_badges: [bool; 4],
    defender_badges: [bool; 4],
) -> u32 {
    let attacker = planted(dex, ABRA, 50, vec![move_id], (101, 101), (101, 101), 50);
    let defender = planted(
        dex,
        ZIGZAGOON,
        50,
        vec![MoveId::TACKLE],
        (101, 101),
        (101, 101),
        50,
    );
    damage(
        dex,
        move_id,
        &with_badges(attacker, attacker_badges),
        &with_badges(defender, defender_badges),
    )
}

#[test]
fn each_badge_boosts_only_the_stat_it_names() {
    let dex = Dex::new();
    // (move, attacker badges, defender badges, expected damage)
    let cases = [
        (MoveId::POUND, NO_BADGES, NO_BADGES, 19),
        (MoveId::POUND, BADGE_1, NO_BADGES, 21),
        (MoveId::POUND, NO_BADGES, BADGE_5, 18),
        // 111 against 111 cancels back to the plain 19.
        (MoveId::POUND, BADGE_1, BADGE_5, 19),
        // The attacker's Defense badge and the defender's Attack badge are
        // not read by the hit.
        (MoveId::POUND, BADGE_5, BADGE_1, 19),
        // Badge 3 is Speed alone and badge 7 is special alone.
        (MoveId::POUND, BADGE_3, BADGE_3, 19),
        (MoveId::POUND, BADGE_7, BADGE_7, 19),
        (MoveId::WATER_GUN, NO_BADGES, NO_BADGES, 19),
        (MoveId::WATER_GUN, BADGE_7, NO_BADGES, 21),
        (MoveId::WATER_GUN, NO_BADGES, BADGE_7, 18),
        (MoveId::WATER_GUN, BADGE_1, BADGE_5, 19),
        (MoveId::WATER_GUN, BADGE_3, BADGE_3, 19),
        // All four badges on both sides: attacker and defender stat both rise.
        (MoveId::POUND, ALL_STAT_BADGES, ALL_STAT_BADGES, 19),
        (MoveId::WATER_GUN, ALL_STAT_BADGES, ALL_STAT_BADGES, 19),
    ];
    for (move_id, attacker_badges, defender_badges, expected) in cases {
        assert_eq!(
            matrix_damage(&dex, move_id, attacker_badges, defender_badges),
            expected,
            "{move_id:?} attacker {attacker_badges:?} defender {defender_badges:?}"
        );
    }
}

/// Attack 105 -> 115 truncates before the formula: 22 against a control of 20.
#[test]
fn the_badge_factor_truncates_the_stat_before_the_damage_formula() {
    let dex = Dex::new();
    let attacker = planted(
        &dex,
        ABRA,
        50,
        vec![MoveId::POUND],
        (105, 101),
        (101, 101),
        50,
    );
    let defender = planted(
        &dex,
        ZIGZAGOON,
        50,
        vec![MoveId::TACKLE],
        (101, 101),
        (101, 101),
        50,
    );
    assert_eq!(damage(&dex, MoveId::POUND, &attacker, &defender), 20);
    let boosted = with_badges(attacker, BADGE_1);
    assert_eq!(damage(&dex, MoveId::POUND, &boosted, &defender), 22);
}

/// Huge Power first (105 -> 210 -> 231, damage 234); badge first would give
/// 230 and 233. No badge: 213.
#[test]
fn the_attack_badge_applies_after_huge_power() {
    let dex = Dex::new();
    let attacker = planted(
        &dex,
        MARILL,
        100,
        vec![MoveId::MEGA_KICK],
        (105, 100),
        (100, 100),
        50,
    )
    .with_ability_slot(1);
    assert_eq!(attacker.ability(), AbilityId::HUGE_POWER);
    let defender = planted(
        &dex,
        ZIGZAGOON,
        100,
        vec![MoveId::TACKLE],
        (100, 100),
        (100, 100),
        50,
    );
    assert_eq!(damage(&dex, MoveId::MEGA_KICK, &attacker, &defender), 213);
    let boosted = with_badges(attacker, BADGE_1);
    assert_eq!(damage(&dex, MoveId::MEGA_KICK, &boosted, &defender), 234);
}

/// Hustle follows the badge: 11 -> 12 -> 18 gives 5, not 16 -> 17 giving 4.
#[test]
fn the_attack_badge_applies_before_hustle() {
    let dex = Dex::new();
    let attacker = planted(
        &dex,
        CORSOLA,
        50,
        vec![MoveId::POUND],
        (11, 101),
        (101, 101),
        50,
    );
    assert_eq!(attacker.ability(), AbilityId::HUSTLE);
    let defender = planted(
        &dex,
        ZIGZAGOON,
        50,
        vec![MoveId::TACKLE],
        (101, 101),
        (101, 101),
        50,
    );
    assert_eq!(damage(&dex, MoveId::POUND, &attacker, &defender), 4);
    let boosted = with_badges(attacker, BADGE_1);
    assert_eq!(damage(&dex, MoveId::POUND, &boosted, &defender), 5);
}

/// Marvel Scale follows the badge: 11 -> 12 -> 18 gives 100, not 16 -> 17
/// giving 106.
#[test]
fn the_defense_badge_applies_before_marvel_scale() {
    let dex = Dex::new();
    let attacker = planted(
        &dex,
        ABRA,
        50,
        vec![MoveId::POUND],
        (101, 101),
        (101, 101),
        50,
    );
    let mut defender = planted(
        &dex,
        MILOTIC,
        50,
        vec![MoveId::TACKLE],
        (101, 11),
        (101, 101),
        50,
    );
    assert_eq!(defender.ability(), AbilityId::MARVEL_SCALE);
    defender.set_status1(Status1::Burned);
    let boosted = with_badges(defender, BADGE_5);
    assert_eq!(damage(&dex, MoveId::POUND, &attacker, &boosted), 100);
}

/// The badge reads change no stored stat.
#[test]
fn stat_badges_never_touch_the_stat_blocks() {
    let dex = Dex::new();
    let plain = planted(
        &dex,
        ABRA,
        50,
        vec![MoveId::POUND],
        (101, 102),
        (103, 104),
        105,
    );
    let boosted = with_badges(plain.clone(), ALL_STAT_BADGES);
    assert_eq!(boosted.stats(), plain.stats());
    assert_eq!(boosted.cached_stats(), plain.cached_stats());
    assert_eq!(boosted.effective_speed(), plain.effective_speed());
    assert_eq!(
        boosted.stat_badges(),
        BadgeBoosts::from_badges(ALL_STAT_BADGES)
    );
    assert_eq!(plain.stat_badges(), BadgeBoosts::NONE);
}

fn zigzagoon(dex: &Dex, speed: u32) -> BattlePokemon {
    planted(
        dex,
        ZIGZAGOON,
        5,
        vec![MoveId::TACKLE],
        (50, 50),
        (50, 50),
        speed,
    )
}

/// Whether the player's Tackle was the first hit of the turn.
fn player_hit_first(events: &[BattleEvent]) -> bool {
    events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit { by_player, .. } => Some(*by_player),
            _ => None,
        })
        .expect("a hit lands")
}

fn untied_turn(
    dex: &Dex,
    player: BattlePokemon,
    enemy: BattlePokemon,
) -> (Vec<BattleEvent>, usize) {
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "no opening tie draw");
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (events, rng.draws())
}

/// Speed 100 against 105 loses without the badge and wins with it (110), and
/// neither takes a tie draw.
#[test]
fn the_speed_badge_lets_a_slightly_slower_player_move_first() {
    let dex = Dex::new();
    let (events, draws) = untied_turn(&dex, zigzagoon(&dex, 100), zigzagoon(&dex, 105));
    assert!(!player_hit_first(&events), "{events:?}");
    assert_eq!(draws, 11);

    let boosted = with_badges(zigzagoon(&dex, 100), BADGE_3);
    let (events, draws) = untied_turn(&dex, boosted, zigzagoon(&dex, 105));
    assert!(player_hit_first(&events), "{events:?}");
    assert_eq!(draws, 11);
}

/// A badge that removes an exact tie removes the opening, action and
/// end-turn draws with it.
#[test]
fn a_speed_badge_that_breaks_a_tie_removes_its_draws() {
    let dex = Dex::new();
    let boosted = with_badges(zigzagoon(&dex, 100), BADGE_3);
    let (events, draws) = untied_turn(&dex, boosted, zigzagoon(&dex, 100));
    assert!(player_hit_first(&events), "{events:?}");
    assert_eq!(draws, 11);
}

/// A badge-created tie costs an opening, action and end-turn draw; an even
/// action draw seats the player first.
#[test]
fn a_speed_badge_that_creates_a_tie_adds_a_draw_to_each_comparison() {
    for (action_tie, expected_player_first) in [(0, true), (1, false)] {
        let dex = Dex::new();
        let player = with_badges(zigzagoon(&dex, 100), BADGE_3);
        let mut rng = SequenceRng::new([
            0, 0, // Battle::new: turn number, opening tie
            0, 0,          // turn number, wild pick
            action_tie, // the action-phase tie
            0, 1, 0, 0, // first Tackle
            0, 1, 0, 0, // second Tackle
            1, // the end-turn tie
        ]);
        let mut battle =
            Battle::new(dex.clone(), player, zigzagoon(&dex, 110), false, &mut rng).unwrap();
        assert_eq!(
            rng.draws(),
            2,
            "the badge-created tie costs an opening draw"
        );
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            player_hit_first(&events),
            expected_player_first,
            "{events:?}"
        );
        rng.assert_exhausted();
    }
}

fn hit_and_tick_sides(events: &[BattleEvent]) -> (Vec<bool>, Vec<bool>) {
    let hits = events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::Hit { by_player, .. } => Some(*by_player),
            _ => None,
        })
        .collect();
    let ticks = events
        .iter()
        .filter_map(|event| match event {
            BattleEvent::HurtByPoison { by_player, .. } => Some(*by_player),
            _ => None,
        })
        .collect();
    (hits, ticks)
}

fn poisoned(mut mon: BattlePokemon) -> BattlePokemon {
    mon.set_status1(Status1::Poisoned);
    mon
}

/// The end-turn order is rebuilt from boosted Speed with its own tie draw
/// (`battle_util.c:1199`-`:1210`), so it can disagree with the action order.
#[test]
fn the_residual_order_reads_the_badge_boosted_speed_with_its_own_tie_draw() {
    for (action_tie, end_turn_tie, hits, ticks) in [
        (0, 1, [true, false], [false, true]),
        (1, 0, [false, true], [true, false]),
    ] {
        let dex = Dex::new();
        let player = with_badges(poisoned(zigzagoon(&dex, 100)), BADGE_3);
        let enemy = poisoned(zigzagoon(&dex, 110));
        let mut rng = SequenceRng::new([
            0,
            0, // Battle::new: turn number, opening tie
            0,
            0, // turn number, wild pick
            action_tie,
            0,
            1,
            0,
            0, // first Tackle
            0,
            1,
            0,
            0, // second Tackle
            end_turn_tie,
        ]);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        let (hit_sides, tick_sides) = hit_and_tick_sides(&events);
        assert_eq!(hit_sides, hits, "{events:?}");
        assert_eq!(tick_sides, ticks, "{events:?}");
        rng.assert_exhausted();
    }
}

#[test]
fn a_badge_boosted_player_acts_and_ticks_first_without_any_tie_draw() {
    let dex = Dex::new();
    let player = with_badges(poisoned(zigzagoon(&dex, 100)), BADGE_3);
    let enemy = poisoned(zigzagoon(&dex, 105));
    let (events, draws) = untied_turn(&dex, player, enemy);
    let (hit_sides, tick_sides) = hit_and_tick_sides(&events);
    assert_eq!(hit_sides, [true, false], "{events:?}");
    assert_eq!(tick_sides, [true, false], "{events:?}");
    assert_eq!(draws, 11);
}

/// Whatever the caller stamped on the enemy is cleared at battle start.
#[test]
fn an_enemy_never_receives_stat_badges() {
    let dex = Dex::new();
    let enemy = with_badges(zigzagoon(&dex, 105), ALL_STAT_BADGES);
    let player = zigzagoon(&dex, 100);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(battle.enemy().stat_badges(), BadgeBoosts::NONE);
    assert_eq!(rng.draws(), 1);
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(!player_hit_first(&events), "{events:?}");
}

/// A trainer's party never receives them either.
#[test]
fn a_trainer_party_never_receives_stat_badges() {
    let dex = Dex::new();
    let player = zigzagoon(&dex, 100);
    // The real Route 103 rival (`TRAINER_MAY_ROUTE_103_MUDKIP`) with a
    // Treecko the caller stamped.
    let treecko = with_badges(
        max_iv_mon(
            &dex,
            SpeciesId::TREECKO.0,
            5,
            vec![MoveId::POUND, MoveId::LEER],
        ),
        ALL_STAT_BADGES,
    );
    let mut rng = SequenceRng::new([0]);
    let battle = Battle::new_trainer(dex, player, TrainerId(529), vec![treecko], &mut rng).unwrap();
    assert_eq!(battle.enemy().stat_badges(), BadgeBoosts::NONE);
}

/// Reserves share the lead's side, so a send-out keeps the boost.
#[test]
fn the_players_reserves_share_the_leads_badges() {
    let dex = Dex::new();
    let lead = with_badges(zigzagoon(&dex, 100), BADGE_1);
    let mut rng = SequenceRng::new([0]);
    let battle = Battle::new_with_player_reserves(
        dex.clone(),
        lead,
        vec![zigzagoon(&dex, 100)],
        zigzagoon(&dex, 50),
        false,
        &mut rng,
    )
    .unwrap();
    let held: Vec<_> = battle
        .player_members()
        .map(BattlePokemon::stat_badges)
        .collect();
    assert_eq!(held, vec![BadgeBoosts::from_badges(BADGE_1); 2]);
}

#[test]
fn the_player_obedience_context_carries_the_stat_badges() {
    let dex = Dex::new();
    let obedience = PlayerObedience::new(1, *b"PLAYER\xFF\0", [false; 4])
        .with_stat_badges(BadgeBoosts::from_badges(BADGE_3));
    let mut rng = SequenceRng::new([0]);
    let battle = Battle::new(
        dex.clone(),
        zigzagoon(&dex, 100),
        zigzagoon(&dex, 105),
        false,
        &mut rng,
    )
    .unwrap()
    .with_player_obedience(obedience);
    assert_eq!(
        battle.player().stat_badges(),
        BadgeBoosts::from_badges(BADGE_3)
    );
    assert_eq!(battle.enemy().stat_badges(), BadgeBoosts::NONE);
}

/// The self-hit is attacker and defender at once (`battle_util.c:2169`-`:2175`,
/// `:4020`-`:4027`): 19 plain, 21 with Attack, 18 with Defense, 19 with both.
#[test]
fn a_players_confusion_self_hit_reads_its_attack_and_defense_badges() {
    for (badges, expected) in [
        (NO_BADGES, 19),
        (BADGE_1, 21),
        (BADGE_5, 18),
        ([true, false, true, false], 19),
        (BADGE_3, 19),
        (BADGE_7, 19),
    ] {
        let dex = Dex::new();
        let mut player = with_badges(
            planted(
                &dex,
                ZIGZAGOON,
                50,
                vec![MoveId::TACKLE],
                (101, 101),
                (101, 101),
                200,
            ),
            badges,
        );
        player.volatiles_mut().set_confusion(3);
        let enemy = planted(
            &dex,
            ZIGZAGOON,
            50,
            vec![MoveId::TACKLE],
            (101, 101),
            (101, 101),
            5,
        );
        let mut rng = SequenceRng::new([
            0, // Battle::new: turn number
            0, 0, // turn number, wild pick
            0, // confusion coin: even -> self-hit
            0, // damage roll
            0, 1, 0, 0, // the slower enemy's Tackle
        ]);
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        let self_hit = events
            .iter()
            .find_map(|event| match event {
                BattleEvent::ConfusionSelfHit { by_player, damage } => {
                    assert!(*by_player);
                    Some(*damage)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no self-hit in {events:?}"));
        assert_eq!(self_hit, expected, "badges {badges:?}");
    }
}

/// An enemy's self-hit never reads a badge.
#[test]
fn an_enemys_confusion_self_hit_ignores_stat_badges() {
    let dex = Dex::new();
    let player = with_badges(
        planted(
            &dex,
            ZIGZAGOON,
            50,
            vec![MoveId::TACKLE],
            (101, 101),
            (101, 101),
            200,
        ),
        ALL_STAT_BADGES,
    );
    let mut enemy = with_badges(
        planted(
            &dex,
            ZIGZAGOON,
            50,
            vec![MoveId::TACKLE],
            (101, 101),
            (101, 101),
            5,
        ),
        ALL_STAT_BADGES,
    );
    enemy.volatiles_mut().set_confusion(3);
    let mut rng = SequenceRng::new([
        0, // Battle::new: turn number
        0, 0, // turn number, wild pick
        0, 1, 0, 0, // the faster player's Tackle
        0, // confusion coin
        0, // damage roll
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    let self_hit = events.iter().find_map(|event| match event {
        BattleEvent::ConfusionSelfHit {
            by_player: false,
            damage,
        } => Some(*damage),
        _ => None,
    });
    assert_eq!(self_hit, Some(19), "{events:?}");
}

/// Escape reads raw Speed (`battle_util.c:463`-`:466`): 100 against 105 gives
/// odds 151, so a roll of 155 fails; the boosted 110 would give 164.
#[test]
fn the_speed_badge_does_not_change_the_escape_odds() {
    let dex = Dex::new();
    let player = with_badges(zigzagoon(&dex, 100), BADGE_3);
    let mut rng = SequenceRng::new([0, 0, 0, 155, 0, 1, 0, 0]);
    let mut battle =
        Battle::new(dex.clone(), player, zigzagoon(&dex, 105), false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(
        events.contains(&BattleEvent::RunAttempt {
            by_player: true,
            success: false
        }),
        "{events:?}"
    );
}

/// A context with no stat badges leaves the lead's own badges alone.
#[test]
fn an_obedience_context_without_stat_badges_keeps_the_leads_badges() {
    let dex = Dex::new();
    let lead = with_badges(zigzagoon(&dex, 100), BADGE_3);
    let mut rng = SequenceRng::new([0]);
    let battle = Battle::new(dex.clone(), lead, zigzagoon(&dex, 105), false, &mut rng)
        .unwrap()
        .with_player_obedience(PlayerObedience::new(1, *b"PLAYER\xFF\0", [false; 4]));
    assert_eq!(
        battle.player().stat_badges(),
        BadgeBoosts::from_badges(BADGE_3)
    );
}
