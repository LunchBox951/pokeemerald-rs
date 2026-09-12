//! Pressure's extra PP cost against a distinct target
//! (`battle_script_commands.c:1205`-`:1237`).
//!
//! `Cmd_ppreduce` starts `ppToDeduct` at one and increments it once more
//! when the actual target is a *different* battler carrying Pressure,
//! saturating the eventual subtraction at zero rather than underflowing. A
//! self-targeting move (`MOVE_TARGET_USER`) never sees the increment, since
//! its target is always the user itself.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, PlayerAction};

/// `MOVE_SCRATCH`, a single-target Normal-type hit.
const SCRATCH: MoveId = MoveId(10);
/// `MOVE_SWORDS_DANCE` (`EFFECT_ATTACK_UP_2`), `MOVE_TARGET_USER`.
const SWORDS_DANCE: MoveId = MoveId(14);

/// `SPECIES_RATTATA`: base Speed 72, used only by the saturation test below,
/// where turn order does not matter.
const RATTATA: u16 = 19;
/// `SPECIES_DUSCLOPS`: Ghost/Ghost, Pressure in its only ability slot
/// (immune to Scratch's Normal typing, so the hit's PP spend is the only
/// thing worth pinning).
const DUSCLOPS: u16 = 362;
/// `SPECIES_ABSOL`: Dark/Dark, Pressure in its only ability slot too, and
/// faster than Dusclops. Both battlers below therefore carry Pressure, so a
/// broken implementation that checked "the opposing battler" without first
/// exempting a self-targeting move would still see a Pressure holder on the
/// other side and wrongly double Swords Dance's cost -- unlike a fixture
/// where only one side carries the ability, which such a bug would pass by
/// coincidence.
const ABSOL: u16 = 376;

#[test]
fn pressure_doubles_pp_cost_against_a_distinct_target_but_not_for_a_self_target() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, ABSOL, 5, vec![SCRATCH]);
    let enemy = max_iv_mon(&dex, DUSCLOPS, 5, vec![SWORDS_DANCE]);
    assert_eq!(
        player.ability(),
        assets::AbilityId::PRESSURE,
        "fixture sanity: Absol's only ability slot is Pressure"
    );
    assert_eq!(
        enemy.ability(),
        assets::AbilityId::PRESSURE,
        "fixture sanity: Dusclops' only ability slot is Pressure"
    );
    let scratch_pp = player.moves()[0].pp;
    let swords_dance_pp = enemy.moves()[0].pp;

    // Generous zero-filled RNG: every accuracy/crit/damage-variance roll of
    // zero hits and Swords Dance's stat-change pipeline draws nothing, so
    // the exact draw count does not need pinning here -- only the PP
    // ledgers do.
    let mut rng = SequenceRng::new([0u16; 24]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert!(
        events.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: SCRATCH,
        }),
        "fixture sanity: Scratch's Normal typing cannot touch a Ghost: {events:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        scratch_pp - 2,
        "a single-target move against a distinct Pressure holder spends two PP"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        swords_dance_pp - 1,
        "a self-targeting move never receives Pressure's increment, even \
         though the user's own opponent also carries Pressure"
    );
}

/// Two turns against a Pressure holder drain a three-PP slot to zero, not
/// one: unfixed code (a flat one-PP `deduct_pp` per turn) would leave one PP
/// after two turns, while this diff's two-PP-then-saturate cost empties it.
/// The second turn is the one that actually exercises the saturating
/// subtraction, since by then only one PP remains against a two-PP cost.
#[test]
fn two_turns_against_a_pressure_holder_drain_a_three_pp_slot_to_zero_not_one() {
    let dex = Dex::new();
    let mut player = max_iv_mon(&dex, RATTATA, 5, vec![SCRATCH]);
    for _ in 0..player.moves()[0].pp - 3 {
        player.deduct_pp(0).unwrap();
    }
    assert_eq!(player.moves()[0].pp, 3, "fixture sanity: three PP left");
    let enemy = max_iv_mon(&dex, DUSCLOPS, 5, vec![SWORDS_DANCE]);

    let mut rng = SequenceRng::new([0u16; 64]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();

    let first_turn = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("the first two-PP cost leaves one PP, no error");
    assert!(
        first_turn.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: SCRATCH,
        }),
        "{first_turn:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        1,
        "three PP minus a two-PP Pressure cost leaves one, matching both \
         fixed and unfixed code so far"
    );

    let second_turn = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("a two-PP cost against a one-PP slot saturates rather than erroring");
    assert!(
        second_turn.contains(&BattleEvent::NoEffect {
            by_player: true,
            move_id: SCRATCH,
        }),
        "{second_turn:?}"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        0,
        "a two-PP cost against a one-PP slot saturates at zero, matching \
         upstream's `else gBattleMons[...].pp[...] = 0` arm \
         (`battle_script_commands.c:1234`-`:1237`); unfixed code's flat \
         one-PP deduction would instead leave one PP after this second turn"
    );
}
