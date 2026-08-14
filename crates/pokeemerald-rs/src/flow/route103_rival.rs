//! The scripted Route 103 rival battle's construction and its own headless
//! driver (issue #237, ladders to S-6 and gates I-5).
//!
//! Sibling of [`crate::flow::first_battle`], and split from it the same way
//! issue #221 split that module from [`crate::flow::wild_encounter`]: the
//! battle *rules* live in the `battle` crate
//! ([`battle::Battle::new_trainer`] and [`battle::trainer`]'s module docs),
//! while the *construction* — `CreateNPCTrainerParty`'s seeded personalities
//! and fixed IVs — needs both `engine` (for the Gen-3 charmap the seed is
//! computed over) and `battle`, so it belongs in the integration layer that
//! already depends on each `(oop-boundaries)`.
//!
//! # `CreateNPCTrainerParty`, and its very odd personality seed
//!
//! `pokeemerald/src/battle_main.c:1960`-`:2073`, reached from
//! `CB2_InitBattleInternal` (`:697`) for `gBattleTypeFlags &
//! BATTLE_TYPE_TRAINER`. Per party member `i`:
//!
//! ```text
//! personalityValue = doubleBattle ? 0x80 : (F_TRAINER_FEMALE ? 0x78 : 0x88);
//! for (j = 0; trainerName[j] != EOS; j++)   nameHash += trainerName[j];
//! for (j = 0; speciesName[j] != EOS; j++)   nameHash += speciesName[j];
//! personalityValue += nameHash << 8;
//! fixedIV = partyData[i].iv * MAX_PER_STAT_IVS / 255;
//! CreateMon(&party[i], species, lvl, fixedIV, TRUE, personalityValue, OT_ID_RANDOM_NO_SHINY, 0);
//! ```
//!
//! Three details that a "reasonable" reimplementation would get wrong, and
//! which [`trainer_party_personalities`] therefore reproduces exactly
//! `(behavioral-fidelity)`:
//!
//! 1. **`nameHash` is declared outside the loop and never reset.** The
//!    trainer's name is added again on *every* iteration, and each species
//!    name accumulates on top of the previous ones — so party member `i`'s
//!    seed carries the trainer name `i + 1` times plus the names of mons
//!    `0..=i`. A one-mon party (every Route 103 rival) never shows this, but
//!    a two-mon party would, so the accumulator is modelled rather than
//!    flattened to a per-mon hash.
//! 2. **The bytes are Gen-3 charmap bytes, not ASCII.** `gSpeciesNames` and
//!    `Trainer.trainerName` are stored in the game's own encoding
//!    (`pokeemerald/charmap.txt`: `'A'` is `0xBB`, not `0x41`), so the sum —
//!    and therefore the personality, and therefore the **nature** the
//!    personality derives — depends on that encoding. [`engine::text`]
//!    already owns the transcribed charmap, so this module reuses
//!    [`engine::text::char_to_byte`] rather than duplicating it, and fails
//!    closed ([`RivalBattleError::UnnamedCharacter`]) on a glyph it does not
//!    cover.
//! 3. **`personalityValue += nameHash << 8` is `u32` arithmetic that can
//!    overflow**, and does so silently in C. Reproduced with
//!    [`u32::wrapping_shl`]/[`u32::wrapping_add`].
//!
//! `CreateMon` itself then draws only its OT id
//! ([`battle::roll_non_shiny_ot_id`]) — the personality and IVs are both
//! fixed here — which is where this construction's whole RNG cost lives.
//!
//! # RNG stream
//!
//! Off the same single shared stream as every other `crate::flow` handoff
//! ([`crate::flow::wild_encounter`]'s module docs), in upstream's order:
//! `CreateNPCTrainerParty` runs at `CB2_InitBattleInternal:697`, before
//! `BeginBattleIntro` reaches `BattleStartClearSetData`'s `gRandomTurnNumber
//! = Random()` (`battle_main.c:3140`). So [`start_route103_rival_battle`]
//! draws **two per party member** (one `Random32` OT id each, barring the
//! `8/65536` shiny retry) and then hands the stream to
//! [`battle::Battle::new_trainer`] for its turn-number draw and conditional
//! speed-tie draw. For the real one-mon Route 103 party that is three draws
//! to reach turn one.
//!
//! Unlike the first battle's construction, there is **no** missing
//! `SetWildMonHeldItem` draw to account for here: its gate excludes
//! `BATTLE_TYPE_TRAINER` outright (`battle_main.c:700`, `src/pokemon.c:6682`),
//! so upstream does not spend it either.
//!
//! # Reachability: wired from real play since issue #248
//!
//! Issue #237 scoped the rules, the construction and the driver, and *not*
//! the overworld reachability; issue #248 wired that on top.
//! `overworld_phase::route103_rival_trigger` recognizes the rival's
//! `Route103_EventScript_Rival` object event on an A-press
//! (`is_rival_trigger`), and its `begin_route103_rival_battle` is this
//! module's production caller. What that trigger covers and what it still
//! defers is recorded on the `src/battle_setup.c#DoTrainerBattle` ledger
//! entry: the trainer sight cone (`TrainerApproachPlayer`), the
//! rival-approach cutscene, and `RivalEnd`'s seven post-battle script lines
//! remain unmodelled (issue #264).
//!
//! Which of the six `TRAINER_*_ROUTE_103_*` rivals a playthrough fights is
//! decided by that caller too: [`Rival::for_gender`] reads the saved
//! `player_gender` (always the *opposite* protagonist), and
//! [`PlayerStarter::from_species`] maps the party lead's own species —
//! `VAR_STARTER_MON` itself is still not modelled, so the lead's real
//! species is the honest stand-in. [`route103_rival_for`] exposes the
//! *table* — the mapping upstream's `Route103_EventScript_*` scripts
//! encode — independently of that derivation.

use assets::trainers::{TrainerData, TrainerId, TrainerParty};
use assets::{MoveId, SpeciesId, SpeciesNames};
use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, PlayerAction, StatStages};
use engine::rng::Rng;

use super::wild_encounter::SharedRng;

/// Which starter the *player* chose — the axis the Route 103 rival's own
/// party is picked along (upstream `VAR_STARTER_MON`, read by
/// `Route103_EventScript_Rival*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerStarter {
    /// `SPECIES_TREECKO` (`VAR_STARTER_MON == 0`).
    Treecko,
    /// `SPECIES_TORCHIC` (`VAR_STARTER_MON == 1`).
    Torchic,
    /// `SPECIES_MUDKIP` (`VAR_STARTER_MON == 2`).
    Mudkip,
}

impl PlayerStarter {
    /// The honest species -> [`PlayerStarter`] mapping issue #248 (I-5)
    /// needs to reach [`route103_rival_for`] from an actual battle-facing
    /// lead, since `VAR_STARTER_MON` itself is not modelled (module docs'
    /// "Explicitly out of scope" section: no starter-select UI exists, so
    /// nothing ever writes it). Every production lead this is asked about
    /// is `crate::new_game::PROVISIONAL_STARTER_SPECIES` (Treecko, the
    /// stand-in for the un-ported Birch-bag handout), but the mapping
    /// covers the real three starters, not just that one, so it stays
    /// correct if a future slice ever lets the mon in slot 0 differ.
    ///
    /// `None` for any other species -- unreachable in production (the
    /// provisional starter is always one of the three), but a real `None`
    /// rather than a guessed starter for, say, a test lead built around an
    /// arbitrary species.
    #[must_use]
    pub const fn from_species(species: SpeciesId) -> Option<Self> {
        match species.0 {
            277 => Some(Self::Treecko), // SPECIES_TREECKO
            280 => Some(Self::Torchic), // SPECIES_TORCHIC
            283 => Some(Self::Mudkip),  // SPECIES_MUDKIP
            _ => None,
        }
    }
}

/// Which rival the player faces — the other axis, upstream's player-gender
/// check (`MALE`/`FEMALE`; the player and the rival are always opposite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rival {
    /// A female player's rival: `TRAINER_BRENDAN_ROUTE_103_*`.
    Brendan,
    /// A male player's rival: `TRAINER_MAY_ROUTE_103_*`.
    May,
}

impl Rival {
    /// The rival a player of `gender` faces (upstream: `Route103_EventScript_Rival`'s
    /// own `checkplayergender` — module docs; `data/maps/Route103/scripts.inc:19-21`)
    /// — always the *opposite* protagonist.
    ///
    /// `None` for [`engine::save::PlayerGender::Other`]: upstream's
    /// `checkplayergender` copies the raw `playerGender` byte into
    /// `VAR_RESULT` and the two `goto_if_eq`s (`MALE`/`FEMALE`) simply fall
    /// through to `end` for any other byte (`src/scrcmd.c:2014-2018`), so a
    /// save with an out-of-range gender byte starts no battle at all —
    /// the same no-op `crate::new_game::apply_truck_intro_flags` already
    /// documents for its own `checkplayergender` branch. Unreachable in
    /// production: [`crate::new_game::DEFAULT_PLAYER_GENDER`] is always
    /// `Male` or `Female`, and nothing else writes the field.
    #[must_use]
    pub const fn for_gender(gender: engine::save::PlayerGender) -> Option<Self> {
        match gender {
            engine::save::PlayerGender::Male => Some(Self::May),
            engine::save::PlayerGender::Female => Some(Self::Brendan),
            engine::save::PlayerGender::Other(_) => None,
        }
    }
}

/// The six `TRAINER_*_ROUTE_103_*` ids
/// (`pokeemerald/include/constants/opponents.h:524`-`:539`).
///
/// The suffix names the **player's** starter, not the rival's: the rival's
/// own party is the type-advantaged answer to it
/// (`src/data/trainer_parties.h:6784`, `:6828`, `:6872`, `:6916`, `:6960`,
/// `:7004` — Mudkip ⇒ a level-5 Treecko, Treecko ⇒ Torchic, Torchic ⇒
/// Mudkip), which is exactly the pairing this function encodes.
#[must_use]
pub const fn route103_rival_for(rival: Rival, starter: PlayerStarter) -> TrainerId {
    match (rival, starter) {
        (Rival::Brendan, PlayerStarter::Mudkip) => TrainerId(520),
        (Rival::Brendan, PlayerStarter::Treecko) => TrainerId(523),
        (Rival::Brendan, PlayerStarter::Torchic) => TrainerId(526),
        (Rival::May, PlayerStarter::Mudkip) => TrainerId(529),
        (Rival::May, PlayerStarter::Treecko) => TrainerId(532),
        (Rival::May, PlayerStarter::Torchic) => TrainerId(535),
    }
}

/// Why a Route 103 rival battle could not be constructed.
///
/// A concrete enum owned by this module rather than a reuse of
/// [`BattleError`] `(oop-boundaries)`: two of its three cases are
/// *construction*-layer facts (an un-encodable name, a party shape carrying
/// held items) that the `battle` crate has no vocabulary for, and both are
/// deliberately fatal rather than silently approximated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RivalBattleError {
    /// The `battle` crate rejected the party or the battle — see
    /// [`battle::Battle::new_trainer`]'s own error list.
    Battle(BattleError),
    /// A trainer or species name contains a character
    /// [`engine::text::char_to_byte`] has no Gen-3 charmap byte for, so
    /// `CreateNPCTrainerParty`'s `nameHash` cannot be computed faithfully.
    ///
    /// Unreachable for the extracted tables (a whole-table test in this
    /// module proves every `gTrainers[].trainerName` and every
    /// `gSpeciesNames[]` entry encodes), and fatal rather than skipped
    /// because a wrong hash is a wrong *personality*, hence a wrong nature,
    /// hence wrong stats.
    UnnamedCharacter {
        /// The name that could not be encoded.
        name: &'static str,
        /// The offending character.
        character: char,
    },
    /// The trainer's party carries held items
    /// (`F_TRAINER_PARTY_HELD_ITEM`), which [`battle::BattlePokemon`] cannot
    /// represent at all.
    ///
    /// No Route 103 rival is one (all six are `NO_ITEM_DEFAULT_MOVES`), and
    /// building the party anyway would silently drop a Sitrus Berry or an
    /// Oran Berry rather than fail — so this fails closed instead.
    HeldItemParty(TrainerId),
}

impl core::fmt::Display for RivalBattleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Battle(error) => write!(f, "{error}"),
            Self::UnnamedCharacter { name, character } => write!(
                f,
                "name `{name}` contains `{character}`, which has no Gen-3 charmap byte"
            ),
            Self::HeldItemParty(id) => write!(
                f,
                "trainer `{}` fields held items, which are not modelled",
                id.0
            ),
        }
    }
}

impl std::error::Error for RivalBattleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Battle(error) => Some(error),
            Self::UnnamedCharacter { .. } | Self::HeldItemParty(_) => None,
        }
    }
}

impl From<BattleError> for RivalBattleError {
    fn from(error: BattleError) -> Self {
        Self::Battle(error)
    }
}

/// `CreateNPCTrainerParty`'s three personality bases
/// (`battle_main.c:1993`-`:1998`), chosen by the trainer's own flags rather
/// than by anything about the mon: `0x80` for a double battle, `0x78` for a
/// female trainer ("use personality more likely to result in a female
/// Pokémon"), `0x88` otherwise.
#[must_use]
fn personality_base(trainer: &TrainerData) -> u32 {
    if trainer.double_battle {
        0x80
    } else if trainer.encounter_music.is_female {
        0x78
    } else {
        0x88
    }
}

/// Sum a name's Gen-3 charmap bytes into `hash`, upstream's
/// `for (j = 0; name[j] != EOS; j++) nameHash += name[j];`.
///
/// # Errors
///
/// [`RivalBattleError::UnnamedCharacter`] for a glyph outside
/// [`engine::text::char_to_byte`]'s table (module docs).
fn add_name_bytes(hash: &mut u32, name: &'static str) -> Result<(), RivalBattleError> {
    for character in name.chars() {
        let byte = engine::text::char_to_byte(character)
            .ok_or(RivalBattleError::UnnamedCharacter { name, character })?;
        *hash = hash.wrapping_add(u32::from(byte));
    }
    Ok(())
}

/// One party-table row, flattened out of [`TrainerParty`]'s four shapes into
/// the fields `CreateNPCTrainerParty` actually feeds `CreateMon`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PartyEntry {
    /// `partyData[i].species`.
    species: SpeciesId,
    /// `partyData[i].lvl`.
    level: u8,
    /// `partyData[i].iv`, before [`battle::fixed_ivs`]'s `* 31 / 255` scale.
    iv: u8,
    /// The fixed moveset the `F_TRAINER_PARTY_CUSTOM_MOVESET` shapes write
    /// over `CreateMon`'s result, or `None` for "leave
    /// `GiveBoxMonInitialMoveset`'s level-up moveset in place".
    moves: Option<Vec<MoveId>>,
}

/// Every member of `trainer`'s party, in `gTrainers[].party` order — the
/// shape-dependent half of `CreateNPCTrainerParty`'s per-mon work.
///
/// # Errors
///
/// [`RivalBattleError::HeldItemParty`] for the two `F_TRAINER_PARTY_HELD_ITEM`
/// shapes (see that variant).
fn party_entries(
    id: TrainerId,
    trainer: &TrainerData,
) -> Result<Vec<PartyEntry>, RivalBattleError> {
    let entries = match trainer.party {
        TrainerParty::NoItemDefaultMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                iv: mon.iv,
                moves: None,
            })
            .collect(),
        TrainerParty::NoItemCustomMoves(party) => party
            .iter()
            .map(|mon| PartyEntry {
                species: mon.species,
                level: mon.lvl,
                iv: mon.iv,
                // `MOVE_NONE` pads the fixed array upstream; a real moveset
                // is however many leading slots are filled.
                moves: Some(mon.moves.iter().copied().filter(|m| m.0 != 0).collect()),
            })
            .collect(),
        TrainerParty::ItemDefaultMoves(_) | TrainerParty::ItemCustomMoves(_) => {
            return Err(RivalBattleError::HeldItemParty(id))
        }
    };
    Ok(entries)
}

/// The personality `CreateNPCTrainerParty` seeds each of `trainer`'s party
/// members with, in party order — the accumulator quirk and the charmap
/// encoding of the module docs, and **no RNG at all**.
///
/// Exposed (rather than folded into [`start_route103_rival_battle`]) because
/// it is the part worth pinning on its own: a personality decides a mon's
/// nature, so this one arithmetic chain decides whether the rival's Treecko
/// is Adamant or Bold.
///
/// # Errors
///
/// [`RivalBattleError::UnnamedCharacter`] or
/// [`RivalBattleError::HeldItemParty`] — see each.
pub fn trainer_party_personalities(id: TrainerId) -> Result<Vec<u32>, RivalBattleError> {
    let trainer = battle::trainer_data(id)?;
    let entries = party_entries(id, trainer)?;
    let names = SpeciesNames::new();
    let base = personality_base(trainer);

    // Declared once for the whole party, exactly as upstream's `u32
    // nameHash = 0;` is -- see the module docs' first quirk.
    let mut name_hash: u32 = 0;
    let mut personalities = Vec::with_capacity(entries.len());
    for entry in &entries {
        add_name_bytes(&mut name_hash, trainer.name)?;
        let species_name = names
            .name(entry.species)
            .map_err(|_| RivalBattleError::Battle(BattleError::UnknownSpecies(entry.species)))?;
        add_name_bytes(&mut name_hash, species_name)?;
        personalities.push(base.wrapping_add(name_hash.wrapping_shl(8)));
    }
    Ok(personalities)
}

/// Build `trainer`'s whole party (`CreateNPCTrainerParty`) and start the
/// `BATTLE_TYPE_TRAINER` battle around it — the trainer-battle counterpart
/// of [`crate::flow::first_battle::start_first_battle`].
///
/// Draws in upstream's order off the shared stream (module docs, "RNG
/// stream"): each party member's OT id, then
/// [`battle::Battle::new_trainer`]'s turn-number draw and its conditional
/// speed-tie draw. The personality and IVs draw nothing — they are the
/// seeded/fixed values above — and a moveset the party table leaves default
/// comes from [`battle::initial_moveset`] (`GiveBoxMonInitialMoveset`, which
/// also draws nothing).
///
/// # Errors
///
/// [`RivalBattleError`]'s three cases. Every construction failure is raised
/// **before** the first draw except the party build itself, which is
/// upstream's own order (a party is built mon by mon, each drawing its OT
/// id as it goes); the `battle`-crate screens that could reject a party
/// wholesale all run inside [`battle::Battle::new_trainer`], after it. That
/// is a knowing difference from [`crate::flow::first_battle`]'s single-mon
/// construction and costs nothing in practice: the six Route 103 rivals are
/// all screened green by this module's own tests.
pub fn start_route103_rival_battle(
    player_lead: BattlePokemon,
    trainer: TrainerId,
    rng: &mut Rng,
) -> Result<Battle, RivalBattleError> {
    let dex = Dex::new();
    let data = battle::trainer_data(trainer)?;
    let entries = party_entries(trainer, data)?;
    let personalities = trainer_party_personalities(trainer)?;

    let mut party = Vec::with_capacity(entries.len());
    for (entry, personality) in entries.into_iter().zip(personalities) {
        let moves = entry
            .moves
            .unwrap_or_else(|| battle::initial_moveset(entry.species, entry.level));
        party.push(battle::build_trainer_pokemon(
            &dex,
            entry.species,
            entry.level,
            battle::fixed_ivs(entry.iv),
            personality,
            moves,
            &mut SharedRng::new(rng),
        )?);
    }

    Ok(Battle::new_trainer(
        dex,
        player_lead,
        trainer,
        party,
        &mut SharedRng::new(rng),
    )?)
}

/// Play one turn of the in-progress rival battle in `slot`, headlessly. A
/// no-op if `slot` is empty.
///
/// Mirrors [`crate::flow::first_battle::advance_first_battle`]'s shape —
/// turn, write-back, neutral stat-stage reset, error-ends-the-battle-too —
/// and for the same reason picks [`PlayerAction::UseMove`]`(0)` every turn
/// rather than [`PlayerAction::Run`]: a trainer battle refuses Run outright
/// ([`BattleError::NoRunningFromTrainer`]), so reusing
/// `wild_encounter::advance_wild_battle`'s policy here would end the battle
/// on turn one instead of playing it. The action is a stand-in for the
/// player's choice, and only the action: every rule it exercises — turn
/// order, the trainer AI's real `AI_SCRIPT_*` scoring, damage, fainting, the
/// forced post-faint send-out, exp and prize money — is the real
/// [`battle::Battle`].
///
/// The write-back is unconditional, fainted lead included — this function's
/// own job stops at "the battle ended, here is who's left standing," the
/// same as [`crate::flow::wild_encounter::advance_wild_battle`]. Losing a
/// trainer battle upstream routes through `CB2_EndTrainerBattle`'s
/// `IsPlayerDefeated` check (`src/battle_setup.c:1327`-`:1338`) to
/// `CB2_WhiteOut`, which halves the player's money, heals the party and
/// warps to the last heal location — this function's own caller
/// ([`crate::flow::overworld_phase::route103_rival_trigger::OverworldPhase::advance_route103_rival_battle_frame`])
/// is what now runs that (issue #261,
/// [`crate::flow::overworld_phase::white_out::OverworldPhase::white_out`]),
/// the instant it sees [`BattleOutcome::PlayerLost`] here — so the fainted
/// lead this function hands back is only ever momentarily fainted in
/// practice, not a standing gap.
///
/// [`battle::BattleEvent::MoneyGained`] is still reported and dropped: this
/// issue only wires `engine::save::SaveBlock1::money` up to be *halved* on a
/// loss (issue #261's own scope) — awarding prize money on a *win* is a
/// separate, still-open gap. The event is returned to the caller through
/// [`battle::Battle::take_turn`] regardless, so a later slice that wires up
/// a wallet credit has the amount already computed at the right point in the
/// battle.
///
/// A `None` return is intentionally ambiguous: it can mean that `slot` was
/// already empty, that the battle remains ongoing, or that a failed turn
/// ended the battle and cleared `slot`. Callers must inspect `slot` after the
/// call to determine whether a battle is still active.
pub fn advance_route103_rival_battle(
    slot: &mut Option<Battle>,
    lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = slot.as_mut()?;
    let failed = match battle.take_turn(PlayerAction::UseMove(0), &mut SharedRng::new(rng)) {
        Ok(_) => false,
        Err(error) => {
            eprintln!("route 103 rival battle: turn failed ({error:?}) -- ending the battle");
            true
        }
    };
    let outcome = battle.outcome();
    if !failed && outcome.is_none() {
        return None;
    }
    let mut mon = battle.player().clone();
    // Stat stages live in `gBattleMons[].statStages` only and never reach
    // the party struct -- see `advance_first_battle`'s own doc comment for
    // the citations.
    *mon.stages_mut() = StatStages::default();
    *lead = Some(mon);
    *slot = None;
    outcome
}

#[cfg(test)]
mod tests;
