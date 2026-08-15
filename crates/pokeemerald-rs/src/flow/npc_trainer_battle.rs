//! Generic NPC trainer-battle construction and headless driver -- the shared
//! home `CreateNPCTrainerParty`'s own port lives in, factored out of
//! [`crate::flow::route103_rival`] (issue #237, S-6) when issue #264 (I-5
//! follow-up) needed the exact same construction for Route 103's *sight*
//! trainers, not just its scripted rival `(oop-boundaries)`: the party-build
//! quirks below have nothing to do with *which* trainer is fighting, so
//! duplicating them per caller would drift the two paths apart the moment
//! either one changed. [`crate::flow::route103_rival`] keeps its own
//! `PlayerStarter`/`Rival`/`route103_rival_for` mapping (the rival-specific
//! *choice* of trainer) and re-exports this module's construction/driver
//! under its original names, so nothing about that module's own public API
//! or tests moved.
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
//!    closed ([`NpcTrainerBattleError::UnnamedCharacter`]) on a glyph it does
//!    not cover.
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
//! = Random()` (`battle_main.c:3140`). So [`start_npc_trainer_battle`] draws
//! **two per party member** (one `Random32` OT id each, barring the
//! `8/65536` shiny retry) and then hands the stream to
//! [`battle::Battle::new_trainer`] for its turn-number draw and conditional
//! speed-tie draw.
//!
//! Unlike the first battle's construction, there is **no** missing
//! `SetWildMonHeldItem` draw to account for here: its gate excludes
//! `BATTLE_TYPE_TRAINER` outright (`battle_main.c:700`, `src/pokemon.c:6682`),
//! so upstream does not spend it either.
//!
//! # Nothing is built before the whole party is screened
//!
//! Upstream never *refuses* a trainer battle, so every refusal this port
//! makes has to cost nothing: the draws have no upstream counterpart, and
//! [`start_npc_trainer_battle`]'s hottest caller
//! ([`crate::flow::overworld_phase::sight_trainer_trigger`]) asks again on
//! **every frame** the player stands in a sight cone, with no button gate to
//! slow it down. Building the party mon by mon and letting
//! [`battle::Battle::new_trainer`]'s own screens reject it afterwards
//! therefore leaked two draws per party member per frame (issue #264
//! review) -- a real, observable corruption of the one shared stream every
//! wild encounter and battle turn shares.
//!
//! So [`start_npc_trainer_battle`] resolves each member's moveset (which
//! draws nothing) and runs [`battle::ensure_trainer_party_startable`] --
//! the same screens `new_trainer` applies, composed into a pre-flight that
//! takes no RNG at all -- **before** the first
//! [`battle::build_trainer_pokemon`] call. A trainer this engine cannot
//! fight is refused for free, forever, the same no-draw-at-all shape
//! [`crate::flow::wild_encounter::map_wild_table_fightable`] already applies
//! to wild tables via [`battle::ensure_wild_startable`].
//!
//! # Held-item parties: a fail-closed gap, not a silent drop
//!
//! Upstream's `party` field is a `union TrainerMonPtr` that can carry a
//! per-mon held item (`F_TRAINER_PARTY_HELD_ITEM`) alongside the fixed-vs-
//! custom moveset axis; [`battle::BattlePokemon`] has no held-item concept at
//! all yet. Building such a party anyway would silently drop a Sitrus Berry
//! or an Oran Berry rather than fail, so [`party_entries`] refuses instead
//! ([`NpcTrainerBattleError::HeldItemParty`]) — the same fail-closed posture
//! this module's every other unmodellable input takes. Reachable in real
//! play as of issue #264: `TRAINER_MIGUEL_1` (Route 103's own Miguel,
//! `crate::flow::overworld_phase::sight_trainer_trigger`) carries
//! `TrainerParty::ItemDefaultMoves`, so his sight cone finds him, but his
//! battle refuses to construct — recorded on that module's own docs and the
//! ledger, not papered over.

use assets::trainers::{TrainerData, TrainerId, TrainerParty};
use assets::{MoveId, SpeciesNames};
use battle::{Battle, BattleError, BattleOutcome, BattlePokemon, Dex, PlayerAction, StatStages};
use engine::rng::Rng;

use super::wild_encounter::SharedRng;

/// Why an NPC trainer battle could not be constructed.
///
/// A concrete enum owned by this module rather than a reuse of
/// [`BattleError`] `(oop-boundaries)`: two of its three cases are
/// *construction*-layer facts (an un-encodable name, a party shape carrying
/// held items) that the `battle` crate has no vocabulary for, and both are
/// deliberately fatal rather than silently approximated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NpcTrainerBattleError {
    /// The `battle` crate rejected the party or the battle — see
    /// [`battle::Battle::new_trainer`]'s own error list.
    Battle(BattleError),
    /// A trainer or species name contains a character
    /// [`engine::text::char_to_byte`] has no Gen-3 charmap byte for, so
    /// `CreateNPCTrainerParty`'s `nameHash` cannot be computed faithfully.
    ///
    /// Unreachable for the extracted tables (a whole-table test in
    /// [`crate::flow::route103_rival`] proves every `gTrainers[].trainerName`
    /// and every `gSpeciesNames[]` entry encodes), and fatal rather than
    /// skipped because a wrong hash is a wrong *personality*, hence a wrong
    /// nature, hence wrong stats.
    UnnamedCharacter {
        /// The name that could not be encoded.
        name: &'static str,
        /// The offending character.
        character: char,
    },
    /// The trainer's party carries held items
    /// (`F_TRAINER_PARTY_HELD_ITEM`), which [`battle::BattlePokemon`] cannot
    /// represent at all (module docs' "Held-item parties" section).
    HeldItemParty(TrainerId),
}

impl core::fmt::Display for NpcTrainerBattleError {
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

impl std::error::Error for NpcTrainerBattleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Battle(error) => Some(error),
            Self::UnnamedCharacter { .. } | Self::HeldItemParty(_) => None,
        }
    }
}

impl From<BattleError> for NpcTrainerBattleError {
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
/// [`NpcTrainerBattleError::UnnamedCharacter`] for a glyph outside
/// [`engine::text::char_to_byte`]'s table (module docs).
fn add_name_bytes(hash: &mut u32, name: &'static str) -> Result<(), NpcTrainerBattleError> {
    for character in name.chars() {
        let byte = engine::text::char_to_byte(character)
            .ok_or(NpcTrainerBattleError::UnnamedCharacter { name, character })?;
        *hash = hash.wrapping_add(u32::from(byte));
    }
    Ok(())
}

/// One party-table row, flattened out of [`TrainerParty`]'s four shapes into
/// the fields `CreateNPCTrainerParty` actually feeds `CreateMon`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PartyEntry {
    /// `partyData[i].species`.
    species: assets::SpeciesId,
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
/// [`NpcTrainerBattleError::HeldItemParty`] for the two
/// `F_TRAINER_PARTY_HELD_ITEM` shapes (see that variant).
fn party_entries(
    id: TrainerId,
    trainer: &TrainerData,
) -> Result<Vec<PartyEntry>, NpcTrainerBattleError> {
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
            return Err(NpcTrainerBattleError::HeldItemParty(id))
        }
    };
    Ok(entries)
}

/// The personality `CreateNPCTrainerParty` seeds each of `trainer`'s party
/// members with, in party order — the accumulator quirk and the charmap
/// encoding of the module docs, and **no RNG at all**.
///
/// Exposed (rather than folded into [`start_npc_trainer_battle`]) because it
/// is the part worth pinning on its own: a personality decides a mon's
/// nature, so this one arithmetic chain decides whether a rival's Treecko is
/// Adamant or Bold.
///
/// # Errors
///
/// [`NpcTrainerBattleError::UnnamedCharacter`] or
/// [`NpcTrainerBattleError::HeldItemParty`] — see each.
pub fn trainer_party_personalities(id: TrainerId) -> Result<Vec<u32>, NpcTrainerBattleError> {
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
        let species_name = names.name(entry.species).map_err(|_| {
            NpcTrainerBattleError::Battle(BattleError::UnknownSpecies(entry.species))
        })?;
        add_name_bytes(&mut name_hash, species_name)?;
        personalities.push(base.wrapping_add(name_hash.wrapping_shl(8)));
    }
    Ok(personalities)
}

/// Build `trainer`'s whole party (`CreateNPCTrainerParty`) and start the
/// `BATTLE_TYPE_TRAINER` battle around it — the trainer-battle counterpart of
/// [`crate::flow::first_battle::start_first_battle`], and the shared core
/// behind both [`crate::flow::route103_rival::start_route103_rival_battle`]
/// and [`crate::flow::overworld_phase::sight_trainer_trigger`]'s own battle
/// handoff.
///
/// Draws in upstream's order off the shared stream (module docs, "RNG
/// stream"): each party member's OT id, then
/// [`battle::Battle::new_trainer`]'s turn-number draw and its conditional
/// speed-tie draw. The personality and IVs draw nothing — they are the
/// seeded/fixed values above — and a moveset the party table leaves default
/// comes from [`battle::initial_moveset`] (`GiveBoxMonInitialMoveset`, which
/// also draws nothing).
///
/// A refusal draws **nothing at all**: the whole party is screened by
/// [`battle::ensure_trainer_party_startable`] ahead of the first
/// [`battle::build_trainer_pokemon`] call (module docs, "Nothing is built
/// before the whole party is screened").
///
/// # Errors
///
/// [`NpcTrainerBattleError`]'s construction-refusal cases are raised
/// **before the first draw** (module docs, "Nothing is built before the
/// whole party is screened"): a refused party leaves `rng` exactly as it
/// found it, however many times it is asked. The one exception is
/// [`battle::BattleError::FaintedBattler`] for a fainted `player_lead`,
/// which [`battle::Battle::new_trainer`] raises only *after* the party
/// build's OT-id draws — the pre-flight takes no player argument and cannot
/// screen it. Both in-tree callers check the lead before calling here;
/// a future caller must do the same or accept the spent draws.
pub fn start_npc_trainer_battle(
    player_lead: BattlePokemon,
    trainer: TrainerId,
    rng: &mut Rng,
) -> Result<Battle, NpcTrainerBattleError> {
    let dex = Dex::new();
    let data = battle::trainer_data(trainer)?;
    let entries = party_entries(trainer, data)?;
    let personalities = trainer_party_personalities(trainer)?;

    // Resolve each member's real moveset first (`GiveBoxMonInitialMoveset`
    // draws nothing), then screen the whole party -- ahead of the first
    // `build_trainer_pokemon` call, which would draw an OT id.
    let movesets: Vec<Vec<MoveId>> = entries
        .iter()
        .map(|entry| {
            entry
                .moves
                .clone()
                .unwrap_or_else(|| battle::initial_moveset(entry.species, entry.level))
        })
        .collect();
    let specs: Vec<battle::TrainerPartyMon<'_>> = entries
        .iter()
        .zip(&movesets)
        .map(|(entry, moves)| battle::TrainerPartyMon {
            species: entry.species,
            level: entry.level,
            moves,
        })
        .collect();
    battle::ensure_trainer_party_startable(&dex, trainer, &specs)?;

    let mut party = Vec::with_capacity(entries.len());
    for ((entry, personality), moves) in entries.iter().zip(personalities).zip(movesets) {
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

/// Play one turn of the in-progress trainer battle in `slot`, headlessly. A
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
/// warps to the last heal location — this function's own callers are what
/// run that (`crate::flow::overworld_phase::white_out::OverworldPhase::white_out`),
/// the instant they see [`BattleOutcome::PlayerLost`] here — so the fainted
/// lead this function hands back is only ever momentarily fainted in
/// practice, not a standing gap.
///
/// [`battle::BattleEvent::MoneyGained`] is still reported and dropped: this
/// port only wires `engine::save::SaveBlock1::money` up to be *halved* on a
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
pub fn advance_npc_trainer_battle(
    slot: &mut Option<Battle>,
    lead: &mut Option<BattlePokemon>,
    rng: &mut Rng,
) -> Option<BattleOutcome> {
    let battle = slot.as_mut()?;
    let failed = match battle.take_turn(PlayerAction::UseMove(0), &mut SharedRng::new(rng)) {
        Ok(_) => false,
        Err(error) => {
            eprintln!("npc trainer battle: turn failed ({error:?}) -- ending the battle");
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
