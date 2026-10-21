//! The pre-PP obedience gate for traded Pokémon
//! (`IsMonDisobedient`, `pokeemerald/src/battle_util.c:3923`-`:4038`, called
//! from `Cmd_attackcanceler`, `src/battle_script_commands.c:944`-`:957`).
//!
//! Modelled: the ownership and badge exemptions, the first `Random() & 255`
//! draw, the second draw that picks the alternate-move branch, the third
//! draw of the nap/self-hit/loaf branch, the loaf-message draw, and the
//! self-hit. Refused with [`BattleError::UnportedDisobedience`]: the nap, the
//! alternate-move branch (slot rejection sampling and target draws), and a
//! Mew or Deoxys above its limit (upstream's fateful-encounter bypass,
//! `battle_util.c:3913`-`:3934`, needs record data the engine lacks; refused
//! before any draw). Unreachable because the engine has no sleep or Uproar
//! state: the already-asleep Snore/Sleep Talk branch, Rage clearing and the
//! Uproar part of the nap check.

use assets::{AbilityId, SpeciesId};

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::status1::Status1;

/// Game-text string terminator (`EOS`).
pub const TEXT_EOS: u8 = 0xFF;
/// Length of the player-name buffer, terminator included
/// (`PLAYER_NAME_LENGTH + 1`).
pub const PLAYER_NAME_BUF_LEN: usize = 8;

/// `ABILITY_INSOMNIA` and `ABILITY_VITAL_SPIRIT` block the nap outcome.
/// `SPECIES_MEW` and `SPECIES_DEOXYS`.
const MEW: SpeciesId = SpeciesId(151);
const DEOXYS: SpeciesId = SpeciesId(410);
const INSOMNIA: AbilityId = AbilityId(15);
const VITAL_SPIRIT: AbilityId = AbilityId(72);

/// The player's identity and badge-derived obedience limit, supplied by the
/// flow at battle start (`gSaveBlock2Ptr->playerTrainerId`/`playerName` and
/// `FLAG_BADGE02/04/06/08_GET`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerObedience {
    trainer_id: u32,
    trainer_name: [u8; PLAYER_NAME_BUF_LEN],
    /// `None` once badge 8 is held: every Pokémon obeys.
    limit: Option<u8>,
}

impl PlayerObedience {
    /// `badges` is `[badge 2, badge 4, badge 6, badge 8]`; badges 1, 3, 5 and
    /// 7 do not affect obedience (`battle_util.c:3943`-`:3955`).
    #[must_use]
    pub const fn new(
        trainer_id: u32,
        trainer_name: [u8; PLAYER_NAME_BUF_LEN],
        badges: [bool; 4],
    ) -> Self {
        let limit = if badges[3] {
            None
        } else if badges[2] {
            Some(70)
        } else if badges[1] {
            Some(50)
        } else if badges[0] {
            Some(30)
        } else {
            Some(10)
        };
        Self {
            trainer_id,
            trainer_name,
            limit,
        }
    }

    /// `IsOtherTrainer` (`pokemon.c:6611`-`:6626`): the full id, then the
    /// name bytes up to the Pokémon's terminator.
    fn is_other_trainer(&self, mon: &BattlePokemon) -> bool {
        if mon.original_trainer_id() != self.trainer_id {
            return true;
        }
        // An unrecorded OT name is compared by id alone.
        let Some(name) = mon.original_trainer_name() else {
            return false;
        };
        for (index, byte) in name.iter().enumerate() {
            if *byte == TEXT_EOS {
                return false;
            }
            if *byte != self.trainer_name[index] {
                return true;
            }
        }
        false
    }
}

/// The text a loafing Pokémon prints (`sATypeMessages`-style table at
/// `battle_message.c:1176`-`:1182`; the draw picks among the first four).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoafMessage {
    /// "is loafing around!"
    Loafing,
    /// "won't obey!"
    WontObey,
    /// "turned away!"
    TurnedAway,
    /// "pretended not to notice!"
    PretendedNotToNotice,
}

impl LoafMessage {
    const fn from_roll(roll: u16) -> Self {
        match roll % 4 {
            0 => Self::Loafing,
            1 => Self::WontObey,
            2 => Self::TurnedAway,
            _ => Self::PretendedNotToNotice,
        }
    }
}

/// What the gate decided for one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Obedience {
    Obeys,
    Loafs(LoafMessage),
    HurtsItself,
}

/// Runs `IsMonDisobedient` for the player's active Pokémon.
pub(crate) fn check(
    player: &PlayerObedience,
    mon: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<Obedience, BattleError> {
    if !player.is_other_trainer(mon) {
        return Ok(Obedience::Obeys);
    }
    let Some(limit) = player.limit else {
        return Ok(Obedience::Obeys);
    };
    let level = u32::from(mon.level());
    let limit = u32::from(limit);
    if level <= limit {
        return Ok(Obedience::Obeys);
    }
    if matches!(mon.species(), MEW | DEOXYS) {
        return Err(BattleError::UnportedDisobedience);
    }
    let first = u32::from(rng.next_u16() & 255);
    if ((level + limit) * first) >> 8 < limit {
        return Ok(Obedience::Obeys);
    }
    let second = u32::from(rng.next_u16() & 255);
    if ((level + limit) * second) >> 8 < limit {
        return Err(BattleError::UnportedDisobedience);
    }
    let excess = level - limit;
    let third = u32::from(rng.next_u16() & 255);
    if third < excess
        && mon.status1() == Status1::Healthy
        && mon.ability() != INSOMNIA
        && mon.ability() != VITAL_SPIRIT
    {
        return Err(BattleError::UnportedDisobedience);
    }
    // Signed in upstream: a nap blocked by status or ability goes negative
    // and so falls into the self-hit.
    if i64::from(third) - i64::from(excess) < i64::from(excess) {
        return Ok(Obedience::HurtsItself);
    }
    Ok(Obedience::Loafs(LoafMessage::from_roll(rng.next_u16())))
}
