//! [`crate::primary_status`]'s own unit tests: the pre-accuracy failure
//! branches that cost no draws, the type immunity that belongs to the move
//! rather than to the status, and confusion's extra duration draw.

use super::{
    ensure_resolvable, is_primary_status_effect, resolve_primary_status_move, PrimaryStatusOutcome,
    EFFECT_CONFUSE, EFFECT_PARALYZE,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::status::Status1;
use assets::{MoveId, SpeciesId};

struct SequenceRng {
    values: Vec<u16>,
    index: usize,
}
impl SequenceRng {
    fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }
    fn draws(&self) -> usize {
        self.index
    }
}
impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let v = self
            .values
            .get(self.index)
            .copied()
            .expect("SequenceRng exhausted");
        self.index += 1;
        v
    }
}

const THUNDER_WAVE: MoveId = MoveId(86);
const STUN_SPORE: MoveId = MoveId(78);
const SUPERSONIC: MoveId = MoveId(48);
const TACKLE: MoveId = MoveId(33);

fn mon(dex: &Dex, species: u16) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), 15, Ivs::default(), 0, vec![TACKLE]).unwrap()
}

#[test]
fn the_three_moves_carry_the_two_effect_ids() {
    let dex = Dex::new();
    assert_eq!(dex.move_data(THUNDER_WAVE).unwrap().effect, EFFECT_PARALYZE);
    assert_eq!(dex.move_data(STUN_SPORE).unwrap().effect, EFFECT_PARALYZE);
    assert_eq!(dex.move_data(SUPERSONIC).unwrap().effect, EFFECT_CONFUSE);
    assert!(is_primary_status_effect(EFFECT_PARALYZE));
    assert!(is_primary_status_effect(EFFECT_CONFUSE));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

/// Thunder Wave is 100 accuracy, so it cannot miss -- but the roll still
/// happens, exactly once, and `SetMoveEffect` adds nothing for paralysis.
#[test]
fn a_landed_thunder_wave_costs_exactly_one_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, 353); // Plusle
    let defender = mon(&dex, 183); // Marill
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_primary_status_move(&dex, THUNDER_WAVE, &attacker, &defender, &mut rng).unwrap(),
        PrimaryStatusOutcome::Paralysed
    );
    assert_eq!(
        rng.draws(),
        1,
        "the accuracy roll only -- seteffectprimary draws nothing for paralysis"
    );
}

/// Stun Spore is 75 accuracy and shares the script, so it can genuinely
/// miss -- still one draw.
#[test]
fn a_missed_stun_spore_costs_the_same_single_draw() {
    let dex = Dex::new();
    let attacker = mon(&dex, 306); // Shroomish
    let defender = mon(&dex, 183);
    // 75 accuracy: draw 75 -> roll 76 > 75 -> miss.
    let mut rng = SequenceRng::new([75]);
    assert_eq!(
        resolve_primary_status_move(&dex, STUN_SPORE, &attacker, &defender, &mut rng).unwrap(),
        PrimaryStatusOutcome::Miss
    );
    assert_eq!(rng.draws(), 1);
}

/// The ordering that matters most: `typecalc` and both status checks sit
/// **before** `accuracycheck`, so a Thunder Wave that cannot possibly land
/// costs the shared stream **nothing**. An implementation that rolled
/// accuracy first would be one draw out for the rest of the battle.
#[test]
fn a_thunder_wave_that_cannot_land_never_reaches_the_accuracy_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, 353); // Plusle
    let empty = || SequenceRng::new([]);

    // Electric into a Ground type: `typecalc` -> `jumpifmovehadnoeffect`.
    let ground = mon(&dex, 27); // Sandshrew
    let mut rng = empty();
    assert_eq!(
        resolve_primary_status_move(&dex, THUNDER_WAVE, &attacker, &ground, &mut rng).unwrap(),
        PrimaryStatusOutcome::Failed
    );
    assert_eq!(rng.draws(), 0, "the type immunity is found before the roll");

    // An already-paralysed target: `jumpifstatus BS_TARGET, STATUS1_PARALYSIS`,
    // whose own distinct `BattleScript_AlreadyParalyzed` string is its own
    // distinct outcome (issue #293 review).
    let mut paralysed = mon(&dex, 183);
    paralysed.set_status(Status1::Paralysed);
    let mut rng = empty();
    assert_eq!(
        resolve_primary_status_move(&dex, THUNDER_WAVE, &attacker, &paralysed, &mut rng).unwrap(),
        PrimaryStatusOutcome::AlreadyParalysed
    );
    assert_eq!(rng.draws(), 0);

    // A target carrying some *other* status: `jumpifstatus BS_TARGET,
    // STATUS1_ANY`.
    let mut poisoned = mon(&dex, 183);
    poisoned.set_status(Status1::Poisoned);
    let mut rng = empty();
    assert_eq!(
        resolve_primary_status_move(&dex, THUNDER_WAVE, &attacker, &poisoned, &mut rng).unwrap(),
        PrimaryStatusOutcome::Failed
    );
    assert_eq!(rng.draws(), 0);
}

/// The type immunity belongs to the **move**, not to paralysis: Stun Spore
/// is Grass, so the very same Ground target it stops Thunder Wave with is
/// paralysable by it. (And an Electric type is paralysable by Thunder Wave,
/// which Gen 4 onward would refuse.)
#[test]
fn the_immunity_is_the_moves_type_and_not_the_statuss() {
    let dex = Dex::new();
    let ground = mon(&dex, 27); // Sandshrew
    let sporer = mon(&dex, 306); // Shroomish
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_primary_status_move(&dex, STUN_SPORE, &sporer, &ground, &mut rng).unwrap(),
        PrimaryStatusOutcome::Paralysed,
        "Grass into Ground is neutral, so Stun Spore lands where Thunder Wave cannot"
    );

    let electric = mon(&dex, 100); // Voltorb
    let waver = mon(&dex, 353);
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_primary_status_move(&dex, THUNDER_WAVE, &waver, &electric, &mut rng).unwrap(),
        PrimaryStatusOutcome::Paralysed,
        "Gen 3 has no Electric-type immunity to paralysis"
    );
}

/// Supersonic has **no `typecalc` at all**, so nothing is immune to it --
/// and it costs a second draw the paralysis script does not, for the 2..5
/// turn duration.
#[test]
fn a_landed_supersonic_costs_two_draws_and_confuses_for_two_to_five_turns() {
    let dex = Dex::new();
    let attacker = mon(&dex, 72); // Tentacool
    let defender = mon(&dex, 183);
    // Supersonic is 55 accuracy: draw 0 -> roll 1 <= 55 -> hit.
    // Then the duration: 1 % 4 + 2 = 3.
    let mut rng = SequenceRng::new([0, 1]);
    assert_eq!(
        resolve_primary_status_move(&dex, SUPERSONIC, &attacker, &defender, &mut rng).unwrap(),
        PrimaryStatusOutcome::Confused(3)
    );
    assert_eq!(rng.draws(), 2, "accuracy, then the duration");

    // A Ghost, which Normal moves cannot touch at all, is still confusable.
    let ghost = mon(&dex, 92); // Gastly
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_primary_status_move(&dex, SUPERSONIC, &attacker, &ghost, &mut rng).unwrap(),
        PrimaryStatusOutcome::Confused(2),
        "BattleScript_EffectConfuse has no typecalc, so type immunity never applies"
    );
}

#[test]
fn a_missed_supersonic_never_reaches_the_duration_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, 72);
    let defender = mon(&dex, 183);
    // 55 accuracy: draw 55 -> roll 56 > 55 -> miss. One value only, so a
    // stray duration draw would panic.
    let mut rng = SequenceRng::new([55]);
    assert_eq!(
        resolve_primary_status_move(&dex, SUPERSONIC, &attacker, &defender, &mut rng).unwrap(),
        PrimaryStatusOutcome::Miss
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn an_already_confused_target_fails_supersonic_before_the_accuracy_roll() {
    let dex = Dex::new();
    let attacker = mon(&dex, 72);
    let mut defender = mon(&dex, 183);
    defender.volatiles_mut().confusion_turns = 2;
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_primary_status_move(&dex, SUPERSONIC, &attacker, &defender, &mut rng).unwrap(),
        PrimaryStatusOutcome::AlreadyConfused,
        "`BattleScript_AlreadyConfused` is its own distinct string, so its \
         own distinct outcome (issue #293 review)"
    );
    assert_eq!(rng.draws(), 0);
}

/// Confusion is `status2` and paralysis is `status1`, so they stack: a
/// paralysed target is still confusable, unlike a *poisoned* one being
/// paralysed.
#[test]
fn confusion_and_paralysis_are_different_stores_and_stack() {
    let dex = Dex::new();
    let attacker = mon(&dex, 72);
    let mut defender = mon(&dex, 183);
    defender.set_status(Status1::Paralysed);
    let mut rng = SequenceRng::new([0, 0]);
    assert_eq!(
        resolve_primary_status_move(&dex, SUPERSONIC, &attacker, &defender, &mut rng).unwrap(),
        PrimaryStatusOutcome::Confused(2)
    );
}

/// `SetMoveEffect`'s zero-HP early-out (`battle_script_commands.c:2261`-
/// `:2264`): a queued status move against a target that fainted earlier in
/// the turn spends its accuracy draw and nothing more -- no status write,
/// no confusion-duration roll (issue #293 review, round 5). The corpse
/// reads as status-free for the pre-accuracy branches because
/// `cleareffectsonfaint` wiped it at the faint.
#[test]
fn a_fainted_target_takes_no_status_after_the_accuracy_draw() {
    let dex = Dex::new();
    let mut corpse = mon(&dex, 183);
    let max_hp = corpse.stats().max_hp;
    corpse.apply_damage(max_hp);

    let waver = mon(&dex, 353);
    // Exactly one value: a duration roll would panic the sequence.
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_primary_status_move(&dex, THUNDER_WAVE, &waver, &corpse, &mut rng).unwrap(),
        PrimaryStatusOutcome::TargetDown
    );
    assert_eq!(rng.draws(), 1, "accuracy only");

    let sonic = mon(&dex, 72);
    let mut rng = SequenceRng::new([0]);
    assert_eq!(
        resolve_primary_status_move(&dex, SUPERSONIC, &sonic, &corpse, &mut rng).unwrap(),
        PrimaryStatusOutcome::TargetDown
    );
    assert_eq!(
        rng.draws(),
        1,
        "accuracy only -- the duration roll is skipped"
    );
}
