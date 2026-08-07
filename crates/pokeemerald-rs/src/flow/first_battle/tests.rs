//! `crate::flow::first_battle` (issue #221), pinned the same way
//! `crate::flow::wild_encounter`'s tests pin the ordinary encounter: real
//! `engine::rng::Rng`, real species/level/moveset data, exact draw counts
//! for the parts this module adds. The turn-by-turn RNG math itself (crit
//! suppression, the `AI_FirstBattle` opponent, damage/accuracy draws) is
//! already exhaustively pinned by `crates/battle/tests/turn_engine/first_battle.rs`
//! against a scripted stream; these tests are only about the new glue —
//! construction off the real generator, and the driver's action policy.

use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, Ivs, PlayerAction};
use engine::rng::Rng;

use super::{
    advance_first_battle, start_first_battle, FIRST_BATTLE_OPPONENT_LEVEL,
    FIRST_BATTLE_OPPONENT_SPECIES,
};

const TACKLE: MoveId = MoveId(33);
const GROWL: MoveId = MoveId(45);
const POUND: MoveId = MoveId(1);

/// A max-IV, fixed-personality player mon -- deterministic stats, mirroring
/// `wild_encounter/tests.rs`'s own helper, so a scenario's outcome depends
/// on the *enemy's* real rolled stats and not on the player's.
fn player_mon(species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), SpeciesId(species), level, ivs, 0, moves)
        .expect("player mon: species/moves must be in the dex")
}

/// `SetUpBattleVarsAndBirchZigzagoon`'s construction, end to end: a
/// level-2 Zigzagoon, its real `GiveBoxMonInitialMoveset` moveset, and proof
/// that `first_battle = true` really reached `Battle::new` -- Run is
/// rejected outright rather than merely attempted (issue #187).
#[test]
fn start_first_battle_builds_a_level_2_zigzagoon_with_first_battle_set() {
    assert_eq!(FIRST_BATTLE_OPPONENT_SPECIES, SpeciesId(288));
    assert_eq!(FIRST_BATTLE_OPPONENT_LEVEL, 2);

    let mut rng = Rng::new(1);
    let lead = player_mon(277, 50, vec![POUND]);
    let mut battle = start_first_battle(lead, &mut rng).expect("construction must succeed");

    assert_eq!(battle.enemy().species(), SpeciesId(288));
    assert_eq!(battle.enemy().level(), 2);
    assert!(!battle.enemy().is_fainted());
    let known: Vec<MoveId> = battle.enemy().moves().iter().map(|m| m.move_id).collect();
    assert_eq!(
        known,
        vec![TACKLE, GROWL],
        "a level-2 Zigzagoon's real level-up learnset is Tackle + Growl"
    );

    let failure = battle
        .take_turn(PlayerAction::Run, &mut super::SharedRng::new(&mut rng))
        .unwrap_err();
    assert_eq!(
        failure.error(),
        BattleError::RunForbidden,
        "first_battle must have reached Battle::new as true"
    );
}

/// The exact draw count [`start_first_battle`] spends before handing back a
/// battle: [`battle::build_pokemon_with_random_personality`]'s personality
/// (one `Random32`, two `next_u16` draws) and IVs (two more), then
/// [`battle::Battle::new`]'s own turn-number draw. No speed-tie draw for
/// this seed -- asserted explicitly below rather than assumed, so a future
/// dex/stat change that flipped it would fail loudly here instead of
/// silently shifting the pin.
#[test]
fn start_first_battle_draws_personality_then_ivs_then_the_turn_number_off_the_shared_stream() {
    const SEED: u32 = 1;
    let mut rng = Rng::new(SEED);
    let lead = player_mon(277, 50, vec![POUND]);
    let battle = start_first_battle(lead, &mut rng).expect("construction must succeed");

    // Replay the same three primitive draws on an independent reference
    // generator seeded identically -- proving both the exact count *and*
    // that the personality/IV draws land before Battle::new's, upstream's
    // own order (module docs, "RNG stream").
    let mut reference = Rng::new(SEED);
    let personality = reference.next_u32();
    let ivs_first = reference.next_u16();
    let ivs_second = reference.next_u16();
    assert_eq!(battle.enemy().personality(), personality);
    assert_eq!(battle.enemy().ivs().hp, (ivs_first & 0x1F) as u8);
    assert_eq!(
        battle.enemy().ivs().sp_defense,
        ((ivs_second >> 10) & 0x1F) as u8
    );
    // `Battle::new`'s own turn-number draw -- the value itself is never read
    // by anything this slice models (module docs' derivation), only its
    // place in the sequence matters here.
    reference.next_u16();

    assert_ne!(
        battle.player().effective_speed(),
        battle.enemy().effective_speed(),
        "this pin assumes no speed tie; a tie would cost one extra draw"
    );
    assert_eq!(
        rng.state(),
        reference.state(),
        "exactly personality (2) + ivs (2) + turn number (1) = 5 draws, no more"
    );
}

/// The driver's whole point: [`advance_first_battle`] must never attempt
/// [`PlayerAction::Run`] (issue #221's headline requirement -- reusing
/// `wild_encounter::advance_wild_battle`'s policy here would instantly end
/// the battle via [`BattleError::RunForbidden`] on turn one instead of
/// playing it). A level-50 Treecko against a level-2 Zigzagoon must instead
/// reach a real terminal outcome.
#[test]
fn advance_first_battle_plays_to_a_terminal_outcome_without_ever_running() {
    let mut rng = Rng::new(1);
    let lead = player_mon(277, 50, vec![POUND]);
    let battle = start_first_battle(lead, &mut rng).expect("construction must succeed");

    let mut slot = Some(battle);
    let mut written_back: Option<BattlePokemon> = None;
    let mut frames = 0;
    let outcome = loop {
        if let Some(outcome) = advance_first_battle(&mut slot, &mut written_back, &mut rng) {
            break outcome;
        }
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    };

    assert!(
        matches!(outcome, BattleOutcome::PlayerWon | BattleOutcome::WildFled),
        "a level-50 Treecko is never going to lose to a level-2 Zigzagoon: got {outcome:?}"
    );
    assert!(
        slot.is_none(),
        "the driver must empty the slot once it ends"
    );
    let lead = written_back.expect("the driver writes the lead mon back on the frame it ends");
    assert_eq!(lead.species(), SpeciesId(277));
    assert_eq!(
        lead.stages(),
        battle::StatStages::default(),
        "in-battle stat stages must not leak back into the overworld copy"
    );
}

/// The abort contract [`advance_first_battle`]'s doc comment spells out, and
/// the reason it needs spelling out: unlike the Run driver's, this driver's
/// [`PlayerAction::UseMove`] policy spends PP, so a lead that reaches the
/// battle with slot 0 already drained fails `Battle::take_turn`'s *pre-draw*
/// validation with [`BattleError::NoPpRemaining`]`(0)` -- a genuinely
/// reachable error arm, not a defensive one. On that frame the driver empties
/// the slot, writes the lead back (drained PP included), and returns `None`
/// -- indistinguishable from an ongoing turn by the return value alone, which
/// is exactly why a caller looping for `Some(outcome)` must watch `slot` too.
#[test]
fn advance_first_battle_aborts_and_writes_back_when_the_lead_has_no_pp() {
    let mut rng = Rng::new(1);
    let mut lead = player_mon(277, 50, vec![POUND]);
    // Drain slot 0 the way turns would have, rather than reaching in: `Pound`
    // starts at its dex PP, and `deduct_pp` is the same accessor the turn
    // engine spends it through.
    let starting_pp = lead.moves()[0].pp;
    assert!(starting_pp > 0, "a freshly built mon starts with PP");
    for _ in 0..starting_pp {
        lead.deduct_pp(0)
            .expect("draining a slot that still has PP");
    }
    assert_eq!(lead.moves()[0].pp, 0);

    let battle = start_first_battle(lead, &mut rng).expect("construction must succeed");
    let mut slot = Some(battle);
    let mut written_back: Option<BattlePokemon> = None;

    let outcome = advance_first_battle(&mut slot, &mut written_back, &mut rng);

    assert!(
        outcome.is_none(),
        "an aborted turn reports no outcome -- the engine never produced one: {outcome:?}"
    );
    assert!(
        slot.is_none(),
        "the abort arm must empty the slot, or the caller loops forever on a dead battle"
    );
    let lead = written_back.expect("the lead is written back on the abort frame too");
    assert_eq!(lead.species(), SpeciesId(277));
    assert_eq!(
        lead.moves()[0].pp,
        0,
        "the drained PP persists into the overworld copy"
    );
    assert_eq!(lead.stages(), battle::StatStages::default());
}

/// `advance_first_battle` is a no-op on an empty slot -- the same guard
/// `wild_encounter::advance_wild_battle` gives the per-frame caller.
#[test]
fn advancing_an_absent_first_battle_does_nothing() {
    let mut slot: Option<Battle> = None;
    let mut lead = None;
    let mut rng = Rng::new(1);
    assert!(advance_first_battle(&mut slot, &mut lead, &mut rng).is_none());
    assert!(lead.is_none());
    assert_eq!(rng.state(), Rng::new(1).state(), "no battle, no draw");
}
