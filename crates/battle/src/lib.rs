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
//! ([`hit::is_ordinary_hit_effect`]) or is one of the
//! `BattleScript_EffectStatUp`/`StatDown`-family stat-changing effects
//! ([`stat_change::is_stat_change_effect`], added by issue #199 so real
//! Route 101 wild movesets — Zigzagoon's Growl, Wurmple's String Shot — and
//! real starter movesets — Treecko's Leer — construct and play, and widened
//! by issue #322 to the whole 18-row family — raises and drops, every stat
//! either script reaches, both magnitudes upstream uses — plus Clear Body's
//! stat-drop refusal and a fainting battler's stage reset), guarded at a
//! two-sided boundary. [`battle::Battle::new`] rejects a battle whose
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
//! Issue #237 adds `BATTLE_TYPE_TRAINER` — the scripted Route 103 rival
//! battle's rules — as a second constructor, [`battle::Battle::new_trainer`],
//! rather than a second flag: a trainer battle needs *state* a wild one does
//! not (a party bench, a prize purse, `gTrainers[].aiFlags`), so that state
//! lives in [`battle::trainer::TrainerContext`] and `Battle` gates every
//! battle-type delta on which shape it was built for. The five deltas —
//! running refused outright ([`BattleError::NoRunningFromTrainer`], a
//! *different* upstream gate from `first_battle`'s), a party opponent, a
//! forced post-faint send-out in party order, `x1.5` experience
//! ([`exp::trainer_faint_exp`]), and prize money on a win
//! ([`battle::BattleEvent::MoneyGained`]) — are enumerated with their
//! upstream citations in [`battle::trainer`]'s module docs. The opponent's
//! move choice is upstream's real `AI_SCRIPT_*` scoring pipeline
//! (`battle`'s private `trainer_ai` submodule): `AI_CheckBadMove`,
//! `AI_TryToFaint`, `AI_CheckViability` and `AI_SetupFirstTurn`, narrowed to
//! the move effects a level-5 starter can carry and screened at construction
//! so nothing outside that narrowing can silently mis-draw. The battle's own
//! construction — `CreateNPCTrainerParty`'s seeded personalities and fixed
//! IVs — and its headless driver are
//! `crates/pokeemerald-rs/src/flow/route103_rival.rs`, the same split issue
//! #221 used for the first battle; Route 103's overworld reachability
//! (the rival's sight cone and approach script) is a later slice.
//!
//! Issue #264 wires Route 103's *sight* trainers to that same construction,
//! and adds the screen it needed to do so honestly:
//! [`battle::trainer::ensure_trainer_party_startable`], the trainer-side
//! counterpart of [`wild::ensure_wild_startable`]. Both compose the screens
//! a battle would apply anyway into a pre-flight an integration layer can
//! run **before the first draw** — the only stream-faithful way to refuse a
//! party this engine cannot fight, since a per-frame trigger that refuses
//! *after* `CreateNPCTrainerParty`'s per-mon OT-id draws would spend the
//! shared stream on every frame the player stands in a cone.
//!
//! Out of scope for this slice (see each module's own docs for exactly what
//! is/isn't modelled): the *general* trainer AI beyond the four scripts
//! above and mid-battle switching AI (`I-5`), battle UI/animations,
//! overworld transition, held items, non-volatile status conditions,
//! weather, multi/double battles, Mist/Substitute (see [`stat_change`]'s
//! module docs for why those two are a documented boundary rather than dead
//! code), abilities beyond Clear Body (same module, same reason), and
//! move-effect breadth beyond the early slice's damaging-move path plus the
//! widened stat-change family above (other status moves,
//! multi-hit/recoil/drain, ...).

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

pub use battle::trainer::{
    build_trainer_pokemon, ensure_trainer_party_startable, fixed_ivs, roll_non_shiny_ot_id,
    shiny_value, trainer_data, trainer_money, TrainerContext, TrainerPartyMon, SHINY_ODDS,
};
pub use battle::{Battle, BattleEvent, BattleOutcome, PlayerAction, TurnError};
pub use damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, apply_type_effectiveness,
    base_damage, calculate_damage, has_stab, BattleRng, DamageInput, MoveCategory, Weather,
    STRUGGLE,
};
pub use dex::Dex;
pub use error::BattleError;
pub use exp::{trainer_faint_exp, wild_faint_exp};
pub use hit::{ensure_resolvable, is_ordinary_hit_effect, HitOutcome};
pub use nature::{Nature, Stat};
pub use pokemon::{
    BattlePokemon, Ivs, MoveSlot, StatStages, Stats, MAX_IV, MAX_LEVEL, MAX_MON_MOVES, MIN_LEVEL,
    MOVE_NONE, SPECIES_NONE,
};
pub use stat_change::{
    is_stat_change_effect, stat_change_for_effect, ChangedStat, StatChangeDirection,
    StatChangeEffect, StatChangeMagnitude, StatChangeOutcome, CLEAR_BODY, HYPER_CUTTER, KEEN_EYE,
    WHITE_SMOKE,
};
pub use stat_stage::StatStage;
pub use wild::{
    build_pokemon_with_random_personality, build_wild_pokemon, ensure_wild_startable,
    initial_moveset,
};
