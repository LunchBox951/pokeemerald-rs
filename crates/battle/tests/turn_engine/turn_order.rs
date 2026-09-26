//! Turn ordering and the RNG draws that determine it.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, PlayerAction};

#[test]
fn battle_start_draws_the_initial_turn_order_tie_on_equal_speeds() {
    // Upstream seeds the initial turn order before turn 1 even runs
    // (`TryDoEventsBeforeFirstTurn`, `battle_main.c:3852`-`:3861`), so an
    // exact Speed tie at construction already costs a draw there
    // (`:4745`-`:4750`), before any turn-body tie draw.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0x1234, 0]);
    let _battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        2,
        "an exact Speed tie costs one extra pre-turn-1 draw"
    );
}

#[test]
fn battle_start_and_every_turn_each_refresh_the_turn_number() {
    let dex = Dex::new();
    // Both battlers survive two full turns, so a real second turn starts
    // and its own turn-number refresh can be asserted on too.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(98)]); // Rattata/Quick Attack
    let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]); // Charmander/Tackle

    // Three distinguishable turn-number draws (battle start, turn 1, turn
    // 2), each followed by the ordinary tail of a turn (no turn-order draw,
    // since priorities differ; the wild mon's move pick; then each side's
    // 4-draw hit) so the sequence stays unambiguous.
    let mut rng = SequenceRng::new([
        0x1111, // battle start
        0x2222, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 1: turn number, then its tail
        0x3333, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 2: turn number, then its tail
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(
        battle.random_turn_number(),
        0x1111,
        "BattleStartClearSetData's draw (battle_main.c:3140)"
    );
    assert_eq!(
        rng.draws(),
        1,
        "Battle::new draws exactly once when speeds differ (no tie draw)"
    );
    let _ = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        battle.random_turn_number(),
        0x2222,
        "the turn's own draw (battle_main.c:3923 / :4013) comes first"
    );
    assert_eq!(
        rng.draws(),
        11,
        "1 (battle start) + 1 (turn number) + 1 (pick) + 4 + 4 (turn 1's hits)"
    );
    let _ = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        battle.random_turn_number(),
        0x3333,
        "every turn redraws the turn number, not just the first"
    );
    assert_eq!(
        rng.draws(),
        21,
        "11 (through turn 1) + 1 (turn number) + 1 (pick) + 4 + 4 (turn 2's hits)"
    );
}

#[test]
fn move_priority_beats_speed_for_either_side() {
    let dex = Dex::new();
    // Leg 1: the slower player's +1-priority Quick Attack moves first
    // against the faster enemy's ordinary Tackle. Priorities differ, so
    // no turn-order draw is made.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(98)]); // slow, +1 priority
    let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]); // fast, priority 0
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(98),
                damage: 6,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
        ],
        "the +1-priority mover acts first despite being slower"
    );
    assert_eq!(rng.draws(), 11, "no turn-order draw when priorities differ");

    // Leg 2, mirrored: the wild mon's rejection loop lands on its own
    // +1-priority slot (draw 1 -> slot 1, Quick Attack) and it moves
    // first despite being far slower than the player.
    let player = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]); // fast, priority 0
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33), MoveId(98)]); // slow
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(98),
                damage: 6,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
        ],
        "the wild mon's +1-priority pick acts first despite being slower"
    );
    assert_eq!(rng.draws(), 11);
    assert_eq!(
        battle.enemy().moves()[1].pp,
        29,
        "Quick Attack (slot 1, base PP 30) was the executed pick"
    );
}

#[test]
fn a_mid_turn_speed_tie_draws_once_between_selection_and_the_first_hit() {
    let dex = Dex::new();
    // A mirror match (Rattata L5 vs Rattata L5, effective Speed 13 on
    // both sides, equal priorities) forces the take_turn tie draw the
    // module docs place after the opponent's selection draw and before
    // the first mover's accuracy draw. The script pins the position:
    // the selection value 1 must land on slot 1 (Scratch) and the tie
    // value 0 must mean "player first" -- were the two consumed in the
    // other order, the odd 1 would flip the tie to the enemy and the 0
    // would pick slot 0 (Tackle), failing both assertions below.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch
    let mut rng = SequenceRng::new([
        0, 0, // Battle::new: battle-start turn number + initial-seeding tie
        0, // the turn's own turn number
        1, // selection: 1 % 4 -> slot 1, Scratch
        0, // the mid-turn tie draw: even -> player (attacker) first
        0, 1, 0, 0, // the player's Tackle
        0, 1, 0, 0, // the enemy's Scratch
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        2,
        "mirror match: Battle::new takes the seeding tie draw"
    );
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 7,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(10),
                damage: 7,
                is_critical: false,
            },
        ],
        "tie draw 0 puts the player first; selection draw 1 picked Scratch"
    );
    assert_eq!(
        rng.draws(),
        13,
        "2 (battle start) + 2 (turn number, pick) + 1 (tie) + 4 + 4"
    );
}

#[test]
fn an_always_hit_move_makes_a_full_turn_cost_ten_draws_not_eleven() {
    let dex = Dex::new();
    // Swift (EFFECT_ALWAYS_HIT) skips `AccuracyCalcHelper`'s roll
    // entirely (`battle_script_commands.c:1089`-`:1094`), so the player's
    // move costs 3 draws where an ordinary move costs 4.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(129)]); // Rattata/Swift
    let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    let mut rng = SequenceRng::new([
        0, // battle start turn number (Rattata is faster: no tie draw)
        0, // the turn's own turn number
        0, // the wild mon's move pick
        1, 0, 0, // the player's Swift: crit, damage roll, effect chance -- no accuracy draw
        0, 1, 0, 0, // the enemy's Tackle: accuracy, crit, damage roll, effect chance
    ]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(129),
                damage: 10,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        rng.draws(),
        10,
        "1 (battle start) + 2 (turn number, pick) + 3 (Swift) + 4 (Tackle)"
    );
}
