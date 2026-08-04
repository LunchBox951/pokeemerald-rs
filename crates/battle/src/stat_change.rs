//! Stat-lowering status moves (issue #199, `S-6`): the shared
//! `BattleScript_EffectStatDown` tail three move effects fall into.
//!
//! `EFFECT_ATTACK_DOWN` (Growl), `EFFECT_DEFENSE_DOWN` (Tail Whip, Leer),
//! and `EFFECT_SPEED_DOWN` (String Shot) each get a one-line battle-script
//! thunk that fixes the stage delta at `-1` and the affected stat, then
//! falls into the shared tail (`pokeemerald/data/battle_scripts_1.s`):
//!
//! ```text
//! BattleScript_EffectAttackDown::                  @ :516
//!     setstatchanger STAT_ATK, 1, TRUE
//!     goto BattleScript_EffectStatDown
//!
//! BattleScript_EffectDefenseDown::                 @ :520
//!     setstatchanger STAT_DEF, 1, TRUE
//!     goto BattleScript_EffectStatDown
//!
//! BattleScript_EffectSpeedDown::                   @ :524
//!     setstatchanger STAT_SPEED, 1, TRUE
//!     goto BattleScript_EffectStatDown
//!
//! BattleScript_EffectStatDown::                    @ :534
//!     attackcanceler
//!     jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE, BattleScript_FailedFromAtkString
//!     accuracycheck BattleScript_PrintMoveMissed, ACC_CURR_MOVE
//!     attackstring
//!     ppreduce
//!     statbuffchange STAT_CHANGE_ALLOW_PTR, BattleScript_StatDownEnd
//!     jumpifbyte CMP_LESS_THAN, cMULTISTRING_CHOOSER, B_MSG_STAT_WONT_DECREASE, BattleScript_StatDownDoAnim
//!     jumpifbyte CMP_EQUAL, cMULTISTRING_CHOOSER, B_MSG_STAT_FELL_EMPTY, BattleScript_StatDownEnd
//!     pause B_WAIT_TIME_SHORT
//!     goto BattleScript_StatDownPrintString
//! BattleScript_StatDownDoAnim::                    @ :545
//!     ...
//! BattleScript_StatDownEnd::                       @ :553
//!     goto BattleScript_MoveEnd
//! ```
//!
//! `setstatchanger STAT_*, 1, TRUE` (`asm/macros/battle_script.inc:1366`)
//! writes `gBattleScripting.statChanger = STAT_* | (1 << 4) | (TRUE << 7)`;
//! the `TRUE` third argument sets `STAT_BUFF_NEGATIVE` (`0x80`,
//! `include/battle.h:481`), so every move here is a `-1`-stage move, never a
//! raise — there is no caller-supplied magnitude to model.
//!
//! # What this module deliberately does not model
//!
//! `jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE` and `ChangeStatBuffs`'s
//! Mist-timer/ability-immunity branches (Clear Body, White Smoke, Keen Eye,
//! Hyper Cutter, Shield Dust — `pokeemerald/src/battle_script_commands.c:6960`-
//! `:7038`) are real upstream branches this module skips outright rather than
//! reproducing as dead code: this slice tracks no Substitute status, no Mist
//! side-timer, and no abilities anywhere (see the crate root docs), so every
//! one of those guards is always false for a battle this crate can construct
//! — there is no reachable state that would take them. If abilities or Mist
//! are ever modelled, [`resolve_stat_lowering_move`] must gain the
//! corresponding branch *and* the RNG draw table below must be re-derived
//! (none of those branches draw, but the-caller-visible outcome would need to
//! change from "stage decreases" to "blocked").
//!
//! # RNG draws
//!
//! Exactly **one** `Random()` call on every path, win, floor, or miss alike —
//! the accuracy roll ([`crate::accuracy::accuracy_check`]; none of these three
//! effects are `EFFECT_ALWAYS_HIT`/`EFFECT_VITAL_THROW`, so the roll always
//! runs). Nothing downstream of it draws:
//!
//! - `Cmd_attackcanceler`/`attackstring`/`Cmd_ppreduce` draw nothing (already
//!   true generically — [`crate::battle::Battle::act`] handles the no-PP
//!   abort and PP deduction for every move effect, this one included).
//! - `ChangeStatBuffs` (`battle_script_commands.c:6937`-`:7098`) has no
//!   `Random()` call anywhere in its body — read start to end, it is pure
//!   bookkeeping (message-buffer prep, the stage add-then-clamp, and the
//!   `MOVE_RESULT_MISSED` flag toggle for the caller-supplied `BS_ptr` jump
//!   path, none of which this slice's callers use).
//! - There is no `Cmd_seteffectwithchance` step in this script at all (unlike
//!   [`crate::hit`]'s `BattleScript_EffectHit`), so there is no analogue of
//!   that pipeline's discarded step-7 draw here.
//!
//! So a caller scripting an RNG sequence budgets exactly one draw per
//! stat-lowering move, regardless of whether it hits, lowers the stage, or
//! finds the target already at [`StatStage::MIN`].

use assets::{MoveEffect, MoveId};

use crate::accuracy::accuracy_check;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_stage::StatStage;

/// `EFFECT_ATTACK_DOWN` (`pokeemerald/include/constants/battle_move_effects.h:22`):
/// Growl's effect id.
pub const EFFECT_ATTACK_DOWN: MoveEffect = MoveEffect(18);

/// `EFFECT_DEFENSE_DOWN` (`:23`): Tail Whip's/Leer's effect id.
pub const EFFECT_DEFENSE_DOWN: MoveEffect = MoveEffect(19);

/// `EFFECT_SPEED_DOWN` (`:24`): String Shot's effect id.
pub const EFFECT_SPEED_DOWN: MoveEffect = MoveEffect(20);

/// The `EFFECT_*` ids this module executes, in `battle_move_effects.h` id
/// order.
const STAT_LOWERING_EFFECTS: [MoveEffect; 3] =
    [EFFECT_ATTACK_DOWN, EFFECT_DEFENSE_DOWN, EFFECT_SPEED_DOWN];

/// Whether `effect`'s battle script is the `BattleScript_EffectStatDown`
/// family this module reproduces.
#[must_use]
pub fn is_stat_lowering_effect(effect: MoveEffect) -> bool {
    STAT_LOWERING_EFFECTS.contains(&effect)
}

/// The one of a [`BattlePokemon`]'s five nature-modifiable stats that an
/// `EFFECT_ATTACK_DOWN`/`EFFECT_DEFENSE_DOWN`/`EFFECT_SPEED_DOWN` move can
/// lower.
///
/// A dedicated three-variant type rather than reusing [`crate::nature::Stat`]
/// (which also carries `SpAttack`/`SpDefense`, neither of which any move in
/// [`STAT_LOWERING_EFFECTS`] can ever select): every variant here is
/// reachable, so callers matching on it never need an `unreachable!` arm for
/// the two stats these three effects cannot touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoweredStat {
    /// `STAT_ATK` (Growl).
    Attack,
    /// `STAT_DEF` (Tail Whip, Leer).
    Defense,
    /// `STAT_SPEED` (String Shot).
    Speed,
}

/// The stat an effect id lowers, or `None` if it is not one of
/// [`STAT_LOWERING_EFFECTS`].
fn stat_for_effect(effect: MoveEffect) -> Option<LoweredStat> {
    if effect == EFFECT_ATTACK_DOWN {
        Some(LoweredStat::Attack)
    } else if effect == EFFECT_DEFENSE_DOWN {
        Some(LoweredStat::Defense)
    } else if effect == EFFECT_SPEED_DOWN {
        Some(LoweredStat::Speed)
    } else {
        None
    }
}

/// `mon`'s current stage for `stat`.
fn stage_of(mon: &BattlePokemon, stat: LoweredStat) -> StatStage {
    let stages = mon.stages();
    match stat {
        LoweredStat::Attack => stages.attack,
        LoweredStat::Defense => stages.defense,
        LoweredStat::Speed => stages.speed,
    }
}

/// The result of resolving one stat-lowering move against one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatChangeOutcome {
    /// The accuracy check failed (`MOVE_RESULT_MISSED`).
    Miss,
    /// The move connected. `new_stage` is the target's post-move stage for
    /// `stat` — identical to its pre-move stage when `floored` is `true`.
    Applied {
        /// Which stat was targeted.
        stat: LoweredStat,
        /// The stage after this move (clamped to [`StatStage::MIN`]).
        new_stage: StatStage,
        /// Whether the target was already at [`StatStage::MIN`] before this
        /// move, so the stage did not actually move — upstream's distinct
        /// `B_MSG_STAT_WONT_DECREASE` ("won't go any lower!") message
        /// (`gStatDownStringIds[B_MSG_STAT_WONT_DECREASE]`,
        /// `src/battle_message.c:1020`) rather than
        /// `B_MSG_DEFENDER_STAT_FELL`.
        floored: bool,
    },
}

/// Whether [`resolve_stat_lowering_move`] can resolve `move_id` at all —
/// checked *before* any state or RNG is touched, mirroring
/// [`crate::hit::ensure_resolvable`]'s contract so
/// [`crate::battle::Battle::new`]/`validate_player_move` can screen a move
/// without leaving a half-mutated turn behind.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if the move's `EFFECT_*` is not
///   one of [`is_stat_lowering_effect`]'s three ids.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_stat_lowering_effect(mv.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    Ok(())
}

/// Resolve `attacker` using `move_id` (one of Growl/Tail Whip/Leer/String
/// Shot's effects) against `defender`.
///
/// Draws from `rng` exactly once — the accuracy roll — regardless of which
/// [`StatChangeOutcome`] comes back (see the module docs' RNG-draws
/// section).
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports for `move_id` — the same check,
/// run here first so this function is safe to call directly. It draws
/// nothing before that check, so a rejected move never disturbs `rng`.
pub fn resolve_stat_lowering_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<StatChangeOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let mv = dex.move_data(move_id)?;

    if !accuracy_check(
        mv.accuracy,
        mv.effect,
        attacker.stages().accuracy,
        defender.stages().evasion,
        rng,
    ) {
        return Ok(StatChangeOutcome::Miss);
    }

    // `ensure_resolvable` already confirmed `mv.effect` is one of the three
    // ids `stat_for_effect` recognises, but this is re-derived through the
    // fallible path rather than an infallible `.expect()` on that guarantee
    // -- one fewer way for a future edit to one check without the other to
    // turn into a panic instead of a clean error.
    let Some(stat) = stat_for_effect(mv.effect) else {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    };
    let current = stage_of(defender, stat);
    if current == StatStage::MIN {
        return Ok(StatChangeOutcome::Applied {
            stat,
            new_stage: current,
            floored: true,
        });
    }
    Ok(StatChangeOutcome::Applied {
        stat,
        new_stage: current.saturating_add(-1),
        floored: false,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_resolvable, is_stat_lowering_effect, resolve_stat_lowering_move, LoweredStat,
        StatChangeOutcome, EFFECT_ATTACK_DOWN, EFFECT_DEFENSE_DOWN, EFFECT_SPEED_DOWN,
    };
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs};
    use crate::stat_stage::StatStage;
    use assets::{MoveEffect, MoveId, SpeciesId};

    /// A `BattleRng` fed from a fixed sequence, for pinning exact draw
    /// order/count.
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

    const MAX_IVS: Ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };

    fn mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves).unwrap()
    }

    #[test]
    fn is_stat_lowering_effect_covers_exactly_the_three_ids() {
        assert!(is_stat_lowering_effect(EFFECT_ATTACK_DOWN));
        assert!(is_stat_lowering_effect(EFFECT_DEFENSE_DOWN));
        assert!(is_stat_lowering_effect(EFFECT_SPEED_DOWN));
        assert!(!is_stat_lowering_effect(MoveEffect(0))); // EFFECT_HIT
        assert!(!is_stat_lowering_effect(MoveEffect(23))); // EFFECT_ACCURACY_DOWN
    }

    #[test]
    fn ensure_resolvable_accepts_growl_leer_and_string_shot() {
        let dex = Dex::new();
        assert_eq!(ensure_resolvable(&dex, MoveId(45)), Ok(())); // Growl
        assert_eq!(ensure_resolvable(&dex, MoveId(43)), Ok(())); // Leer
        assert_eq!(ensure_resolvable(&dex, MoveId(39)), Ok(())); // Tail Whip
        assert_eq!(ensure_resolvable(&dex, MoveId(81)), Ok(())); // String Shot
    }

    #[test]
    fn ensure_resolvable_rejects_unrelated_moves_without_drawing() {
        let dex = Dex::new();
        assert_eq!(
            ensure_resolvable(&dex, MoveId(33)), // Tackle: EFFECT_HIT
            Err(BattleError::UnsupportedMoveEffect(MoveId(33)))
        );
        assert_eq!(
            ensure_resolvable(&dex, MoveId(28)), // Sand Attack: EFFECT_ACCURACY_DOWN
            Err(BattleError::UnsupportedMoveEffect(MoveId(28)))
        );
        let bad = MoveId(60_000);
        assert_eq!(
            ensure_resolvable(&dex, bad),
            Err(BattleError::UnknownMove(bad))
        );
    }

    #[test]
    fn a_hit_growl_lowers_attack_by_one_and_draws_once() {
        let dex = Dex::new();
        let attacker = mon(&dex, 288, 3, vec![MoveId(45)]); // Zigzagoon/Growl
        let defender = mon(&dex, 290, 3, vec![MoveId(33)]); // Wurmple
                                                            // Growl is 100 accuracy: calc = 100, so roll = draw%100+1 always
                                                            // hits (1..=100 <= 100). Draw 0 for a representative value.
        let mut rng = SequenceRng::new([0]);
        let outcome =
            resolve_stat_lowering_move(&dex, MoveId(45), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            StatChangeOutcome::Applied {
                stat: LoweredStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
                floored: false,
            }
        );
        assert_eq!(rng.draws(), 1);
    }

    #[test]
    fn a_missed_string_shot_reports_miss_and_draws_once() {
        let dex = Dex::new();
        let attacker = mon(&dex, 290, 3, vec![MoveId(81)]); // Wurmple/String Shot
        let defender = mon(&dex, 288, 3, vec![MoveId(33)]); // Zigzagoon
                                                            // String Shot is 95 accuracy: roll = draw%100+1. draw=95 -> roll=96
                                                            // > 95 -> miss (the same accuracy arithmetic crate::hit's tests pin
                                                            // for Tackle).
        let mut rng = SequenceRng::new([95]);
        let outcome =
            resolve_stat_lowering_move(&dex, MoveId(81), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(outcome, StatChangeOutcome::Miss);
        assert_eq!(rng.draws(), 1);
    }

    #[test]
    fn a_hit_against_a_floored_stat_reports_floored_and_leaves_the_stage_unchanged() {
        let dex = Dex::new();
        let attacker = mon(&dex, 288, 3, vec![MoveId(45)]); // Growl
        let mut defender = mon(&dex, 290, 3, vec![MoveId(33)]);
        defender.stages_mut().attack = StatStage::MIN;

        let mut rng = SequenceRng::new([0]);
        let outcome =
            resolve_stat_lowering_move(&dex, MoveId(45), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            outcome,
            StatChangeOutcome::Applied {
                stat: LoweredStat::Attack,
                new_stage: StatStage::MIN,
                floored: true,
            }
        );
        assert_eq!(
            rng.draws(),
            1,
            "the floor case still costs the accuracy roll"
        );
    }

    #[test]
    fn string_shot_lowers_speed_and_leer_lowers_defense() {
        let dex = Dex::new();
        let attacker = mon(&dex, 290, 3, vec![MoveId(81), MoveId(43)]);
        let defender = mon(&dex, 288, 3, vec![MoveId(33)]);

        let mut rng = SequenceRng::new([0]);
        let speed_outcome =
            resolve_stat_lowering_move(&dex, MoveId(81), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            speed_outcome,
            StatChangeOutcome::Applied {
                stat: LoweredStat::Speed,
                new_stage: StatStage::new(-1).unwrap(),
                floored: false,
            }
        );

        let mut rng = SequenceRng::new([0]);
        let defense_outcome =
            resolve_stat_lowering_move(&dex, MoveId(43), &attacker, &defender, &mut rng).unwrap();
        assert_eq!(
            defense_outcome,
            StatChangeOutcome::Applied {
                stat: LoweredStat::Defense,
                new_stage: StatStage::new(-1).unwrap(),
                floored: false,
            }
        );
    }

    #[test]
    fn a_rejected_move_draws_nothing() {
        let dex = Dex::new();
        let attacker = mon(&dex, 288, 3, vec![MoveId(33)]);
        let defender = mon(&dex, 290, 3, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            resolve_stat_lowering_move(&dex, MoveId(33), &attacker, &defender, &mut rng),
            Err(BattleError::UnsupportedMoveEffect(MoveId(33)))
        );
    }
}
