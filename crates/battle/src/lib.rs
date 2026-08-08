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
//! level via the upstream personality/nature/IV RNG draws, plus
//! [`wild::initial_moveset`] — `GiveBoxMonInitialMoveset`, added by issue
//! #169 so an overworld encounter can derive the *real* wild moveset for a
//! rolled species/level instead of being handed one).
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
//! rather than assumed: a move is only executable if its `EFFECT_*` runs
//! upstream's plain `BattleScript_EffectHit`
//! ([`hit::is_ordinary_hit_effect`]) or is one of the three
//! `BattleScript_EffectStatDown`-family stat-lowering effects
//! ([`stat_change::is_stat_lowering_effect`], added by issue #199 so real
//! Route 101 wild movesets — Zigzagoon's Growl, Wurmple's String Shot — and
//! real starter movesets — Treecko's Leer — construct and play), guarded at
//! a two-sided boundary. [`battle::Battle::new`] rejects a battle whose
//! **wild** mon knows anything else (its rejection loop can land on any
//! slot), while the **player's** moveset may carry unsupported moves and
//! each *chosen* slot is validated per turn instead. Both checks run before
//! any RNG is drawn, so an unsupported configuration or pick can never leave
//! a half-played turn behind.
//!
//! Issue #187 adds `BATTLE_TYPE_FIRST_BATTLE` — the Route 101 intro
//! Zigzagoon fight's rules — as [`battle::Battle::new`]'s `first_battle`
//! flag: crit suppression ([`hit::resolve_hit`]'s `suppress_crit`, see
//! [`critical`]'s module docs), running forbidden
//! ([`BattleError::RunForbidden`], see `escape`'s module docs), and the
//! wild opponent's narrow AI-branch move choice (`battle`'s private
//! `opponent_ai` submodule, not upstream's general trainer AI, which stays
//! `I-5`). Issue #221 adds the scripted intro's own construction —
//! `SetUpBattleVarsAndBirchZigzagoon`'s Zigzagoon, built by
//! [`wild::build_pokemon_with_random_personality`] rather than
//! [`wild::build_wild_pokemon`] (see that function's docs for the exact
//! upstream draw-order difference) — and its own headless driver, both in
//! `crates/pokeemerald-rs/src/flow/first_battle.rs`. The "don't leave Prof.
//! Birch!" narrative trigger that upstream reaches it through has no script
//! engine to run it and stays unmodelled (that module's own docs, and the
//! `src/battle_setup.c` ledger entry); every ordinary Route 101 grass
//! encounter still constructs with `first_battle = false`
//! (`crates/pokeemerald-rs/src/flow/wild_encounter.rs`).
//!
//! Out of scope for this slice (see each module's own docs for exactly what
//! is/isn't modelled): general trainer/wild AI (`I-5`), battle UI/animations,
//! overworld transition, abilities, held items, non-volatile status
//! conditions, weather, multi/double battles, Mist/Substitute (see
//! [`stat_change`]'s module docs for why those two are a documented boundary
//! rather than dead code), and move-effect breadth beyond the v1
//! first-encounter damaging-move path plus the three stat-lowering effects
//! above (other status moves, stat-raising moves, multi-hit/recoil/drain,
//! ...).

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
pub mod stat_change;
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
pub use stat_change::{is_stat_lowering_effect, LoweredStat, StatChangeOutcome};
pub use stat_stage::StatStage;
pub use wild::{
    build_pokemon_with_random_personality, build_wild_pokemon, ensure_wild_startable,
    initial_moveset,
};
