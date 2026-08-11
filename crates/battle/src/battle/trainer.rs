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
#[must_use]
pub fn fixed_ivs(iv: u16) -> Ivs {
    #[allow(clippy::cast_possible_truncation)]
    let value = (iv * MAX_PER_STAT_IVS / 255) as u8;
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
/// (`pokeemerald/src/battle_main.c:2014`).
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
pub fn build_trainer_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    fixed_iv: Ivs,
    personality: u32,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    BattlePokemon::validate(dex, species, level, &moves)?;
    let ot_id = roll_non_shiny_ot_id(personality, rng);
    Ok(
        BattlePokemon::new(dex, species, level, fixed_iv, personality, moves)?
            .with_original_trainer_id(ot_id),
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

#[cfg(test)]
mod tests {
    use super::{
        build_trainer_pokemon, fixed_ivs, money_value_for_class, roll_non_shiny_ot_id, shiny_value,
        trainer_data, trainer_money, DEFAULT_MONEY_VALUE, SHINY_ODDS,
    };
    use crate::damage::BattleRng;
    use crate::dex::Dex;
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

    #[test]
    fn an_out_of_range_trainer_id_is_rejected_rather_than_panicking() {
        assert_eq!(
            trainer_data(TrainerId(60_000)).unwrap_err(),
            crate::error::BattleError::UnknownTrainer(TrainerId(60_000))
        );
    }
}
