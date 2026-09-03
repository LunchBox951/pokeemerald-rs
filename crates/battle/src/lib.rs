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
//! one of the battle scripts this crate reproduces — the seven pipelines
//! `battle::ensure_executable` composes:
//!
//! | pipeline | script | added by |
//! |---|---|---|
//! | [`hit`] | `BattleScript_EffectHit` ([`hit::is_ordinary_hit_effect`]) | #125 |
//! | [`stat_change`] | the `BattleScript_EffectStatUp`/`StatDown` family ([`stat_change::is_stat_change_effect`]) | #199, widened by #322 |
//! | [`drain`] | `BattleScript_EffectAbsorb` ([`drain::is_drain_effect`]) | #321 |
//! | [`fixed_damage`] | `_Sonicboom` / `_DragonRage` / `_LevelDamage` ([`fixed_damage::is_fixed_damage_effect`]) | #321 |
//! | [`multi_hit`] | `BattleScript_EffectMultiHit` ([`multi_hit::is_multi_hit_effect`]) | #321 |
//! | [`flag_move`] | `_Splash` / `_FocusEnergy` / `_Charge` ([`flag_move::is_flag_move_effect`]) | #321 |
//! | [`paralyze`] | `BattleScript_EffectParalyze` ([`paralyze::is_paralyze_effect`]) | this slice |
//!
//! The screen is guarded at a two-sided boundary. [`battle::Battle::new`]
//! rejects a battle whose **opposing** mon knows anything else (its
//! rejection loop can land on any slot), while the **player's** moveset may
//! carry unsupported moves and each *chosen* slot is validated per turn
//! instead. Both checks run before any RNG is drawn and before any HP, PP or
//! stage changes, so an unsupported configuration or pick can never leave a
//! half-played turn behind — the reason it matters is that a script this
//! crate does not reproduce computes different damage *and* spends a
//! different number of `Random()` calls, and a shared stream that advanced
//! the wrong number of steps is wrong for the rest of the battle.
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
//! ([`battle::BattleEvent::MoneyGained`]) — each carry their upstream
//! citation beside the code that owns them. The opponent's
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
//! Issue #321 (parent decomposition #311, position 2) widens move-effect
//! breadth by four pipelines at once, each its own module because each
//! reproduces a *different* battle script with a different `Random()` spend
//! (the tables in each module's docs are the contract): [`drain`]
//! (`BattleScript_EffectAbsorb`, 3 draws for a landed move where an
//! ordinary one costs 4, because the script never reaches
//! `seteffectwithchance`), [`fixed_damage`] (Sonic Boom / Dragon Rage /
//! Seismic Toss / Night Shade — 2 draws, no crit roll and no damage roll,
//! and a flat figure no stat can move), [`multi_hit`]
//! (`Cmd_setmultihitcounter`'s **two-stage** 2..5 draw, then a crit and a
//! damage roll per hit, then one trailing effect-chance draw for the whole
//! move), and [`flag_move`] (Splash, Focus Energy, Charge — **zero** draws
//! on every path, which [`flag_move::resolve_flag_move`]'s signature
//! enforces by taking no RNG at all).
//!
//! Two supporting concepts arrived with them. [`secondary`] is the shared
//! post-damage hook — `Cmd_seteffectwithchance`, previously inline in
//! [`hit`] — now its own module because the five damaging scripts disagree
//! about whether and when they reach it; it spends the draw exactly where
//! upstream does and **fails closed**
//! ([`BattleError::UnportedSecondaryEffect`]) if a roll ever lands on a
//! `MOVE_EFFECT_*` byte whose infliction is not ported, which is issue
//! #323's slice. [`volatile`] carries the two `status2`/`gStatuses3` bits
//! the flag-only moves set, read by [`critical::crit_stage`] (Focus
//! Energy's `+2` crit stages) and [`hit::damage_core`] (Charge's Electric
//! doubling), and ticked down at end of turn.
//!
//! Six abilities land here too, bundled with the move family that exposes
//! them (#311's decomposition rule) rather than with a speculative ability
//! layer: [`ability::pinch_boosts_power`] — Overgrow's `x1.5` power boost
//! at or below a third of maximum HP, applied inside the damage formula —
//! and [`ability::inverts_drain`] — Liquid Ooze turning a drain heal into
//! damage on the attacker, in the drain script's own message and faint
//! order. Issue #391 added the two damage-path pairs the same rule pulls
//! in: [`ability::suppresses_critical_hits`] — Battle Armor and Shell
//! Armor, which short-circuit `Cmd_critcalc` off the *defender* so the crit
//! and its `Random()` draw are both skipped — and
//! [`ability::huge_power_attack`] — Huge Power and Pure Power, doubling the
//! *attacker*'s raw Attack for a physical move before the stat-stage
//! multiply, in the real damage path and the trainer AI's estimate alike.
//! [`pokemon::BattlePokemon::ability`] derives the ability from the
//! personality exactly as `CreateBoxMon`/`GetAbilityBySpecies` do, so a
//! seeded party's abilities are deterministic.
//!
//! Issue #304 closes the two move-slot gaps that were left open above.
//! [`pokemon::PpBonuses`] is upstream's packed `ppBonuses` byte —
//! [`pokemon::BattlePokemon`] carries it, [`pokemon::calculate_pp_with_bonus`]
//! is `CalculatePPWithBonus`, and a slot's capacity (what a heal restores to,
//! what PP counts down from) is now the PP-Up-adjusted maximum rather than the
//! move's base PP. And the four-known-moves case is no longer a silent
//! decline: [`pokemon::BattlePokemon::apply_experience`] parks a
//! [`pokemon::PendingMoveLearn`] on the mon itself, which must be answered
//! with a [`pokemon::MoveLearnDecision`] — the mon owns the open question,
//! so a stale copy cannot be replayed and one mon's prompt cannot be
//! answered on another. [`battle::Battle`] surfaces it as
//! [`battle::Battle::pending_move_learn`] /
//! [`battle::Battle::resolve_move_learn`] and enforces it by refusing a
//! turn while the question is open. The pause is faithful to upstream's
//! one-level-at-a-time award loop: the mon holds *at* the prompted level
//! (the award's remainder unconsumed on the token), and everything after
//! the knockout — a trainer's forced send-out, the money payout, the
//! battle's end — waits for the last answer, as upstream finishes the
//! level-up script before `HandleFaintedMonActions`' aftermath. Answering
//! resumes the same walk, so declining still continues to the next learnset
//! entry; replacing a slot clears that slot's PP Ups, exactly as
//! `RemoveMonPPBonus` + `SetMonMoveSlot` do — unless the slot holds an HM
//! move, which is refused the way `IsHMMove2` refuses it
//! ([`error::BattleError::HmMoveCantBeForgotten`]). What this crate still
//! does not own is the *asking* — there is no message layer or summary
//! screen here, and there is deliberately no default answer baked in.
//!
//! Out of scope for this slice (see each module's own docs for exactly what
//! is/isn't modelled): the *general* trainer AI beyond the four scripts
//! above and mid-battle switching AI (`I-5`) — which is now the *first*
//! screen Route 103's sight-trainer parties hit, since #321 got four of
//! them past `ensure_executable` and into
//! `battle::trainer_ai::ensure_scoreable` (issue #325) — battle
//! UI/animations, overworld transition, every ability but Overgrow, Liquid
//! Ooze, Battle Armor, Shell Armor, Huge Power, Pure Power (all six above)
//! and the four stat-drop guards — Clear Body, White Smoke, Keen Eye,
//! Hyper Cutter ([`stat_change`]'s module docs; Shield Dust is the one
//! guard left unmodelled) — held items, every primary status but
//! [`status1::Status1::Paralysed`] (poison, confusion, sleep, freeze, burn,
//! toxic — see [`status1`]'s module docs), weather, multi/double
//! battles, Limber/Mist/Substitute/Safeguard/Protect (see [`stat_change`]'s
//! and [`paralyze`]'s module docs for why those are a documented boundary
//! rather than dead code), and the move effects the seven pipelines still do
//! not cover — Defense Curl (flag *and* stat raise, so it belongs with the
//! stat-change family), the secondary-effect trampolines
//! ([`secondary::SECONDARY_TRAMPOLINES`] lists all 31, none of them
//! [`paralyze::EFFECT_PARALYZE`]'s on-hit sibling `EFFECT_PARALYZE_HIT`),
//! recoil, OHKO, Counter, Bide, Leech Seed and the rest of the end-of-turn
//! residual family, and so on.
//!
//! This slice adds [`status1::Status1`] — [`pokemon::BattlePokemon`]'s
//! primary status, distinct from [`volatile::Volatiles`] — modelling
//! [`status1::Status1::Healthy`] and [`status1::Status1::Paralysed`].
//! [`paralyze`] is the seventh [`battle::Battle::execute_move`] pipeline
//! (`BattleScript_EffectParalyze`: Thunder Wave, Stun Spore, Glare), whose
//! type/status guards run before the accuracy draw. [`status1::Status1`]
//! also gates every mover through [`battle::Battle::act`]'s full-paralysis
//! draw, ported from `AtkCanceler_UnableToUseMove`'s `CANCELER_PARALYZED`
//! branch and ordered ahead of PP deduction and the no-PP abort, and
//! quarters [`pokemon::BattlePokemon::speed_for_turn_order`] after stat-stage
//! scaling.

pub mod ability;
pub mod accuracy;
pub mod battle;
pub mod critical;
pub mod damage;
pub mod dex;
pub mod drain;
pub mod error;
pub mod escape;
pub mod exp;
pub mod fixed_damage;
pub mod flag_move;
pub mod hit;
mod move_gate;
pub mod multi_hit;
pub mod nature;
pub mod paralyze;
pub mod pokemon;
pub mod secondary;
pub mod stat_change;
pub mod stat_stage;
pub mod status1;
pub mod turn_order;
pub mod volatile;
pub mod wild;

#[cfg(test)]
mod script_rng;

pub use ability::{inverts_drain, pinch_boosts_power, LIQUID_OOZE, OVERGROW};
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
pub use drain::{drain_amount, is_drain_effect, resolve_drain, DrainOutcome};
pub use error::BattleError;
pub use exp::{trainer_faint_exp, wild_faint_exp};
pub use fixed_damage::{is_fixed_damage_effect, resolve_fixed_damage_move, FixedDamage};
pub use flag_move::{is_flag_move_effect, resolve_flag_move, FlagMoveOutcome};
pub use hit::{accuracy_roll, damage_core, ensure_resolvable, is_ordinary_hit_effect, HitOutcome};
pub use multi_hit::{is_multi_hit_effect, roll_hit_count, MAX_HITS, MIN_HITS};
pub use nature::{Nature, Stat};
pub use paralyze::{is_paralyze_effect, resolve_paralyze_move, ParalyzeOutcome};
pub use pokemon::{
    calculate_pp_with_bonus, compute_stats_with_evs, BattlePokemon, Evs, Ivs, LearnedMove,
    MoveLearnDecision, MoveLearnResolution, MoveSlot, PendingMoveLearn, PpBonuses, StatStages,
    Stats, MAX_IV, MAX_LEVEL, MAX_MON_MOVES, MAX_PP_UPS, MIN_LEVEL, MOVE_NONE, SPECIES_NONE,
    SPECIES_SHEDINJA,
};
pub use secondary::{is_secondary_effect, spend_effect_chance_draw, Trampoline};
pub use stat_change::{
    is_stat_change_effect, stat_change_for_effect, ChangedStat, StatChangeDirection,
    StatChangeEffect, StatChangeMagnitude, StatChangeOutcome, CLEAR_BODY, HYPER_CUTTER, KEEN_EYE,
    WHITE_SMOKE,
};
pub use stat_stage::StatStage;
pub use status1::Status1;
pub use volatile::Volatiles;
pub use wild::{
    build_pokemon_with_random_personality, build_wild_pokemon, ensure_wild_startable,
    initial_moveset,
};
