//! The enemy-ability interactions issue #293's sight-trainer parties make
//! reachable (issue #293 review, round 6): Clear Body on Andrew's and
//! Pete's Tentacool, Soundproof on Marcos's Voltorb, wired through the
//! whole turn engine. Thick Fat's damage term is pinned at the unit level
//! (`crate::damage`'s tests) because no executable move is Fire- or
//! Ice-typed yet.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::ability::CLEAR_BODY;
use battle::{Battle, BattleEvent, ChangedStat, Dex, PlayerAction};

const TACKLE: MoveId = MoveId(33);
const GROWL: MoveId = MoveId(45);
const CONSTRICT: MoveId = MoveId(132);
const RATTATA: u16 = 19;
const TENTACOOL: u16 = 72;
const VOLTORB: u16 = 100;

/// `ChangeStatBuffs`' Clear Body guard
/// (`src/battle_script_commands.c:6987`-`:7008`): the player's Growl
/// connects -- the accuracy draw is spent -- but the stage does not move
/// and the block is reported, not the fall.
#[test]
fn clear_body_blocks_a_stat_lowering_move_after_the_accuracy_draw() {
    let dex = Dex::new();
    // `max_iv_mon` builds at personality 0: even, so the two-ability
    // Tentacool resolves to slot 0, Clear Body -- the same slot every
    // seeded sight-trainer mon gets from `CreateNPCTrainerParty`'s
    // even personalities (`src/battle_main.c:1993`-`:1998`).
    let player = max_iv_mon(&dex, RATTATA, 15, vec![GROWL]);
    let enemy = max_iv_mon(&dex, TENTACOOL, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0u16; 64]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let start = rng.draws();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::StatLossPrevented {
            by_player: true,
            move_id: GROWL,
            stat: ChangedStat::Attack,
            ability: CLEAR_BODY,
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::StatFell { .. })),
        "{events:?}"
    );
    assert_eq!(
        battle.enemy().stages().attack,
        battle::StatStage::NEUTRAL,
        "the stage must not move"
    );
    assert_eq!(
        rng.draws() - start,
        7,
        "turn number + enemy selection + Growl's accuracy draw (the block \
         costs nothing further) + the enemy's 4-draw Tackle"
    );
}

/// The silent `flags == 0` half (`SetMoveEffect`'s stat-drop group,
/// `:2672`-`:2674`): Constrict's landed secondary is blocked by Clear Body
/// with no event at all, and the draw count is the ordinary landed hit's 4.
#[test]
fn clear_body_blocks_a_secondary_stat_drop_silently() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, TENTACOOL, 15, vec![CONSTRICT]);
    let enemy = max_iv_mon(&dex, TENTACOOL, 10, vec![TACKLE]);
    let mut rng = SequenceRng::new([0u16; 64]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let start = rng.draws();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                ..
            }
        )),
        "the damage half lands: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            BattleEvent::StatFell { .. } | BattleEvent::StatLossPrevented { .. }
        )),
        "the blocked secondary is silent: {events:?}"
    );
    assert_eq!(battle.enemy().stages().speed, battle::StatStage::NEUTRAL);
    assert_eq!(
        rng.draws() - start,
        10,
        "turn number + enemy selection + Constrict's ordinary 4 (accuracy, \
         crit, damage, effect chance -- the roll is spent, the block eats \
         its result) + the enemy's 4-draw Tackle"
    );
}

/// `ABILITYEFFECT_MOVES_BLOCK` (`src/battle_util.c:2659`-`:2675`): a sound
/// move into Soundproof is cancelled after the cancellers, spending PP
/// (`BattleScript_SoundproofProtected`'s `ppreduce`,
/// `data/battle_scripts_1.s:4158`-`:4164`) and zero draws.
#[test]
fn soundproof_cancels_a_sound_move_spending_pp_and_no_draws() {
    let dex = Dex::new();
    let base_pp = dex.move_data(GROWL).unwrap().pp;
    let player = max_iv_mon(&dex, RATTATA, 15, vec![GROWL]);
    let enemy = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    let mut rng = SequenceRng::new([0u16; 64]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let start = rng.draws();

    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.contains(&BattleEvent::BlockedBySoundproof {
            by_player: true,
            move_id: GROWL,
        }),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            BattleEvent::StatFell { .. } | BattleEvent::StatLossPrevented { .. }
        )),
        "{events:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        base_pp - 1,
        "unlike a canceller cancellation, the Soundproof block spends PP"
    );
    assert_eq!(
        rng.draws() - start,
        6,
        "turn number + enemy selection + the enemy's 4-draw Tackle -- the \
         blocked Growl draws nothing, not even its accuracy"
    );
}
