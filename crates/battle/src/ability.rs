//! The ability interactions issue #293's four sight-trainer battles make
//! reachable, and nothing more.
//!
//! Route 103's seeded parties field a Clear Body Tentacool (Andrew, Pete),
//! a Thick Fat Makuhita (Rhett) and a Soundproof Voltorb (Marcos) — every
//! one deterministic, because `CreateNPCTrainerParty` builds a non-female
//! single-battle trainer mon from `personalityValue = 0x88 + (nameHash <<
//! 8)` (`src/battle_main.c:1993`-`:1998`), whose bit 0 is always `0`, and
//! [`crate::pokemon::BattlePokemon::ability`] resolves an even personality
//! to ability slot 0. Three interactions follow, each modelled where
//! upstream runs it:
//!
//! - **Stat-drop immunities** (`ChangeStatBuffs`,
//!   `src/battle_script_commands.c:6987`-`:7038`): Clear Body and White
//!   Smoke block every opponent-inflicted drop, Keen Eye blocks accuracy
//!   drops, Hyper Cutter blocks Attack drops, and Shield Dust blocks the
//!   *silent* (`flags == 0`) secondary-effect drops only —
//!   [`stat_drop_blocker`].
//! - **Soundproof** (`AbilityBattleEffects`'s `ABILITYEFFECT_MOVES_BLOCK`,
//!   `src/battle_util.c:2659`-`:2675`, called from `Cmd_attackcanceler` at
//!   `src/battle_script_commands.c:932`): cancels a sound move outright,
//!   after the status cancellers and before the no-PP test —
//!   [`SOUND_MOVES`], [`soundproof_blocks`].
//! - **Thick Fat** (`CalculateBaseDamage`, `src/pokemon.c:3202`-`:3203`):
//!   halves the special stat against Fire/Ice moves —
//!   [`crate::damage::DamageInput`]'s `defender_thick_fat`.
//!
//! # Not modelled
//!
//! Every other ability, unchanged from before this slice. In particular the
//! pinch boosts the same parties carry — Overgrow, Blaze and Torrent on the
//! rival battles' starters, Swift Swim on Andrew's Magikarp — stay
//! unmodelled because no modelled battler can trigger them: the pinch
//! boosts key off typed damaging moves their level 5-15 movesets do not
//! include (`CalculateBaseDamage`'s `1/3`-HP gates at
//! `src/pokemon.c:3219`-`:3226` boost a matching-*type* move's power, and
//! Pound/Tackle/Scratch/Splash match none), and Swift Swim needs
//! weather no modelled battle can set. Intimidate, Static, Trace and the
//! rest of the on-switch/on-contact family are similarly unreachable for
//! these parties (slot 0 is deterministic, and no fielded slot-0 ability is
//! in that family). A future slice fielding new parties must re-argue this,
//! not inherit it.

use assets::species::AbilityId;
use assets::MoveId;

/// `ABILITY_SHIELD_DUST` (`include/constants/abilities.h:23`).
pub const SHIELD_DUST: AbilityId = AbilityId(19);
/// `ABILITY_CLEAR_BODY` (`:33`).
pub const CLEAR_BODY: AbilityId = AbilityId(29);
/// `ABILITY_SOUNDPROOF` (`:47`).
pub const SOUNDPROOF: AbilityId = AbilityId(43);
/// `ABILITY_THICK_FAT` (`:51`).
pub const THICK_FAT: AbilityId = AbilityId(47);
/// `ABILITY_KEEN_EYE` (`:55`).
pub const KEEN_EYE: AbilityId = AbilityId(51);
/// `ABILITY_HYPER_CUTTER` (`:56`).
pub const HYPER_CUTTER: AbilityId = AbilityId(52);
/// `ABILITY_WHITE_SMOKE` (`:77`).
pub const WHITE_SMOKE: AbilityId = AbilityId(73);

/// `sSoundMovesTable` (`src/battle_util.c:688`-`:692`), transcribed whole:
/// Growl, Roar, Sing, Supersonic, Screech, Snore, Uproar, Metal Sound,
/// Grass Whistle, Hyper Voice. Only Growl, Supersonic and Screech are
/// executable this slice, but the block is a table membership test, so the
/// table is carried complete rather than narrowed.
pub const SOUND_MOVES: [MoveId; 10] = [
    MoveId(45),  // MOVE_GROWL
    MoveId(46),  // MOVE_ROAR
    MoveId(47),  // MOVE_SING
    MoveId(48),  // MOVE_SUPERSONIC
    MoveId(103), // MOVE_SCREECH
    MoveId(173), // MOVE_SNORE
    MoveId(253), // MOVE_UPROAR
    MoveId(319), // MOVE_METAL_SOUND
    MoveId(320), // MOVE_GRASS_WHISTLE
    MoveId(304), // MOVE_HYPER_VOICE
];

/// `AbilityBattleEffects`' `ABILITYEFFECT_MOVES_BLOCK` case for the
/// target's Soundproof (`src/battle_util.c:2659`-`:2675`): a sound move
/// into a Soundproof battler is cancelled. Zero draws — the case is a pair
/// of table lookups.
#[must_use]
pub fn soundproof_blocks(target_ability: AbilityId, move_id: MoveId) -> bool {
    target_ability == SOUNDPROOF && SOUND_MOVES.contains(&move_id)
}

/// `ChangeStatBuffs`' target-ability guards against an opponent-inflicted
/// stat drop (`src/battle_script_commands.c:6987`-`:7038`), in upstream's
/// test order: Clear Body / White Smoke block every drop, Keen Eye blocks
/// `STAT_ACC`, Hyper Cutter blocks `STAT_ATK`, and Shield Dust blocks only
/// the message-less `flags == 0` calls — which is exactly the secondary-
/// effect path (`SetMoveEffect`'s `MOVE_EFFECT_*_MINUS_1` group,
/// `:2672`-`:2674`); the stat-move path passes `STAT_CHANGE_ALLOW_PTR`.
/// Every guard requires `!certain`, which every modelled caller satisfies
/// (no modelled move carries `MOVE_EFFECT_CERTAIN`).
///
/// Returns the blocking ability, or `None` when the drop goes through.
/// The caller decides the surface: the stat-move path reports the block
/// (`BattleScript_AbilityNoStatLoss` / `_AbilityNoSpecificStatLoss`,
/// `data/battle_scripts_1.s`), the secondary path is silent.
#[must_use]
pub fn stat_drop_blocker(
    target_ability: AbilityId,
    stat: crate::stat_change::ChangedStat,
    silent_secondary: bool,
) -> Option<AbilityId> {
    use crate::stat_change::ChangedStat;
    if target_ability == CLEAR_BODY || target_ability == WHITE_SMOKE {
        return Some(target_ability);
    }
    if target_ability == KEEN_EYE && stat == ChangedStat::Accuracy {
        return Some(target_ability);
    }
    if target_ability == HYPER_CUTTER && stat == ChangedStat::Attack {
        return Some(target_ability);
    }
    if silent_secondary && target_ability == SHIELD_DUST {
        return Some(target_ability);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dex::Dex;
    use crate::stat_change::ChangedStat;
    use assets::SpeciesId;

    /// The four sight-trainer species really carry the abilities the
    /// module docs argue about, in slot 0 (`gSpeciesInfo`,
    /// `src/data/pokemon/species_info.h`).
    #[test]
    fn the_seeded_parties_slot_zero_abilities_are_the_documented_ones() {
        let dex = Dex::new();
        let slot0 = |id: u16| dex.species(SpeciesId(id)).unwrap().abilities[0];
        assert_eq!(slot0(72), CLEAR_BODY, "Tentacool (Andrew, Pete)");
        assert_eq!(slot0(335), THICK_FAT, "Makuhita (Rhett)");
        assert_eq!(slot0(100), SOUNDPROOF, "Voltorb (Marcos)");
    }

    /// `ChangeStatBuffs`' guard chain, branch by branch
    /// (`src/battle_script_commands.c:6987`-`:7038`).
    #[test]
    fn stat_drop_blocker_reproduces_the_guard_chain() {
        // Clear Body / White Smoke: every stat, both paths.
        for ability in [CLEAR_BODY, WHITE_SMOKE] {
            for stat in [
                ChangedStat::Attack,
                ChangedStat::Speed,
                ChangedStat::Accuracy,
            ] {
                assert_eq!(stat_drop_blocker(ability, stat, false), Some(ability));
                assert_eq!(stat_drop_blocker(ability, stat, true), Some(ability));
            }
        }
        // Keen Eye: accuracy only. Hyper Cutter: Attack only.
        assert_eq!(
            stat_drop_blocker(KEEN_EYE, ChangedStat::Accuracy, false),
            Some(KEEN_EYE)
        );
        assert_eq!(
            stat_drop_blocker(KEEN_EYE, ChangedStat::Attack, false),
            None
        );
        assert_eq!(
            stat_drop_blocker(HYPER_CUTTER, ChangedStat::Attack, false),
            Some(HYPER_CUTTER)
        );
        assert_eq!(
            stat_drop_blocker(HYPER_CUTTER, ChangedStat::Defense, false),
            None
        );
        // Shield Dust: the silent `flags == 0` secondary path only
        // (`:7035`-`:7038`).
        assert_eq!(
            stat_drop_blocker(SHIELD_DUST, ChangedStat::Speed, true),
            Some(SHIELD_DUST)
        );
        assert_eq!(
            stat_drop_blocker(SHIELD_DUST, ChangedStat::Speed, false),
            None
        );
        // An inert ability blocks nothing.
        assert_eq!(
            stat_drop_blocker(AbilityId(1), ChangedStat::Attack, true),
            None
        );
    }

    /// `sSoundMovesTable` membership decides the Soundproof block; the
    /// ability alone does not.
    #[test]
    fn soundproof_blocks_exactly_the_sound_moves() {
        for mv in SOUND_MOVES {
            assert!(soundproof_blocks(SOUNDPROOF, mv));
        }
        assert!(!soundproof_blocks(SOUNDPROOF, assets::MoveId(33))); // Tackle
        assert!(!soundproof_blocks(CLEAR_BODY, assets::MoveId(45))); // Growl
    }
}
