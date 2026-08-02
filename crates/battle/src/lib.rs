//! Battle subsystem: typed core data, the Gen-3 damage formula, and the
//! turn engine built on top of it.
//!
//! Issue #125 (S-6 slice 1) laid the typed data foundation and deterministic
//! damage pipeline — data-first, no state machine, no AI, no UI. Issue #159
//! (S-6 slice 2) adds the turn engine on top: [`battle::Battle`], an owned
//! state machine that plays a full headless single wild battle (action
//! selection → turn resolution → faint/exp → victory/defeat/run) using
//! [`hit::resolve_hit`] to assemble [`accuracy`], [`critical`], and
//! [`damage`] into upstream's exact battle-script step order, plus
//! [`turn_order`] (priority/speed/speed-tie), [`escape`] (the run-away
//! formula), [`exp`] (faint exp gain), [`pokemon::BattlePokemon`] (computed
//! battle stats), and [`wild`] (wild-encounter construction from species +
//! level via the upstream personality/nature/IV RNG draws).
//!
//! Species base stats, move data, and the type-effectiveness chart are
//! already extracted upstream data living in [`assets`] `(no-verbatim)`;
//! [`dex::Dex`] bundles typed access to them for battle-formula callers.
//! This crate adds the upstream tables `assets` didn't yet carry —
//! nature stat modifiers ([`nature`], upstream `gNatureStatTable`) and
//! stat-stage multipliers ([`stat_stage`], upstream `gStatStageRatios`) —
//! plus [`damage`]'s re-implementation of `CalculateBaseDamage`
//! (`pokeemerald/src/pokemon.c`) and the STAB/type-effectiveness/random-roll
//! battle-script steps around it.
//!
//! Move-effect breadth is the sharp edge of this slice, so it is enforced
//! rather than assumed: only moves whose `EFFECT_*` runs upstream's plain
//! `BattleScript_EffectHit` are executable ([`hit::is_ordinary_hit_effect`]),
//! and [`battle::Battle::new`] rejects a battle in which either mon knows
//! anything else — before any state exists or any RNG is drawn, so an
//! unsupported configuration can never leave a half-played turn behind.
//!
//! Out of scope for this slice (see each module's own docs for exactly what
//! is/isn't modelled): trainer/wild AI (`I-5`), battle UI/animations,
//! overworld transition, abilities, held items, non-volatile status
//! conditions, weather, multi/double battles, and move-effect breadth beyond
//! the v1 first-encounter damaging-move path (status moves, stat-changing
//! moves' actual execution, multi-hit/recoil/drain, ...).

pub mod accuracy;
pub mod battle;
pub mod critical;
pub mod damage;
pub mod dex;
pub mod error;
pub mod escape;
pub mod exp;
pub mod hit;
pub mod nature;
pub mod pokemon;
pub mod stat_stage;
pub mod turn_order;
pub mod wild;

pub use battle::{Battle, BattleEvent, BattleOutcome, PlayerAction, TurnError};
pub use damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, apply_type_effectiveness,
    base_damage, calculate_damage, has_stab, BattleRng, DamageInput, MoveCategory, Weather,
    STRUGGLE,
};
pub use dex::Dex;
pub use error::BattleError;
pub use hit::{ensure_resolvable, is_ordinary_hit_effect, HitOutcome};
pub use nature::{Nature, Stat};
pub use pokemon::{
    BattlePokemon, Ivs, MoveSlot, StatStages, Stats, MAX_IV, MAX_LEVEL, MAX_MON_MOVES, MIN_LEVEL,
    MOVE_NONE, SPECIES_NONE,
};
pub use stat_stage::StatStage;
pub use wild::build_wild_pokemon;
