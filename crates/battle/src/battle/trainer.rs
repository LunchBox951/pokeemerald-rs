//! `BATTLE_TYPE_TRAINER`'s battle-side context (S-6, issue #237): the
//! opponent's remaining party, the trainer's `gTrainers` metadata, and the
//! prize money a win pays out.
//!
//! Split out of `battle.rs` for the same reason [`super::opponent_ai`] was
//! `(oop-boundaries)`: everything here is about the *trainer* — who they
//! are, what they still have on the bench, what beating them is worth — and
//! none of it needs [`crate::battle::Battle`]'s turn-order/PP/event
//! machinery. `Battle` owns one [`TrainerContext`] for the whole battle and
//! asks it questions; the context never reaches back.
//!
//! # What a trainer battle changes, and where each change lives
//!
//! `gBattleTypeFlags & BATTLE_TYPE_TRAINER` (set by
//! `BattleSetup_ConfigureTrainerBattle`/`DoTrainerBattle`,
//! `pokeemerald/src/battle_setup.c:459`) changes five observable
//! things relative to an ordinary wild battle. All five are modelled:
//!
//! 1. **Running is refused outright.** `HandleTurnActionSelectionState`
//!    tests `gBattleTypeFlags & BATTLE_TYPE_TRAINER` *before*
//!    `IsRunningFromBattleImpossible` (`src/battle_main.c:4331`-`:4337`) and
//!    runs `BattleScript_PrintCantRunFromTrainer`
//!    (`data/battle_scripts_1.s:3071`-`:3073`), printing
//!    `STRINGID_NORUNNINGFROMTRAINERS` — "No! There's no running\nfrom a
//!    TRAINER battle!" (`src/battle_message.c:330`) — and re-prompting the
//!    action menu. [`crate::battle::Battle::take_turn`] rejects
//!    [`crate::battle::PlayerAction::Run`] with
//!    [`BattleError::NoRunningFromTrainer`] before any draw, exactly as it
//!    rejects the `first_battle` case (see [`crate::escape`]'s module docs
//!    for why an action-selection-time gate belongs there rather than inside
//!    the escape formula).
//! 2. **The opponent is a party, not one mon.** [`TrainerContext::bench`]
//!    holds every party member behind the active one, in party order.
//! 3. **A fainted opponent is replaced, not the end of the battle.** See
//!    [`TrainerContext::send_out_next`].
//! 4. **Experience is worth 1.5x.** `Cmd_getexp`'s
//!    `if (gBattleTypeFlags & BATTLE_TYPE_TRAINER) gBattleMoveDamage =
//!    (gBattleMoveDamage * 150) / 100` (`src/battle_script_commands.c:3378`-`:3379`)
//!    — [`crate::exp::trainer_faint_exp`].
//! 5. **Winning pays prize money.** [`trainer_money`], below.
//!
//! # What is *not* modelled here
//!
//! - **The trainer's own items.** `gTrainers[].items` (Super Potions and
//!   friends) are loaded into `BATTLE_HISTORY->trainerItems`
//!   (`src/battle_ai_script_commands.c:290`-`:307`) and spent by
//!   the item-use AI in `src/battle_ai_switch_items.c`. No item system
//!   exists in this crate, and every Route 103 rival carries `.items = {}`,
//!   so the whole path is absent rather than stubbed.
//! - **A party mon's *held* item, as an in-battle effect.** The item itself
//!   is now represented — [`crate::pokemon::BattlePokemon::held_item`],
//!   written here by [`build_trainer_pokemon`] exactly where
//!   `CreateNPCTrainerParty`'s `SetMonData(&party[i], MON_DATA_HELD_ITEM,
//!   &partyData[i].heldItem)` writes it (`src/battle_main.c:2046`, `:2060`)
//!   — but nothing *runs* it, so `ensure_held_item_playable` refuses a
//!   non-`ITEM_NONE` item before the first draw rather than fielding a mon
//!   whose Oran Berry never fires. See [`BattleError::UnsupportedHeldItem`].
//! - **Mid-battle switching.** `ShouldSwitch`/`AI_ShouldSwitchIfPerishSong`
//!   (`battle_ai_switch_items.c:429`, called from `:543`) can switch a *healthy* mon out;
//!   that is not modelled, so the only switch this crate performs is the
//!   forced post-faint one.
//! - **`GetMostSuitableMonToSwitchInto`'s preference order** — see
//!   [`TrainerContext::send_out_next`] for why party order is the honest
//!   model for this battle and exactly where the two could diverge.
//! - **The Amulet Coin money multiplier.** `gBattleStruct->moneyMultiplier`
//!   is `1` unless a party mon holds `ITEM_AMULET_COIN`
//!   (`Cmd_handleballthrow`'s neighbours set it; `battle_main.c:3118`
//!   initialises it to `1`). Held items are out of scope, so
//!   [`trainer_money`] hard-codes the `1` case.

use assets::items::ItemId;
use assets::trainers::{AiFlags, TrainerClass, TrainerData, TrainerId, TrainerParty, TrainerTable};
use assets::{MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};

/// `MAX_PER_STAT_IVS` (`pokeemerald/include/constants/pokemon.h`): the
/// numerator `CreateNPCTrainerParty` scales a party entry's `iv` byte by.
pub const MAX_PER_STAT_IVS: u16 = 31;

/// `SHINY_ODDS` (`pokeemerald/include/constants/pokemon.h`): a rolled OT id
/// whose shiny value falls below this is rejected and redrawn by
/// `OT_ID_RANDOM_NO_SHINY` (see [`roll_non_shiny_ot_id`]).
pub const SHINY_ODDS: u16 = 8;

/// `CreateNPCTrainerParty`'s per-mon IV derivation
/// (`pokeemerald/src/battle_main.c:2013`): `fixedIV = partyData[i].iv *
/// MAX_PER_STAT_IVS / 255`, a `u8` in `0..=31` applied to **every** stat
/// alike (`CreateBoxMon`'s `fixedIV < USE_RANDOM_IVS` branch,
/// `src/pokemon.c:2265`-`:2272`, sets all six from the one value and draws
/// nothing).
///
/// Every Route 103 rival party entry has `iv = 0`, so their starters really
/// do run on all-zero IVs — pinned by this module's tests rather than left
/// implicit.
// The parameter is upstream's `partyData[i].iv` byte, named in full here:
// CodeQL's `rust/hard-coded-cryptographic-value` reads a parameter named
// exactly `iv` as a cryptographic initialization-vector sink (the PR #167
// false-positive convention -- rename, never dismiss).
#[must_use]
pub fn fixed_ivs(individual_value: u8) -> Ivs {
    let value = u8::try_from(u16::from(individual_value) * MAX_PER_STAT_IVS / u16::from(u8::MAX))
        .unwrap_or(u8::MAX);
    Ivs {
        hp: value,
        attack: value,
        defense: value,
        speed: value,
        sp_attack: value,
        sp_defense: value,
    }
}

/// `GET_SHINY_VALUE` (`pokeemerald/include/pokemon.h`): the xor-fold of both
/// halves of the OT id and both halves of the personality. A mon is shiny
/// when this is below [`SHINY_ODDS`].
#[must_use]
pub const fn shiny_value(ot_id: u32, personality: u32) -> u16 {
    // `HIHALF(x) ^ LOHALF(x)` for each argument, then xored together --
    // written out rather than via a closure, which `const fn` cannot call.
    #[allow(clippy::cast_possible_truncation)]
    {
        ((ot_id >> 16) as u16)
            ^ (ot_id as u16)
            ^ ((personality >> 16) as u16)
            ^ (personality as u16)
    }
}

/// `CreateBoxMon`'s `OT_ID_RANDOM_NO_SHINY` loop
/// (`pokeemerald/src/pokemon.c:2223`-`:2231`): draw `Random32()` until the
/// resulting mon would **not** be shiny.
///
/// ```text
/// do {
///     value = Random32();
///     shinyValue = GET_SHINY_VALUE(value, personality);
/// } while (shinyValue < SHINY_ODDS);
/// ```
///
/// This is the *only* RNG a trainer party mon costs: its personality is
/// fixed by `CreateNPCTrainerParty`'s seeded formula and its IVs are fixed
/// by the party table, so neither draws. One `Random32()` is two
/// [`BattleRng::next_u16`] draws, and the loop retries with probability
/// `8/65536` — so a trainer mon almost always costs exactly two draws, but
/// the loop is reproduced rather than assumed because *where* the stream
/// lands is observable `(behavioral-fidelity)`.
///
/// Unlike [`crate::wild::build_wild_pokemon`]'s wild path — which takes the
/// player's own OT id with no draw at all (`OT_ID_PLAYER_ID`) — this branch
/// is genuinely RNG-consuming, which is why it is modelled here rather than
/// noted as a simplification.
#[must_use]
pub fn roll_non_shiny_ot_id(personality: u32, rng: &mut impl BattleRng) -> u32 {
    loop {
        let value = rng.next_u32();
        if shiny_value(value, personality) >= SHINY_ODDS {
            return value;
        }
    }
}

/// Build one NPC trainer party member: `CreateMon(..., fixedIV,
/// hasFixedPersonality = TRUE, personality, OT_ID_RANDOM_NO_SHINY, 0)`
/// (`pokeemerald/src/battle_main.c:2014`), plus the
/// `SetMonData(MON_DATA_HELD_ITEM)` write the two
/// `F_TRAINER_PARTY_HELD_ITEM` shapes make right after it (`:2046`, `:2060`)
/// — pass [`ItemId::NONE`] for the two shapes that make no such write.
///
/// Distinct from both of [`crate::wild`]'s construction paths, and the
/// difference is entirely in the draws:
///
/// | Path | personality | IVs | OT id |
/// |---|---|---|---|
/// | [`crate::wild::build_wild_pokemon`] (`CreateWildMon`) | nature roll + rejection loop | 2 draws | player's, 0 draws |
/// | [`crate::wild::build_pokemon_with_random_personality`] (`CreateMon`, free nature) | 1 `Random32` | 2 draws | player's, 0 draws |
/// | this (`CreateNPCTrainerParty`) | **fixed**, 0 draws | **fixed**, 0 draws | [`roll_non_shiny_ot_id`], ≥1 `Random32` |
///
/// Nothing here is Route-103-specific: `personality` and `fixed_iv` are the
/// caller's, exactly as species/level/moves are, because the seeded
/// personality formula belongs to `CreateNPCTrainerParty`'s loop rather than
/// to `CreateMon` (the integration layer that owns the loop supplies it —
/// `crates/pokeemerald-rs/src/flow/route103_rival.rs`).
///
/// # Errors
///
/// Whatever [`BattlePokemon::validate`] reports for the caller's
/// species/level/moveset — checked **before the first draw**, so a rejected
/// request leaves the shared stream exactly as it found it, the same rule
/// [`crate::wild::build_wild_pokemon`] follows.
// Eight parameters because upstream's own call is eight-wide: `CreateMon(mon,
// species, level, fixedIV, hasFixedPersonality, personality, otIdType,
// fixedOtId)` (`src/pokemon.c:2196`), of which this reproduces six, plus the
// `dex` this crate threads instead of globals and the `SetMonData(
// MON_DATA_HELD_ITEM)` write that follows on two of the four party shapes.
// The arity is inherent to the upstream record `(behavioral-fidelity)`, the
// same convention `assets`' own `battle_moves::m` row constructor records.
#[allow(clippy::too_many_arguments)]
pub fn build_trainer_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    fixed_iv: Ivs,
    personality: u32,
    moves: Vec<MoveId>,
    held_item: ItemId,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    BattlePokemon::validate(dex, species, level, &moves)?;
    let ot_id = roll_non_shiny_ot_id(personality, rng);
    Ok(
        BattlePokemon::new(dex, species, level, fixed_iv, personality, moves)?
            .with_original_trainer_id(ot_id)
            .with_held_item(held_item),
    )
}

/// `gTrainerMoneyTable` (`pokeemerald/src/battle_main.c:474`-`:531`): the
/// per-class prize-money multiplier, transcribed in upstream table order.
///
/// The trailing `{0xFF, 5}` sentinel row is re-expressed as
/// [`DEFAULT_MONEY_VALUE`] plus a linear search that falls through, rather
/// than as a `0xFF`-keyed entry: upstream's loop stops at whichever comes
/// first, a class match or the sentinel, so "not listed above uses this" is
/// the honest reading `(no-verbatim)`.
///
/// 55 rows for 66 `TRAINER_CLASS_*` ids: the eleven absent ones
/// (`PKMN_TRAINER_1`/`_2`, `COOLTRAINER_2`, `RS_PROTAG`, and the seven
/// Frontier Brain classes) really do fall through to the sentinel upstream.
const TRAINER_MONEY_TABLE: [(u8, u32); 55] = [
    (0x03, 5),  // TRAINER_CLASS_TEAM_AQUA
    (0x0b, 10), // TRAINER_CLASS_AQUA_ADMIN
    (0x0d, 20), // TRAINER_CLASS_AQUA_LEADER
    (0x0f, 10), // TRAINER_CLASS_AROMA_LADY
    (0x10, 15), // TRAINER_CLASS_RUIN_MANIAC
    (0x11, 12), // TRAINER_CLASS_INTERVIEWER
    (0x12, 1),  // TRAINER_CLASS_TUBER_F
    (0x13, 1),  // TRAINER_CLASS_TUBER_M
    (0x39, 3),  // TRAINER_CLASS_SIS_AND_BRO
    (0x05, 12), // TRAINER_CLASS_COOLTRAINER
    (0x0e, 6),  // TRAINER_CLASS_HEX_MANIAC
    (0x14, 50), // TRAINER_CLASS_LADY
    (0x15, 20), // TRAINER_CLASS_BEAUTY
    (0x16, 50), // TRAINER_CLASS_RICH_BOY
    (0x17, 15), // TRAINER_CLASS_POKEMANIAC
    (0x08, 2),  // TRAINER_CLASS_SWIMMER_M
    (0x0c, 8),  // TRAINER_CLASS_BLACK_BELT
    (0x18, 8),  // TRAINER_CLASS_GUITARIST
    (0x19, 8),  // TRAINER_CLASS_KINDLER
    (0x1a, 4),  // TRAINER_CLASS_CAMPER
    (0x38, 10), // TRAINER_CLASS_OLD_COUPLE
    (0x1c, 15), // TRAINER_CLASS_BUG_MANIAC
    (0x1d, 6),  // TRAINER_CLASS_PSYCHIC
    (0x1e, 20), // TRAINER_CLASS_GENTLEMAN
    (0x1f, 25), // TRAINER_CLASS_ELITE_FOUR
    (0x20, 25), // TRAINER_CLASS_LEADER
    (0x21, 5),  // TRAINER_CLASS_SCHOOL_KID
    (0x22, 4),  // TRAINER_CLASS_SR_AND_JR
    (0x24, 20), // TRAINER_CLASS_POKEFAN
    (0x0a, 10), // TRAINER_CLASS_EXPERT
    (0x25, 4),  // TRAINER_CLASS_YOUNGSTER
    (0x26, 50), // TRAINER_CLASS_CHAMPION
    (0x27, 10), // TRAINER_CLASS_FISHERMAN
    (0x28, 10), // TRAINER_CLASS_TRIATHLETE
    (0x29, 12), // TRAINER_CLASS_DRAGON_TAMER
    (0x06, 8),  // TRAINER_CLASS_BIRD_KEEPER
    (0x2a, 3),  // TRAINER_CLASS_NINJA_BOY
    (0x2b, 6),  // TRAINER_CLASS_BATTLE_GIRL
    (0x2c, 10), // TRAINER_CLASS_PARASOL_LADY
    (0x2d, 2),  // TRAINER_CLASS_SWIMMER_F
    (0x1b, 4),  // TRAINER_CLASS_PICNICKER
    (0x2e, 3),  // TRAINER_CLASS_TWINS
    (0x2f, 8),  // TRAINER_CLASS_SAILOR
    (0x07, 15), // TRAINER_CLASS_COLLECTOR
    (0x32, 15), // TRAINER_CLASS_RIVAL
    (0x04, 10), // TRAINER_CLASS_PKMN_BREEDER
    (0x34, 12), // TRAINER_CLASS_PKMN_RANGER
    (0x09, 5),  // TRAINER_CLASS_TEAM_MAGMA
    (0x31, 10), // TRAINER_CLASS_MAGMA_ADMIN
    (0x35, 20), // TRAINER_CLASS_MAGMA_LEADER
    (0x36, 4),  // TRAINER_CLASS_LASS
    (0x33, 4),  // TRAINER_CLASS_BUG_CATCHER
    (0x02, 10), // TRAINER_CLASS_HIKER
    (0x37, 8),  // TRAINER_CLASS_YOUNG_COUPLE
    (0x23, 10), // TRAINER_CLASS_WINSTRATE
];

/// `gTrainerMoneyTable`'s `{0xFF, 5}` sentinel value — "any trainer class
/// not listed above uses this" (`src/battle_main.c:530`).
pub const DEFAULT_MONEY_VALUE: u32 = 5;

/// The `gTrainerMoneyTable` multiplier for `class`, or
/// [`DEFAULT_MONEY_VALUE`] when the class is not listed
/// (`src/battle_script_commands.c:5618`-`:5623`, the linear search that
/// stops at the `0xFF` sentinel).
#[must_use]
pub fn money_value_for_class(class: TrainerClass) -> u32 {
    TRAINER_MONEY_TABLE
        .iter()
        .find(|(id, _)| *id == class.index())
        .map_or(DEFAULT_MONEY_VALUE, |(_, value)| *value)
}

/// The last level in `party`, upstream's `lastMonLevel`
/// (`src/battle_script_commands.c:5593`-`:5615`: whichever of the four
/// party shapes applies, always `party[partySize - 1].lvl`).
///
/// `None` for the empty `TRAINER_NONE` party, which
/// [`GetTrainerMoneyToGive`](trainer_money) can never be handed in practice
/// — a battle against it could not start.
#[must_use]
fn last_mon_level(party: TrainerParty) -> Option<u8> {
    match party {
        TrainerParty::NoItemDefaultMoves(p) => p.last().map(|m| m.lvl),
        TrainerParty::NoItemCustomMoves(p) => p.last().map(|m| m.lvl),
        TrainerParty::ItemDefaultMoves(p) => p.last().map(|m| m.lvl),
        TrainerParty::ItemCustomMoves(p) => p.last().map(|m| m.lvl),
    }
}

/// `GetTrainerMoneyToGive` (`src/battle_script_commands.c:5578`), reduced to
/// the single-battle, no-Amulet-Coin case: `4 * lastMonLevel *
/// moneyMultiplier * gTrainerMoneyTable[i].value` with `moneyMultiplier ==
/// 1`.
///
/// `Cmd_getmoneyreward` (`:5635`) then adds this straight to
/// `gSaveBlock1Ptr->money` and buffers it into the "got ¥N for winning!"
/// string. This crate has no money field to add it *to*, so
/// [`crate::battle::Battle`] reports the amount as
/// [`crate::battle::BattleEvent::MoneyGained`] and the integration layer
/// owns crediting it — the same division of labour every other terminal
/// event follows.
///
/// The `BATTLE_TYPE_TWO_OPPONENTS` and `BATTLE_TYPE_DOUBLE` arms (`:5625`,
/// `:5627`) are not modelled: neither is reachable for a single trainer
/// battle, and the double arm's extra `* 2` would need a double-battle
/// engine to be observable at all. The `TRAINER_SECRET_BASE` arm (`:5583`)
/// is likewise absent — secret bases have no model here.
///
/// Returns `0` for a trainer with an empty party (only `TRAINER_NONE`),
/// which no startable battle can reach.
#[must_use]
pub fn trainer_money(trainer: &TrainerData) -> u32 {
    let Some(level) = last_mon_level(trainer.party) else {
        return 0;
    };
    4 * u32::from(level) * money_value_for_class(trainer.class)
}

/// The `BATTLE_TYPE_TRAINER` half of a [`crate::battle::Battle`]'s state
/// `(oop-boundaries)`: who the opponent is, what they have left, and what
/// beating them pays.
///
/// Constructed by [`crate::battle::Battle::new_trainer`] from a
/// [`TrainerId`] and an already-built party; the *construction* of that
/// party (`CreateNPCTrainerParty`'s seeded personalities and fixed IVs) is
/// the integration layer's, exactly as `SetUpBattleVarsAndBirchZigzagoon`'s
/// Zigzagoon is (`crates/pokeemerald-rs/src/flow/first_battle.rs`).
#[derive(Debug, Clone)]
pub struct TrainerContext {
    id: TrainerId,
    class: TrainerClass,
    ai_flags: AiFlags,
    money: u32,
    /// Every party member *behind* the active one, in `gTrainers[].party`
    /// order. The active mon lives in [`crate::battle::Battle`]'s `enemy`
    /// field, so this is the bench and only the bench.
    bench: Vec<BattlePokemon>,
}

impl TrainerContext {
    /// Build the context for `trainer`, taking ownership of the bench (every
    /// party member after the lead).
    #[must_use]
    pub(crate) fn new(id: TrainerId, data: &TrainerData, bench: Vec<BattlePokemon>) -> Self {
        Self {
            id,
            class: data.class,
            ai_flags: data.ai_flags,
            money: trainer_money(data),
            bench,
        }
    }

    /// The opponent's `TRAINER_*` id.
    #[must_use]
    pub const fn id(&self) -> TrainerId {
        self.id
    }

    /// The opponent's `TRAINER_CLASS_*`.
    #[must_use]
    pub const fn class(&self) -> TrainerClass {
        self.class
    }

    /// The opponent's `gTrainers[].aiFlags` — the `AI_SCRIPT_*` bitset
    /// [`super::trainer_ai`] runs.
    #[must_use]
    pub const fn ai_flags(&self) -> AiFlags {
        self.ai_flags
    }

    /// The prize money a win pays ([`trainer_money`]), fixed at construction
    /// because upstream's own inputs (`gTrainers[]`, not the live party) are.
    #[must_use]
    pub const fn money(&self) -> u32 {
        self.money
    }

    /// How many party members are still on the bench (the active mon is not
    /// counted).
    #[must_use]
    pub fn bench_len(&self) -> usize {
        self.bench.len()
    }

    /// The bench, in party order.
    #[must_use]
    pub fn bench(&self) -> &[BattlePokemon] {
        &self.bench
    }

    /// The next mon the trainer sends out after theirs faints, in **party
    /// order**, skipping any already-fainted member — or `None` when the
    /// bench is exhausted, which is the trainer's defeat.
    ///
    /// # What upstream does, and how far this matches
    ///
    /// `OpponentHandleChoosePokemon` (`src/battle_controller_opponent.c:1621`)
    /// asks `GetMostSuitableMonToSwitchInto`
    /// (`src/battle_ai_switch_items.c:629`) first and only falls back to a
    /// party-order scan (`:1637`-`:1655`: the first member with non-zero HP
    /// that is not already out) when that returns `PARTY_SIZE`. This models
    /// the fallback scan and **not** the preference pass, for three reasons:
    ///
    /// - it draws no RNG either way in a single battle (the one `Random()`
    ///   in `GetMostSuitableMonToSwitchInto` is the doubles-only
    ///   `opposingBattler = Random() & BIT_FLANK` at `:660`), so the shared
    ///   stream is identical whichever branch runs `(behavioral-fidelity)`;
    /// - the preference pass only returns a mon that has **both** favourable
    ///   defensive typing and at least one super-effective move against what
    ///   is currently out (`:727`-`:738`); otherwise it marks every candidate
    ///   invalid and returns `PARTY_SIZE`, i.e. the fallback;
    /// - it cannot differ at all for the battle this slice targets: every
    ///   Route 103 rival party is one mon, so there is never a second
    ///   candidate to prefer.
    ///
    /// The divergence is therefore confined to a multi-mon trainer whose
    /// bench happens to hold a super-effective answer, which no battle this
    /// slice constructs can be. It is recorded on the
    /// `src/battle_ai_switch_items.c#GetMostSuitableMonToSwitchInto` ledger
    /// entry rather than papered over.
    ///
    /// A fainted bench member is skipped, not sent out, matching the
    /// fallback scan's own `MON_DATA_HP != 0` test. Nothing in this crate can
    /// currently faint a benched mon (only the active one takes damage), so
    /// the skip is upstream's rule kept honest rather than a reachable path.
    pub(crate) fn send_out_next(&mut self) -> Option<BattlePokemon> {
        while !self.bench.is_empty() {
            let mon = self.bench.remove(0);
            if !mon.is_fainted() {
                return Some(mon);
            }
        }
        None
    }
}

/// Look up `trainer` in the extracted `gTrainers` table.
///
/// # Errors
///
/// [`BattleError::UnknownTrainer`] if the id is outside
/// `0..`[`TrainerTable::LEN`].
pub fn trainer_data(trainer: TrainerId) -> Result<&'static TrainerData, BattleError> {
    TrainerTable::new()
        .get(trainer)
        .ok_or(BattleError::UnknownTrainer(trainer))
}

/// Both per-move screens [`crate::battle::Battle::new_trainer`] applies to
/// every party member, composed — see that method's own "Two screens, both
/// before the first draw" section for why the pair is inseparable.
///
/// `ai_flags` is the trainer's own `gTrainers[].aiFlags`, because the second
/// screen is flag-aware since issue #293: a move only has to be scoreable by
/// the scripts *this* trainer runs (see
/// [`super::trainer_ai::ensure_scoreable`]).
///
/// Shared with [`ensure_trainer_party_startable`] so the per-move halves of
/// the pre-flight and the real handoff cannot drift apart: a move the
/// pre-flight admitted but `new_trainer` rejected would be exactly the RNG
/// leak the pre-flight exists to prevent. (The other screens -- empty
/// party, `trainer_data`, `ensure_supported_flags`, per-mon `validate` --
/// are *duplicated* between the two paths, not shared; a screen added to
/// `new_trainer` alone can still drift and must be mirrored here.)
pub(crate) fn ensure_move_playable(
    dex: &Dex,
    move_id: MoveId,
    ai_flags: AiFlags,
) -> Result<(), BattleError> {
    crate::battle::ensure_executable(dex, move_id)?;
    super::trainer_ai::ensure_scoreable(dex, move_id, ai_flags)
}

/// Whether a party mon holding `held_item` can be fielded — the held-item
/// counterpart of `ensure_move_playable`, run by both
/// [`ensure_trainer_party_startable`]'s pre-flight and
/// [`crate::battle::Battle::new_trainer`]'s last line of defence (issue
/// #293).
///
/// [`ItemId::NONE`] passes; **every other item is refused**, and that is the
/// whole rule rather than a table with one row missing: this crate has no
/// `ItemBattleEffects` / `ITEM_EFFECT_*` machinery at all, so no item can
/// act. The check is on the *item*, not the party shape — upstream's
/// `F_TRAINER_PARTY_HELD_ITEM` rows are free to store `ITEM_NONE` (several
/// do), and such a mon is fielded normally, so the item is now carried
/// through construction ([`crate::pokemon::BattlePokemon::held_item`])
/// rather than the whole shape being rejected as unrepresentable.
///
/// # Errors
///
/// [`BattleError::UnsupportedHeldItem`] carrying `held_item`.
pub(crate) const fn ensure_held_item_playable(held_item: ItemId) -> Result<(), BattleError> {
    // `PartialEq` is not callable from a `const fn` on stable, so the
    // reserved `ITEM_NONE` id is compared field-wise.
    if held_item.0 == ItemId::NONE.0 {
        Ok(())
    } else {
        Err(BattleError::UnsupportedHeldItem(held_item))
    }
}

/// One prospective `CreateNPCTrainerParty` party member as
/// [`ensure_trainer_party_startable`] needs to see it: the four `CreateMon`
/// (plus post-`CreateMon`) inputs the screens actually depend on. The
/// personality and the IVs are deliberately absent — both are fixed values
/// upstream computes without drawing, and the only IV producer
/// (`fixed_ivs`, `iv * 31 / 255`) is range-safe by construction, so the
/// post-draw `InvalidIv` refusal in `BattlePokemon::new` is unreachable from
/// a trainer party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerPartyMon<'a> {
    /// `partyData[i].species`.
    pub species: SpeciesId,
    /// `partyData[i].lvl`.
    pub level: u8,
    /// The moveset the member will actually field — the party table's own
    /// fixed moveset, or [`crate::wild::initial_moveset`]'s level-up result
    /// for the `F_TRAINER_PARTY_*_DEFAULT_MOVES` shapes.
    pub moves: &'a [MoveId],
    /// `partyData[i].heldItem` for the two `F_TRAINER_PARTY_HELD_ITEM`
    /// shapes, [`ItemId::NONE`] for the two that have no such field
    /// (issue #293) — screened by `ensure_held_item_playable`.
    pub held_item: ItemId,
}

/// Whether the whole `CreateNPCTrainerParty` →
/// [`crate::battle::Battle::new_trainer`] handoff would accept `trainer`
/// fielding `party` — every screen the two make between them, composed,
/// with **no mon built and no RNG drawn**. The trainer-battle counterpart of
/// [`crate::wild::ensure_wild_startable`], and it exists for the same reason
/// (issue #264 review).
///
/// Upstream builds a trainer's party mon by mon, each drawing its own
/// `OT_ID_RANDOM_NO_SHINY` id ([`roll_non_shiny_ot_id`]) as it goes, and
/// only then reaches the battle's own screens — so a party this engine
/// cannot fight has no stream-faithful failure mode once construction has
/// started: those draws are already spent, with no upstream counterpart, and
/// a caller that retries every frame (a sight cone the player is standing
/// in) spends them again on every frame. Screening first is the only shape
/// that leaves the shared stream exactly as it found it, which is what a
/// refusal must cost `(behavioral-fidelity)`.
///
/// The order below follows the real handoff's composed order — per-mon
/// [`BattlePokemon::validate`] (which [`build_trainer_pokemon`] runs mon by
/// mon, ahead of the battle), then
/// `super::trainer_ai`'s `ensure_supported_flags` and the `player_lead`
/// screens beside it, then the per-move pair — with one caveat: this screen
/// resolves `trainer_data` *first*, where the composed path only reaches it
/// inside `new_trainer` after the mons validate, so a party that is both
/// mis-specced and on an unknown trainer id reports the trainer error here
/// and the mon error there (unreachable through
/// `start_npc_trainer_battle`, whose `party_entries` resolves the trainer
/// before any mon exists). For every reachable input, screening early does
/// not change *which* error a rejected party reports, only how early it is
/// discovered.
///
/// `player_lead` is here — rather than left to the caller — because it is the
/// AI's own **target**, and the question that has to be asked about it is
/// exactly the kind of refusal that must not cost the stream anything: does
/// its species have two possible abilities, which `Cmd_get_ability` would
/// guess between with a `Random() & 1`
/// (`super::trainer_ai::ensure_deterministic_target_ability`)? A screen a
/// caller can forget to run is a screen that leaks (issue #293 review). Its
/// fainted-ness is asked in the same place for the same reason, closing the
/// one gap `crate::battle::Battle::new_trainer` used to hold alone.
///
/// # Errors
///
/// [`BattleError::EmptyTrainerParty`] for an empty `party`,
/// [`BattleError::UnknownTrainer`] for an id outside `gTrainers`,
/// [`BattleError::UnsupportedAiFlags`] for a `gTrainers[].aiFlags` bit this
/// crate's AI does not model, [`BattleError::AmbiguousTargetAbility`] or
/// [`BattleError::FaintedBattler`] for `player_lead`,
/// [`BattleError::UnsupportedHeldItem`] for a member holding an item nothing
/// here runs (`ensure_held_item_playable`), and whatever
/// [`BattlePokemon::validate`] or `ensure_move_playable` report for a
/// member's species/level/moveset.
///
/// The held-item screen runs **last**, after every move, so a party that is
/// both unfieldable and item-carrying reports the move — the more actionable
/// half, and the one a move-coverage slice can move.
pub fn ensure_trainer_party_startable(
    dex: &Dex,
    trainer: TrainerId,
    player_lead: &BattlePokemon,
    party: &[TrainerPartyMon<'_>],
) -> Result<(), BattleError> {
    if party.is_empty() {
        return Err(BattleError::EmptyTrainerParty(trainer));
    }
    let data = trainer_data(trainer)?;
    for mon in party {
        BattlePokemon::validate(dex, mon.species, mon.level, mon.moves)?;
    }
    super::trainer_ai::ensure_supported_flags(data.ai_flags)?;
    super::trainer_ai::ensure_deterministic_target_ability(dex, player_lead.species())?;
    if player_lead.is_fainted() {
        return Err(BattleError::FaintedBattler(true));
    }
    for mon in party {
        for move_id in mon.moves {
            ensure_move_playable(dex, *move_id, data.ai_flags)?;
        }
    }
    for mon in party {
        ensure_held_item_playable(mon.held_item)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_trainer_pokemon, ensure_trainer_party_startable, fixed_ivs, money_value_for_class,
        roll_non_shiny_ot_id, shiny_value, trainer_data, trainer_money, TrainerPartyMon,
        DEFAULT_MONEY_VALUE, SHINY_ODDS,
    };
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use assets::items::ItemId;
    use assets::trainers::{TrainerClass, TrainerId};
    use assets::{MoveId, SpeciesId};

    /// `TRAINER_MAY_ROUTE_103_MUDKIP` (`include/constants/opponents.h:533`).
    const MAY_ROUTE_103_MUDKIP: TrainerId = TrainerId(529);

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
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            let v = self.values[self.index];
            self.index += 1;
            v
        }
    }

    #[test]
    fn a_zero_iv_party_entry_really_means_all_zero_ivs() {
        // Every Route 103 rival party entry is `.iv = 0`
        // (trainer_parties.h:6784 and friends): 0 * 31 / 255 == 0.
        assert_eq!(fixed_ivs(0).as_array(), [0; 6]);
        // The other end of the byte: 255 * 31 / 255 == 31 (MAX_PER_STAT_IVS).
        assert_eq!(fixed_ivs(255).as_array(), [31; 6]);
        // And an intermediate one, truncating: 100 * 31 / 255 == 12.
        assert_eq!(fixed_ivs(100).as_array(), [12; 6]);
    }

    #[test]
    fn the_ot_id_loop_redraws_only_while_the_result_would_be_shiny() {
        const PERSONALITY: u32 = 0;
        // With personality 0, shinyValue is just the xor-fold of the OT id.
        // A first draw folding to < SHINY_ODDS must be rejected; the second
        // must be kept.
        let shiny: u32 = 0x0000_0001; // fold = 0 ^ 1 = 1 < 8
        assert!(shiny_value(shiny, PERSONALITY) < SHINY_ODDS);
        let plain: u32 = 0x0000_00FF; // fold = 0 ^ 255 = 255 >= 8
        assert!(shiny_value(plain, PERSONALITY) >= SHINY_ODDS);

        #[allow(clippy::cast_possible_truncation)]
        let mut rng = SequenceRng::new([
            shiny as u16,
            (shiny >> 16) as u16,
            plain as u16,
            (plain >> 16) as u16,
        ]);
        assert_eq!(roll_non_shiny_ot_id(PERSONALITY, &mut rng), plain);
        assert_eq!(rng.index, 4, "two Random32 draws: one rejected, one kept");
    }

    #[test]
    fn a_trainer_mon_draws_only_its_ot_id() {
        let dex = Dex::new();
        // 0x00FF folds to 255, accepted on the first draw.
        let mut rng = SequenceRng::new([0x00FF, 0x0000]);
        let mon = build_trainer_pokemon(
            &dex,
            SpeciesId(277), // Treecko
            5,
            fixed_ivs(0),
            0x1234_5678,
            vec![MoveId(1)], // Pound
            ItemId::NONE,
            &mut rng,
        )
        .expect("Treecko/Pound are dex-resident");
        assert_eq!(
            rng.index, 2,
            "personality and IVs are fixed; only the OT id draws"
        );
        assert_eq!(mon.personality(), 0x1234_5678);
        assert_eq!(mon.ivs().as_array(), [0; 6]);
        assert_eq!(mon.original_trainer_id(), 0x0000_00FF);
    }

    #[test]
    fn a_rejected_request_draws_nothing_at_all() {
        let dex = Dex::new();
        let mut rng = SequenceRng::new([]);
        let error = build_trainer_pokemon(
            &dex,
            SpeciesId(277),
            0, // below MIN_LEVEL
            fixed_ivs(0),
            0,
            vec![MoveId(1)],
            ItemId::NONE,
            &mut rng,
        )
        .unwrap_err();
        assert_eq!(error, crate::error::BattleError::InvalidLevel(0));
        assert_eq!(rng.index, 0, "validation runs ahead of the OT-id loop");
    }

    #[test]
    fn the_route_103_rival_pays_the_rival_classs_prize_money() {
        // gTrainerMoneyTable's TRAINER_CLASS_RIVAL row is 15
        // (battle_main.c:518), the party's last (only) mon is level 5
        // (trainer_parties.h:6916-6921), and moneyMultiplier is 1:
        // 4 * 5 * 1 * 15 == 300.
        let data = trainer_data(MAY_ROUTE_103_MUDKIP).expect("a real TRAINER_* id");
        assert_eq!(data.class, TrainerClass(0x32));
        assert_eq!(money_value_for_class(data.class), 15);
        assert_eq!(trainer_money(data), 300);
    }

    #[test]
    fn an_unlisted_class_falls_through_to_the_sentinel_value() {
        // TRAINER_CLASS_COOLTRAINER_2 (0x30) and the Frontier Brain classes
        // (0x3a..=0x41) are absent from gTrainerMoneyTable, so upstream's
        // loop runs to the {0xFF, 5} sentinel.
        assert_eq!(
            money_value_for_class(TrainerClass(0x30)),
            DEFAULT_MONEY_VALUE
        );
        assert_eq!(
            money_value_for_class(TrainerClass(0x41)),
            DEFAULT_MONEY_VALUE
        );
        // ...while a listed one does not.
        assert_eq!(money_value_for_class(TrainerClass(0x26)), 50); // CHAMPION
    }

    /// A single-ability player lead for the pre-flight's own `player_lead`
    /// argument: Treecko's `gSpeciesInfo[].abilities[1]` is `ABILITY_NONE`,
    /// so `Cmd_get_ability` answers it without a draw and the screen passes.
    fn player_lead() -> crate::pokemon::BattlePokemon {
        crate::pokemon::BattlePokemon::new(
            &Dex::new(),
            SpeciesId(277),
            5,
            fixed_ivs(0),
            0,
            vec![MoveId(1)],
        )
        .expect("Treecko/Pound is a valid pairing")
    }

    /// The pre-flight accepts exactly the party the real handoff accepts —
    /// the Route 103 rival moveset every `route103_rival` test already
    /// proves constructible.
    #[test]
    fn ensure_trainer_party_startable_accepts_a_real_constructible_party() {
        let dex = Dex::new();
        let moves = [MoveId(1)]; // MOVE_POUND
        assert_eq!(
            ensure_trainer_party_startable(
                &dex,
                MAY_ROUTE_103_MUDKIP,
                &player_lead(),
                &[TrainerPartyMon {
                    species: SpeciesId(277), // Treecko
                    level: 5,
                    moves: &moves,
                    held_item: ItemId::NONE,
                }],
            ),
            Ok(())
        );
    }

    /// The whole point of the pre-flight (issue #264 review): it must report
    /// *exactly* what the real handoff would, while the real handoff can
    /// only report it after `CreateNPCTrainerParty` has already spent the
    /// party's OT-id draws off the shared stream.
    #[test]
    fn the_pre_flight_reports_what_the_real_handoff_would_but_without_the_draws() {
        let dex = Dex::new();
        // MOVE_HARDEN: neither an ordinary-hit effect nor one of the three
        // stat-lowering ones, so `ensure_executable` refuses it.
        let moves = [MoveId(106)];
        let screened = ensure_trainer_party_startable(
            &dex,
            MAY_ROUTE_103_MUDKIP,
            &player_lead(),
            &[TrainerPartyMon {
                species: SpeciesId(277),
                level: 5,
                moves: &moves,
                held_item: ItemId::NONE,
            }],
        )
        .expect_err("Harden is not executable by this turn engine");

        // The same party, built for real: two draws for the OT id, and only
        // then the identical refusal.
        let mut rng = SequenceRng::new([0x00FF, 0x0000]);
        let enemy = build_trainer_pokemon(
            &dex,
            SpeciesId(277),
            5,
            fixed_ivs(0),
            0,
            moves.to_vec(),
            ItemId::NONE,
            &mut rng,
        )
        .expect("Treecko/Harden is a valid pairing -- only the turn engine refuses it");
        assert_eq!(rng.index, 2, "the leak: draws are spent before the screen");
        let handoff = crate::battle::Battle::new_trainer(
            Dex::new(),
            player_lead(),
            MAY_ROUTE_103_MUDKIP,
            vec![enemy],
            &mut rng,
        )
        .expect_err("the battle refuses the same moveset");
        assert_eq!(screened, handoff);
    }

    /// An empty party and an unknown id are the two shape errors the
    /// pre-flight must raise itself rather than defer to the battle.
    #[test]
    fn the_pre_flight_rejects_an_empty_party_and_an_unknown_trainer() {
        let dex = Dex::new();
        assert_eq!(
            ensure_trainer_party_startable(&dex, MAY_ROUTE_103_MUDKIP, &player_lead(), &[]),
            Err(crate::error::BattleError::EmptyTrainerParty(
                MAY_ROUTE_103_MUDKIP
            ))
        );
        let moves = [MoveId(1)];
        assert_eq!(
            ensure_trainer_party_startable(
                &dex,
                TrainerId(60_000),
                &player_lead(),
                &[TrainerPartyMon {
                    species: SpeciesId(277),
                    level: 5,
                    moves: &moves,
                    held_item: ItemId::NONE,
                }],
            ),
            Err(crate::error::BattleError::UnknownTrainer(TrainerId(60_000)))
        );
    }

    #[test]
    fn an_out_of_range_trainer_id_is_rejected_rather_than_panicking() {
        assert_eq!(
            trainer_data(TrainerId(60_000)).unwrap_err(),
            crate::error::BattleError::UnknownTrainer(TrainerId(60_000))
        );
    }
}
