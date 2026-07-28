//! Battle subsystem (S-6): typed core data and the Gen-3 damage formula.
//!
//! This slice (issue #125) is the typed data foundation and deterministic
//! damage pipeline every later battle slice builds on — data-first, no
//! state machine, no AI, no UI yet.
//!
//! Species base stats, move data, and the type-effectiveness chart are
//! already extracted upstream data living in [`assets`] `(no-verbatim)`;
//! [`dex::Dex`] bundles typed access to them for battle-formula callers.
//! This crate adds the two upstream tables `assets` didn't yet carry —
//! nature stat modifiers ([`nature`], upstream `gNatureStatTable`) and
//! stat-stage multipliers ([`stat_stage`], upstream `gStatStageRatios`) —
//! plus [`damage`]'s re-implementation of `CalculateBaseDamage`
//! (`pokeemerald/src/pokemon.c`) and the STAB/type-effectiveness/random-roll
//! battle-script steps around it.
//!
//! Out of scope for this slice: the battle state machine/turn order, AI,
//! animations, UI, and non-v1 move effects (tracked separately per issue
//! #125's notes).

pub mod damage;
pub mod dex;
pub mod error;
pub mod nature;
pub mod stat_stage;

pub use damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, apply_type_effectiveness,
    base_damage, calculate_damage, has_stab, BattleRng, DamageInput, MoveCategory, Weather,
};
pub use dex::Dex;
pub use error::BattleError;
pub use nature::{Nature, Stat};
pub use stat_stage::StatStage;
