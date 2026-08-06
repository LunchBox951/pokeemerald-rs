//! Stat-stage move effects and their downstream battle behavior.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, LoweredStat, PlayerAction, StatStage};

#[test]
fn wild_zigzagoon_growl_executes_when_the_rejection_loop_lands_on_it() {
    let dex = Dex::new();
    // A wild Zigzagoon's real level-1 learnset is Tackle + Growl
    // (`sZigzagoonLevelUpLearnset`, `src/data/pokemon/level_up_learnsets.h:3765`-
    // `:3767`), so a Route 101 L2-3 Zigzagoon (`src/data/wild_encounters.json`)
    // knows both -- construction must accept the whole moveset, and the
    // rejection loop can land on either slot.
    //
    // Stats (max IV, Hardy/neutral), hand computed from CALC_STAT:
    //   Rattata L5: atk 12, def 10, speed 13 (established elsewhere).
    //   Zigzagoon L3 (base atk 30/def 41/spd 60/hp 38):
    //     def  = (2*41+31)*3/100+5 = 339/100=3, +5 = 8.
    //     speed= (2*60+31)*3/100+5 = 453/100=4, +5 = 9.
    //     hp   = (2*38+31)*3/100+3+10 = 321/100=3, +13 = 16.
    // Rattata (13) is faster than Zigzagoon (9): Rattata's Tackle
    // resolves first every turn.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 288, 3, vec![MoveId(33), MoveId(45)]); // Zigzagoon: Tackle, Growl

    // battle start (no tie, speeds differ), turn number, the rejection
    // loop landing on slot 1 (Growl: draw 1 % 4 == 1), the player's
    // 4-draw Tackle, then Growl's 1-draw accuracy check (100 accuracy:
    // cannot miss).
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1);
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Rattata's Tackle: 12*35=420, *4=1680, /8=210, /50=4, +2=6, STAB
    // (Normal on Normal) *15/10 = 9.
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::StatFell {
                by_player: false,
                move_id: MoveId(45),
                stat: LoweredStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
            },
        ]
    );
    assert_eq!(battle.player().stages().attack, StatStage::new(-1).unwrap());
    assert_eq!(battle.enemy().current_hp(), 16 - 9);
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (rejection loop) + 4 (Tackle) + 1 (Growl)"
    );
}

#[test]
fn wild_wurmple_string_shot_misses_when_the_rejection_loop_lands_on_it() {
    let dex = Dex::new();
    // A wild Wurmple's real level-1 learnset is Tackle + String Shot
    // (`sWurmpleLevelUpLearnset`, `src/data/pokemon/level_up_learnsets.h:3799`-
    // `:3801`), the other Route 101 stat-lowering shape.
    //
    // Stats (max IV, neutral), hand computed:
    //   Wurmple L3 (base atk 45/def 35/spd 20/hp 45):
    //     def  = (2*35+31)*3/100+5 = 303/100=3, +5 = 8.
    //     speed= (2*20+31)*3/100+5 = 213/100=2, +5 = 7.
    //     hp   = (2*45+31)*3/100+3+10 = 363/100=3, +13 = 16.
    // Rattata L5 (speed 13) is faster than Wurmple L3 (speed 7).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 290, 3, vec![MoveId(33), MoveId(81)]); // Wurmple: Tackle, String Shot

    // battle start, turn number, the rejection loop landing on slot 1
    // (String Shot: draw 1 % 4 == 1), the player's 4-draw Tackle, then
    // String Shot's accuracy roll: draw 95 -> roll 96 > 95 (95
    // accuracy) -> miss, the same arithmetic crate::hit's Tackle-miss
    // test pins.
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 95]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Rattata's Tackle into Wurmple (def 8): identical arithmetic to the
    // Zigzagoon case above (same attacker, same defense stat) -> 9.
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::Missed {
                by_player: false,
                move_id: MoveId(81),
            },
        ]
    );
    assert_eq!(
        battle.player().stages().speed,
        StatStage::NEUTRAL,
        "a miss must not change any stage"
    );
    assert_eq!(battle.enemy().current_hp(), 16 - 9);
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (rejection loop) + 4 (Tackle) + 1 (String Shot)"
    );
}

#[test]
fn a_stat_already_at_the_floor_reports_wont_go_lower_and_stays_put() {
    let dex = Dex::new();
    // Rattata L5 (speed 13, faster) uses Growl against a Bulbasaur L5
    // (speed 11) whose Attack stage is *already* at MIN_STAT_STAGE --
    // upstream's `gBattleMons[].statStages[statId] == MIN_STAT_STAGE`
    // check inside `ChangeStatBuffs` (`battle_script_commands.c:7056`),
    // reached even though the move still "connects" (the accuracy
    // check has nothing to do with the stage floor).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(45)]); // Rattata/Growl
    let mut enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
    enemy.stages_mut().attack = StatStage::MIN;

    // battle start, turn number, enemy pick (its only move), Growl's
    // 1-draw accuracy check (100 accuracy: cannot miss), then the
    // enemy's ordinary 4-draw Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Bulbasaur's Tackle into Rattata (def 10) is *also* affected here,
    // because it is the very same mon/stat Growl just found already
    // floored: stage-adjusted attack = 11*10/40 = 2 (110/40 truncated,
    // gStatStageRatios' MIN_STAT_STAGE ratio); 2*35=70, *4=280,
    // /10=28, /50=0 (truncated), floored to 1 (physical moves always
    // deal at least 1), +2 = 3 -- far below the neutral-Attack pin of
    // 5 (`full_wild_battle_runs_to_a_faint_and_reports_victory`).
    assert_eq!(
        events,
        vec![
            BattleEvent::StatWontGoLower {
                by_player: true,
                move_id: MoveId(45),
                stat: LoweredStat::Attack,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        battle.enemy().stages().attack,
        StatStage::MIN,
        "already at the floor: the stage must not move"
    );
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (pick) + 1 (Growl) + 4 (Tackle)"
    );
}

#[test]
fn growl_lowers_the_players_subsequent_tackle_damage() {
    let dex = Dex::new();
    // A faster wild Rattata (speed 13) Growls the player's Bulbasaur
    // (speed 11) before Bulbasaur's own Tackle resolves *in the same
    // turn* -- stat stages take effect immediately, so the damage
    // formula reads the already-lowered Attack stage.
    //
    // Baseline (neutral Attack, hand computed elsewhere in this file):
    // Bulbasaur's Tackle into Rattata (def 10) deals 5. With Attack at
    // -1 (gStatStageRatios (10, 15)): stage-adjusted attack =
    // 11*10/15 = 7 (110/15 truncated); 7*35=245, *4=980, /10=98, /50=1
    // (98/50 truncated), +2 = 3 -- strictly less than the neutral 5.
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(45)]); // Rattata/Growl
    let player = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    // battle start, turn number, enemy pick (its only move), Growl's
    // 1-draw accuracy check (100 accuracy: cannot miss), then the
    // player's ordinary 4-draw Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::StatFell {
                by_player: false,
                move_id: MoveId(45),
                stat: LoweredStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
            },
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ],
        "the player's Tackle must reflect the -1 Attack stage Growl \
         just applied, not the neutral baseline of 5"
    );
    assert_eq!(battle.player().stages().attack, StatStage::new(-1).unwrap());
}

#[test]
fn string_shot_flips_turn_order_once_the_targets_effective_speed_drops_below_the_threshold() {
    let dex = Dex::new();
    // Poochyena L5 (speed 10) is faster than Wurmple L5 (speed 8), so
    // Poochyena moves first in turn 1. Wurmple's String Shot that same
    // turn lowers Poochyena's Speed stage to -1: gStatStageRatios'
    // (10, 15) ratio makes its effective Speed 10*10/15 = 6 (integer
    // division) from turn 2 on -- which drops *below* Wurmple's
    // untouched 8, flipping who moves first despite Wurmple being the
    // "slower" mon by raw stats.
    //
    // Stats (max IV, neutral), hand computed:
    //   Poochyena L5 (base atk 55/def 35/spd 35/hp 35): atk
    //     (2*55+31)*5/100+5=705/100=7,+5=12; def (2*35+31)*5/100+5=
    //     505/100=5,+5=10; speed same formula = 10; hp
    //     (2*35+31)*5/100+5+10=505/100=5,+15=20.
    //   Wurmple L5 (base atk 45/def 35/spd 20/hp 45): atk
    //     (2*45+31)*5/100+5=605/100=6,+5=11; def = 10 (same numbers as
    //     Poochyena's); speed (2*20+31)*5/100+5=355/100=3,+5=8; hp
    //     (2*45+31)*5/100+5+10=605/100=6,+15=21.
    // Neither Tackle gets STAB here (Poochyena is Dark, Wurmple is
    // Bug); Normal is neutral into both.
    let player = max_iv_mon(&dex, 290, 5, vec![MoveId(81), MoveId(33)]); // Wurmple: String Shot, Tackle
    let enemy = max_iv_mon(&dex, 286, 5, vec![MoveId(33)]); // Poochyena: Tackle

    // battle start (no tie, 8 vs 10), then:
    // turn 1: turn number, enemy pick (its only move), Poochyena's
    //   4-draw Tackle (it is still faster this turn), Wurmple's 1-draw
    //   String Shot accuracy check (draw 0 -> roll 1 <= 95 -> hit) --
    //   7 draws in all.
    // turn 2: turn number, enemy pick, Wurmple's 4-draw Tackle (now
    //   first -- the flip), Poochyena's 4-draw Tackle -- 10 draws.
    let mut rng = SequenceRng::new([
        0, // battle start
        0, 0, // turn 1: turn number, enemy pick
        0, 1, 0, 0, // turn 1: Poochyena's Tackle
        0, // turn 1: Wurmple's String Shot (hits)
        0, 0, // turn 2: turn number, enemy pick
        0, 1, 0, 0, // turn 2: Wurmple's Tackle (now first)
        0, 1, 0, 0, // turn 2: Poochyena's Tackle
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "no speed-tie draw: 8 and 10 differ");

    // Turn 1: Poochyena moves first (10 > 8).
    // Poochyena's Tackle into Wurmple (def 10): 12*35=420, *4=1680,
    // /10=168, /50=3, +2=5.
    let turn1 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        turn1,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
            BattleEvent::StatFell {
                by_player: true,
                move_id: MoveId(81),
                stat: LoweredStat::Speed,
                new_stage: StatStage::new(-1).unwrap(),
            },
        ],
        "turn 1: Poochyena (faster) moves first"
    );
    assert_eq!(battle.enemy().stages().speed, StatStage::new(-1).unwrap());
    assert_eq!(rng.draws(), 8);

    // Turn 2: Wurmple's raw 8 now beats Poochyena's debuffed effective
    // 6 -- the order has flipped from turn 1, purely from the stage
    // change String Shot committed.
    // Wurmple's Tackle into Poochyena (def 10): 11*35=385, *4=1540,
    // /10=154, /50=3, +2=5 (same numbers as turn 1's Tackle, different
    // attacker).
    let turn2 = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(
        turn2,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
        ],
        "turn 2: the player now moves FIRST despite being the \
         raw-slower mon -- only explicable by the Speed debuff \
         turn 1 committed"
    );
    assert_eq!(battle.player().current_hp(), 21 - 5 - 5);
    assert_eq!(battle.enemy().current_hp(), 20 - 5);
    assert_eq!(rng.draws(), 18);
}
