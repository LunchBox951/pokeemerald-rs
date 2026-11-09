//! The pre-PP obedience gate for traded Pokémon
//! (`IsMonDisobedient`, `pokeemerald/src/battle_util.c:3923`-`:4038`, called
//! from `Cmd_attackcanceler`, `src/battle_script_commands.c:944`-`:957`).
//!
//! Modelled: the ownership and badge exemptions, the first `Random() & 255`
//! draw, the second draw that picks the alternate-move branch, that
//! branch's `CheckMoveLimitations` mask and `Random() % 4` slot sampling,
//! the alternate move's `GetMoveTarget` draws, the third draw of the
//! nap/self-hit/loaf branch, the nap, the self-hit and the loaf-message
//! draw. Refused with [`BattleError::UnportedDisobedience`] before any draw:
//! a traded Mew or Deoxys above its limit (upstream's fateful-encounter
//! bypass, `battle_util.c:3913`-`:3934`, needs record data the engine
//! lacks). Unreachable because the engine has no Rage or Uproar state: Rage
//! clearing and the Uproar part of the nap check. Not modelled: the
//! already-asleep Snore/Sleep Talk ignore (`:3968`-`:3972`), moot while
//! neither move has an effect pipeline (`EFFECT_SNORE`, `EFFECT_SLEEP_TALK`
//! are unported), so an action selecting one fails as unported anyway.

use assets::{AbilityId, MoveTarget, SpeciesId};

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};
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
    /// `BattleScript_IgnoresAndUsesRandomMove`: the move in `slot` runs in
    /// place of the chosen one, with `HITMARKER_OBEYS` already set.
    UsesInstead {
        slot: usize,
    },
    /// `BattleScript_IgnoresAndFallsAsleep`: the sleep itself is applied by
    /// the caller, whose `SetMoveEffect` draw follows this gate's draws.
    Naps,
    HurtsItself,
}

/// `gBitTable[slot]` over `MAX_MON_MOVES` slots; `ALL_MOVES_MASK` is every
/// bit set.
const ALL_MOVES_MASK: u8 = 0b1111;

/// `CheckMoveLimitations(battler, gBitTable[chosenSlot], MOVE_LIMITATIONS_ALL)`
/// (`battle_util.c:1082`-`:1123`) over the limitations the engine has state
/// for: the chosen slot itself, an empty slot (`MOVE_LIMITATION_ZEROMOVE`)
/// and a slot with no PP (`MOVE_LIMITATION_PP`). Disable, Torment, Taunt,
/// Imprison and Encore are unmodelled volatiles and Choice Band is refused
/// at admission, so none of them can mark a slot here. `chosen_slot` is
/// `None` for a forced Struggle, which has no slot of its own.
fn unusable_slots(mon: &BattlePokemon, chosen_slot: Option<usize>) -> u8 {
    let mut mask = chosen_slot.map_or(0, |slot| 1 << slot);
    for slot in 0..MAX_MON_MOVES {
        if mon.moves().get(slot).is_none_or(|known| known.pp == 0) {
            mask |= 1 << slot;
        }
    }
    mask
}

/// `GetMoveTarget(move, NO_TARGET_OVERRIDE)` for the player's battler in a
/// single battle (`battle_util.c:3836`-`:3910`): `MOVE_TARGET_SELECTED`
/// samples `Random() % gBattlersCount` until the result is not the attacker
/// (`:3851`-`:3853`), so with two battlers and the player at index 0 every
/// even draw is rejected. Every other target kind resolves without a draw
/// in singles, including `MOVE_TARGET_RANDOM` (`:3882`-`:3902`). Follow Me
/// (`followmeTimer`) is unmodelled and the Lightning Rod redirect (`:3854`-
/// `:3861`) cannot flip in singles because the lone opponent is the holder.
pub(crate) fn draw_move_target(target: MoveTarget, rng: &mut impl BattleRng) {
    if target != MoveTarget::SELECTED {
        return;
    }
    while rng.next_u16() % BATTLERS_IN_SINGLES == PLAYER_BATTLER_INDEX {}
}

/// `MAX_MON_MOVES` as the modulus of the slot draw.
const MAX_MON_MOVES_DRAW: u16 = 4;
/// `gBattlersCount` in a single battle.
const BATTLERS_IN_SINGLES: u16 = 2;
/// `gBattlerAttacker` for the player's battler.
const PLAYER_BATTLER_INDEX: u16 = 0;

/// Runs `IsMonDisobedient` for the player's active Pokémon, whose chosen
/// action sits in `chosen_slot` (`gCurrMovePos`; `None` for a forced
/// Struggle).
pub(crate) fn check(
    player: &PlayerObedience,
    mon: &BattlePokemon,
    chosen_slot: Option<usize>,
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
    // Upstream tests the Mew/Deoxys guard first (`battle_util.c:3913-3934`):
    // a traded one above the limit disobeys unless it carries the modern
    // fateful-encounter bit, which skips every exemption above. This port
    // keeps the guard after the ownership, badge and level exemptions on
    // purpose and does not model the bit. Every legitimate upstream source of
    // a Mew or Deoxys sets it (`seteventmon` -> `CreateEventMon`,
    // `pokemon.c:2633-2639`) and trades refuse one without it
    // (`trade.c:1577-1582`), so a guard-first order with no bit to read would
    // refuse every legitimately caught Mew. A domestic Mew therefore obeys.
    if matches!(mon.species(), MEW | DEOXYS) {
        return Err(BattleError::UnportedDisobedience);
    }
    let first = u32::from(rng.next_u16() & 255);
    if ((level + limit) * first) >> 8 < limit {
        return Ok(Obedience::Obeys);
    }
    let second = u32::from(rng.next_u16() & 255);
    if ((level + limit) * second) >> 8 < limit {
        // `battle_util.c:3975`-`:3998`: every other slot unusable loafs;
        // otherwise `MOD(Random(), MAX_MON_MOVES)` is drawn until it lands
        // on a usable slot, each draw counted.
        let unusable = unusable_slots(mon, chosen_slot);
        if unusable == ALL_MOVES_MASK {
            return Ok(Obedience::Loafs(LoafMessage::from_roll(rng.next_u16())));
        }
        let slot = loop {
            let slot = usize::from(rng.next_u16() % MAX_MON_MOVES_DRAW);
            if unusable & (1 << slot) == 0 {
                break slot;
            }
        };
        return Ok(Obedience::UsesInstead { slot });
    }
    let excess = level - limit;
    let third = u32::from(rng.next_u16() & 255);
    // The Uproar scan (`battle_util.c:4009`-`:4013`) finds nothing: the
    // engine has no Uproar state, so a nap is never blocked by it.
    if third < excess
        && mon.status1() == Status1::Healthy
        && mon.ability() != INSOMNIA
        && mon.ability() != VITAL_SPIRIT
    {
        return Ok(Obedience::Naps);
    }
    // Signed in upstream: a nap blocked by status or ability goes negative
    // and so falls into the self-hit.
    if i64::from(third) - i64::from(excess) < i64::from(excess) {
        return Ok(Obedience::HurtsItself);
    }
    Ok(Obedience::Loafs(LoafMessage::from_roll(rng.next_u16())))
}
