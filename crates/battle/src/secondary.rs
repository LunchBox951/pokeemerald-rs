//! The shared post-damage hook (S-6, issue #321): `Cmd_seteffectwithchance`
//! (`pokeemerald/src/battle_script_commands.c:2908`-`:2939`), the one step
//! every damaging battle script in this crate ends with — and the single
//! place its `Random()` is spent.
//!
//! Before this module the draw lived inline in [`crate::hit::resolve_hit`],
//! which was fine while one script needed it. Four more pipelines arrived
//! with issue #321 and they *disagree* about it — [`crate::drain`] never
//! reaches the step at all, [`crate::multi_hit`] takes it once for a whole
//! 2..5-hit sequence, [`crate::fixed_damage`] takes it after a damage
//! calculation that never happened — so the step became a concept of its
//! own `(oop-boundaries)`. Getting the count wrong desynchronises every
//! later roll in the battle, so it is worth one file.
//!
//! # The command, transcribed
//!
//! ```text
//! percentChance = secondaryEffectChance          (x2 for Serene Grace)
//! if (MOVE_EFFECT_BYTE & MOVE_EFFECT_CERTAIN            // :2917 -- NO Random()
//!     && !(gMoveResultFlags & MOVE_RESULT_NO_EFFECT))
//!     SetMoveEffect(...)
//! else if (Random() % 100 < percentChance               // :2923 -- ALWAYS drawn
//!          && gBattleCommunication[MOVE_EFFECT_BYTE]    // :2924
//!          && !(gMoveResultFlags & MOVE_RESULT_NO_EFFECT))  // :2925
//!     SetMoveEffect(...)
//! ```
//!
//! Three consequences this module exists to keep straight:
//!
//! 1. **The draw is the *leading* operand of the `else if`.** It happens
//!    before either of the tests that could suppress it, so a plain
//!    `EFFECT_HIT` move (whose `MOVE_EFFECT_BYTE` is `0` — the plain hit
//!    script never runs `setmoveeffect`) still spends a value and throws it
//!    away, and so does a **type-immune** hit: `Cmd_typecalc` records
//!    `MOVE_RESULT_DOESNT_AFFECT_FOE` and falls through rather than jumping,
//!    and the `NO_EFFECT` test is only the third operand.
//! 2. **A `MOVE_EFFECT_CERTAIN` byte takes the *first* branch, which draws
//!    nothing** — as long as the hit had an effect. `MOVE_STRUGGLE`'s
//!    `EFFECT_RECOIL` script is the one such move this crate lets through
//!    ([`crate::hit`]'s exception), and it is why a landed Struggle costs
//!    one draw fewer than a landed Tackle. A `CERTAIN` byte on a
//!    *type-immune* hit falls through to the `else if` and **does** draw.
//! 3. **Serene Grace doubles `percentChance`.** It is an ability, and the
//!    only two this crate models are [`crate::ability`]'s, neither of which
//!    is Serene Grace — but it changes the *value*, never the draw count, so
//!    it could never move a stream even if it were modelled.
//!
//! # Fail-closed, twice over
//!
//! No move whose script writes a non-zero `MOVE_EFFECT_BYTE` is executable
//! by this crate: [`crate::battle::ensure_executable`] screens every move
//! against the four pipelines' allow-lists **before the first draw and
//! before any state changes**, and none of those lists contains a
//! [`SECONDARY_TRAMPOLINES`] row. So in production the hook's second operand
//! is a constant `false` and the drawn value is always discarded.
//!
//! [`spend_effect_chance_draw`] nevertheless evaluates the whole chain and
//! **refuses** — [`BattleError::UnportedSecondaryEffect`] — if a roll ever
//! lands on a trampoline byte. That is the "dispatch to a fail-closed stub"
//! half of the hook: the draw is upstream-faithful today, and the day issue
//! #323 ports paralysis/poison/confusion infliction it replaces one `Err`
//! arm here rather than inventing a second hook. Note the ordering that
//! makes the refusal honest: the draw is spent **first**, exactly as
//! upstream spends it, and only then is the unported byte reported — a
//! caller that recovers from the error still has a correctly-advanced
//! stream.
//!
//! # What this module is not
//!
//! `SetMoveEffect` itself (`:2270`-`:2700`) — the infliction semantics,
//! including the further `Random()` calls some of its cases make (a
//! sleep-turn roll, a confusion-turn roll) — is issue #323's, and is
//! deliberately absent rather than stubbed with a guess.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;

/// One `setmoveeffect X` / `goto BattleScript_EffectHit` trampoline: a move
/// effect whose damage half is exactly [`crate::hit`]'s pipeline and whose
/// only difference is the `MOVE_EFFECT_BYTE` it leaves for
/// [`spend_effect_chance_draw`] to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Trampoline {
    /// The `EFFECT_*` id whose `gBattleScriptsForMoveEffects` entry is this
    /// two-instruction script.
    pub effect: MoveEffect,
    /// The `MOVE_EFFECT_*` symbol the script writes, for diagnostics and for
    /// the slice that ports its infliction. Carried as its upstream name
    /// rather than its numeric value: nothing in this crate applies the
    /// effect yet, so a number here would be data no reader could check.
    pub move_effect: &'static str,
    /// The byte carries `MOVE_EFFECT_CERTAIN`, which takes
    /// `Cmd_seteffectwithchance`'s **draw-free** first branch on a hit that
    /// had an effect (module docs, consequence 2).
    pub certain: bool,
    /// The byte carries `MOVE_EFFECT_AFFECTS_USER`: the effect lands on the
    /// attacker, not the target.
    pub affects_user: bool,
}

/// Shorthand for a plain target-side, chance-rolled trampoline row.
const fn hits_foe(effect: u8, move_effect: &'static str) -> Trampoline {
    Trampoline {
        effect: MoveEffect(effect),
        move_effect,
        certain: false,
        affects_user: false,
    }
}

/// Every `setmoveeffect X` immediately followed by `goto
/// BattleScript_EffectHit` in `pokeemerald/data/battle_scripts_1.s`, paired
/// with the `EFFECT_*` id whose dispatch-table entry points at it — the
/// complete list of move effects for which the hook's `Random()` stops being
/// discarded, in effect-id order.
///
/// Transcribed whole rather than narrowed to the ones a Route 103 party
/// carries, because it is a membership test: a narrowed list would make the
/// *next* slice re-derive the same scan, and a byte missing from it would
/// silently take the "discard the roll" path — the failure this module
/// exists to prevent.
///
/// Several effects share one script (`EFFECT_BLAZE_KICK` reuses
/// `BattleScript_EffectBurnHit`, `EFFECT_POISON_TAIL` reuses
/// `BattleScript_EffectPoisonHit`, and `EFFECT_TWISTER`/
/// `EFFECT_FLINCH_MINIMIZE_HIT` both route through
/// `BattleScript_FlinchEffect`, `:1830-:1832`), which is why some
/// `MOVE_EFFECT_*` names appear more than once.
pub const SECONDARY_TRAMPOLINES: [Trampoline; 31] = [
    hits_foe(2, "MOVE_EFFECT_POISON"),       // EFFECT_POISON_HIT, :320
    hits_foe(4, "MOVE_EFFECT_BURN"),         // EFFECT_BURN_HIT, :363
    hits_foe(5, "MOVE_EFFECT_FREEZE"),       // EFFECT_FREEZE_HIT, :367
    hits_foe(6, "MOVE_EFFECT_PARALYSIS"),    // EFFECT_PARALYZE_HIT, :371
    hits_foe(31, "MOVE_EFFECT_FLINCH"),      // EFFECT_FLINCH_HIT, :669
    hits_foe(34, "MOVE_EFFECT_PAYDAY"),      // EFFECT_PAY_DAY, :721
    hits_foe(36, "MOVE_EFFECT_TRI_ATTACK"),  // EFFECT_TRI_ATTACK, :732
    hits_foe(42, "MOVE_EFFECT_WRAP"),        // EFFECT_TRAP, :836
    hits_foe(68, "MOVE_EFFECT_ATK_MINUS_1"), // EFFECT_ATTACK_DOWN_HIT, :1041
    hits_foe(69, "MOVE_EFFECT_DEF_MINUS_1"), // EFFECT_DEFENSE_DOWN_HIT, :1045
    hits_foe(70, "MOVE_EFFECT_SPD_MINUS_1"), // EFFECT_SPEED_DOWN_HIT, :1049
    hits_foe(71, "MOVE_EFFECT_SP_ATK_MINUS_1"), // EFFECT_SPECIAL_ATTACK_DOWN_HIT, :1053
    hits_foe(72, "MOVE_EFFECT_SP_DEF_MINUS_1"), // EFFECT_SPECIAL_DEFENSE_DOWN_HIT, :1057
    hits_foe(73, "MOVE_EFFECT_ACC_MINUS_1"), // EFFECT_ACCURACY_DOWN_HIT, :1061
    hits_foe(76, "MOVE_EFFECT_CONFUSION"),   // EFFECT_CONFUSE_HIT, :1072
    hits_foe(105, "MOVE_EFFECT_STEAL_ITEM"), // EFFECT_THIEF, :1441
    hits_foe(125, "MOVE_EFFECT_BURN"),       // EFFECT_THAW_HIT, :1680
    Trampoline {
        // EFFECT_RAPID_SPIN, :1717
        effect: MoveEffect(129),
        move_effect: "MOVE_EFFECT_RAPIDSPIN",
        certain: true,
        affects_user: true,
    },
    Trampoline {
        // EFFECT_DEFENSE_UP_HIT, :1765
        effect: MoveEffect(138),
        move_effect: "MOVE_EFFECT_DEF_PLUS_1",
        certain: false,
        affects_user: true,
    },
    Trampoline {
        // EFFECT_ATTACK_UP_HIT, :1769
        effect: MoveEffect(139),
        move_effect: "MOVE_EFFECT_ATK_PLUS_1",
        certain: false,
        affects_user: true,
    },
    Trampoline {
        // EFFECT_ALL_STATS_UP_HIT, :1773
        effect: MoveEffect(140),
        move_effect: "MOVE_EFFECT_ALL_STATS_UP",
        certain: false,
        affects_user: true,
    },
    hits_foe(146, "MOVE_EFFECT_FLINCH"), // EFFECT_TWISTER, :1831
    hits_foe(150, "MOVE_EFFECT_FLINCH"), // EFFECT_FLINCH_MINIMIZE_HIT, :1831 (via :1901's goto)
    Trampoline {
        // EFFECT_FAKE_OUT, :2051 -- the CERTAIN bit means a landed Fake
        // Out spends *zero* draws, so omitting this row would be a live
        // stream desync, not just a dropped effect.
        effect: MoveEffect(158),
        move_effect: "MOVE_EFFECT_FLINCH",
        certain: true,
        affects_user: false,
    },
    Trampoline {
        // EFFECT_SUPERPOWER, :2389
        effect: MoveEffect(182),
        move_effect: "MOVE_EFFECT_ATK_DEF_DOWN",
        certain: true,
        affects_user: true,
    },
    hits_foe(188, "MOVE_EFFECT_KNOCK_OFF"), // EFFECT_KNOCK_OFF, :2476
    Trampoline {
        // EFFECT_DOUBLE_EDGE, :2568
        effect: MoveEffect(198),
        move_effect: "MOVE_EFFECT_RECOIL_33",
        certain: true,
        affects_user: true,
    },
    hits_foe(200, "MOVE_EFFECT_BURN"),  // EFFECT_BLAZE_KICK, :363
    hits_foe(202, "MOVE_EFFECT_TOXIC"), // EFFECT_POISON_FANG, :2641
    Trampoline {
        // EFFECT_OVERHEAT, :2649
        effect: MoveEffect(204),
        move_effect: "MOVE_EFFECT_SP_ATK_TWO_DOWN",
        certain: true,
        affects_user: true,
    },
    hits_foe(209, "MOVE_EFFECT_POISON"), // EFFECT_POISON_TAIL, :320
];

/// The [`Trampoline`] `effect`'s battle script is, or `None` when its script
/// writes no `MOVE_EFFECT_BYTE` at all (every effect this crate can execute).
#[must_use]
pub fn trampoline_for_effect(effect: MoveEffect) -> Option<&'static Trampoline> {
    SECONDARY_TRAMPOLINES.iter().find(|t| t.effect == effect)
}

/// Whether `effect`'s script writes a secondary-effect byte —
/// i.e. whether the hook's roll would *land* rather than be discarded.
#[must_use]
pub fn is_secondary_effect(effect: MoveEffect) -> bool {
    trampoline_for_effect(effect).is_some()
}

/// Run `Cmd_seteffectwithchance` for `move_id` on a hit that
/// `hit_had_effect` (i.e. was not `MOVE_RESULT_NO_EFFECT`).
///
/// Draws **exactly one** `Random()` on every path except the one upstream
/// also skips: a `MOVE_EFFECT_CERTAIN` byte on a hit that had an effect
/// (module docs, consequence 2). For every move this crate can currently
/// execute the drawn value is discarded, and the function returns `Ok(())`.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`. Nothing is
///   drawn before that lookup.
/// - [`BattleError::UnportedSecondaryEffect`] if the chain reached
///   `SetMoveEffect` — the fail-closed stub for infliction this slice does
///   not port (module docs). The draw, if upstream would have made one, has
///   already happened when this is returned.
pub fn spend_effect_chance_draw(
    dex: &Dex,
    move_id: MoveId,
    hit_had_effect: bool,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    let trampoline = trampoline_for_effect(mv.effect);

    // Branch 1 (`:2917`): a CERTAIN byte on a hit that landed. No draw.
    if trampoline.is_some_and(|t| t.certain) && hit_had_effect {
        return Err(BattleError::UnportedSecondaryEffect(move_id));
    }

    // Branch 2 (`:2923`): the roll is the *leading* operand, so it is spent
    // before either suppressing test is even looked at.
    let roll = u32::from(rng.next_u16()) % 100 < u32::from(mv.secondary_effect_chance);
    if roll && trampoline.is_some() && hit_had_effect {
        return Err(BattleError::UnportedSecondaryEffect(move_id));
    }
    Ok(())
}

#[cfg(test)]
#[path = "secondary/tests.rs"]
mod tests;
