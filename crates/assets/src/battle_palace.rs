//! The Battle Palace nature tables: for each of the 25 natures, the chances
//! of picking a move from the attack and defense groups, the preferred
//! target in a double battle, and the flavor-text message id.
//!
//! Sources:
//!
//! - `gBattlePalaceNatureToMoveGroupLikelihood`
//!   (`src/battle_script_commands.c:849-884`), read by
//!   `src/battle_gfx_sfx_util.c:141`.
//! - `sBattlePalaceNatureToFlavorTextId`
//!   (`src/battle_script_commands.c:886-913`), read by the
//!   `VARIOUS_PALACE_FLAVOR_TEXT` command in the same file.
//! - `gBattlePalaceNatureToMoveTarget`
//!   (`src/battle_anim_smokescreen.c:20-47`), read by
//!   `src/battle_gfx_sfx_util.c:340`.
//!
//! The tables are indexed by the Gen III nature id (`NATURE_*`, `0..25`,
//! the value `GetNatureFromPersonality` returns). This crate holds no nature
//! type of its own and cannot depend on `battle`, so the lookups take the raw
//! id (`battle::Nature::id`). Data only: Palace move selection, its RNG
//! draws, the move-group classification of each move, battle scripts and the
//! flavor strings are not ported here.

use std::error::Error;
use std::fmt;

/// The number of natures (`NUM_NATURES`), the length of each table.
pub const NATURES_COUNT: usize = 25;

/// A failed lookup in a Battle Palace nature table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattlePalaceError {
    /// The value is not a `NATURE_*` id below [`NATURES_COUNT`].
    NatureOutOfRange(u8),
}

impl fmt::Display for BattlePalaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NatureOutOfRange(value) => write!(f, "nature id `{value}` is out of range"),
        }
    }
}

impl Error for BattlePalaceError {}

/// One nature's row of `gBattlePalaceNatureToMoveGroupLikelihood`: the
/// cumulative percent thresholds that choose a move group.
///
/// The Palace rolls `Random() % 100` and compares it with one of two pairs.
/// Upstream picks the pair from the battler's sticky `palaceFlags` bit
/// (`src/battle_gfx_sfx_util.c:130`): the bit is set at end of turn the
/// first time an awake battler is seen at or below half HP
/// (`src/battle_script_commands.c:6389-6395`) and is not cleared when it
/// heals, so "at or below half HP" names that bit, not the current HP. A roll below the `*_attack_below` threshold
/// selects the attack group; otherwise a roll below the `*_defense_below`
/// threshold selects the defense group; otherwise the support group is
/// selected. The `*_defense_below` threshold is cumulative: it is the attack
/// percent plus the defense percent, as upstream stores it.
///
/// The pair used above half HP and the pair used at or below half HP carry
/// different field names, so a caller cannot read one pair's thresholds as
/// the other's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveGroupLikelihood {
    /// Table slot 0: attack-group threshold while above half HP.
    pub above_half_attack_below: u8,
    /// Table slot 1: defense-group threshold (cumulative) while above half HP.
    pub above_half_defense_below: u8,
    /// Table slot 2: attack-group threshold at or below half HP.
    pub at_or_below_half_attack_below: u8,
    /// Table slot 3: defense-group threshold (cumulative) at or below half HP.
    pub at_or_below_half_defense_below: u8,
}

/// Builds a row from the four per-group percents, mirroring upstream's
/// `PALACE_STYLE` macro: the defense thresholds add the attack percent.
const fn palace_style(
    attack: u8,
    defense: u8,
    attack_low: u8,
    defense_low: u8,
) -> MoveGroupLikelihood {
    MoveGroupLikelihood {
        above_half_attack_below: attack,
        above_half_defense_below: attack + defense,
        at_or_below_half_attack_below: attack_low,
        at_or_below_half_defense_below: attack_low + defense_low,
    }
}

/// The Palace target preference of a nature (`PALACE_TARGET_*`), used in a
/// double battle when the two opposing battlers' HP differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalaceTarget {
    /// `PALACE_TARGET_STRONGER` (0): the opposing battler with more HP.
    Stronger,
    /// `PALACE_TARGET_WEAKER` (1): the opposing battler with less HP.
    Weaker,
    /// `PALACE_TARGET_RANDOM` (2): a random opposing battler.
    Random,
}

impl PalaceTarget {
    /// Returns the upstream `PALACE_TARGET_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::Stronger => 0,
            Self::Weaker => 1,
            Self::Random => 2,
        }
    }
}

/// The end-of-turn flavor message a nature prints when its battler first
/// drops to half HP or below: a `B_MSG_*` multistring chooser index. The
/// message strings are not ported here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalaceFlavorText {
    /// `B_MSG_GLINT_IN_EYE` (0).
    GlintInEye,
    /// `B_MSG_GETTING_IN_POS` (1).
    GettingInPosition,
    /// `B_MSG_GROWL_DEEPLY` (2).
    GrowlDeeply,
    /// `B_MSG_EAGER_FOR_MORE` (3).
    EagerForMore,
}

impl PalaceFlavorText {
    /// Returns the upstream `B_MSG_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::GlintInEye => 0,
            Self::GettingInPosition => 1,
            Self::GrowlDeeply => 2,
            Self::EagerForMore => 3,
        }
    }
}

/// Looks up a nature's move-group likelihood row
/// (`gBattlePalaceNatureToMoveGroupLikelihood`).
///
/// # Errors
///
/// Returns [`BattlePalaceError::NatureOutOfRange`] when `nature` is
/// [`NATURES_COUNT`] or more.
pub const fn battle_palace_move_group_likelihood(
    nature: u8,
) -> Result<MoveGroupLikelihood, BattlePalaceError> {
    if (nature as usize) < NATURES_COUNT {
        Ok(MOVE_GROUP_LIKELIHOOD[nature as usize])
    } else {
        Err(BattlePalaceError::NatureOutOfRange(nature))
    }
}

/// Looks up a nature's flavor-text message
/// (`sBattlePalaceNatureToFlavorTextId`).
///
/// # Errors
///
/// Returns [`BattlePalaceError::NatureOutOfRange`] when `nature` is
/// [`NATURES_COUNT`] or more.
pub const fn battle_palace_flavor_text(nature: u8) -> Result<PalaceFlavorText, BattlePalaceError> {
    if (nature as usize) < NATURES_COUNT {
        Ok(FLAVOR_TEXT[nature as usize])
    } else {
        Err(BattlePalaceError::NatureOutOfRange(nature))
    }
}

/// Looks up a nature's double-battle target preference
/// (`gBattlePalaceNatureToMoveTarget`).
///
/// # Errors
///
/// Returns [`BattlePalaceError::NatureOutOfRange`] when `nature` is
/// [`NATURES_COUNT`] or more.
pub const fn battle_palace_move_target(nature: u8) -> Result<PalaceTarget, BattlePalaceError> {
    if (nature as usize) < NATURES_COUNT {
        Ok(MOVE_TARGET[nature as usize])
    } else {
        Err(BattlePalaceError::NatureOutOfRange(nature))
    }
}

/// `gBattlePalaceNatureToMoveGroupLikelihood`, indexed by nature id. Each
/// row is the attack and defense percents above half HP, then at or below
/// half HP.
const MOVE_GROUP_LIKELIHOOD: [MoveGroupLikelihood; NATURES_COUNT] = [
    palace_style(61, 7, 61, 7),   // Hardy
    palace_style(20, 25, 84, 8),  // Lonely
    palace_style(70, 15, 32, 60), // Brave
    palace_style(38, 31, 70, 15), // Adamant
    palace_style(20, 70, 70, 22), // Naughty
    palace_style(30, 20, 32, 58), // Bold
    palace_style(56, 22, 56, 22), // Docile
    palace_style(25, 15, 75, 15), // Relaxed
    palace_style(69, 6, 28, 55),  // Impish
    palace_style(35, 10, 29, 6),  // Lax
    palace_style(62, 10, 30, 20), // Timid
    palace_style(58, 37, 88, 6),  // Hasty
    palace_style(34, 11, 29, 11), // Serious
    palace_style(35, 5, 35, 60),  // Jolly
    palace_style(56, 22, 56, 22), // Naive
    palace_style(35, 45, 34, 60), // Modest
    palace_style(44, 50, 34, 6),  // Mild
    palace_style(56, 22, 56, 22), // Quiet
    palace_style(30, 58, 30, 58), // Bashful
    palace_style(30, 13, 27, 6),  // Rash
    palace_style(40, 50, 25, 62), // Calm
    palace_style(18, 70, 90, 5),  // Gentle
    palace_style(88, 6, 22, 20),  // Sassy
    palace_style(42, 50, 42, 5),  // Careful
    palace_style(56, 22, 56, 22), // Quirky
];

/// `sBattlePalaceNatureToFlavorTextId`, indexed by nature id.
const FLAVOR_TEXT: [PalaceFlavorText; NATURES_COUNT] = {
    use PalaceFlavorText::{EagerForMore, GettingInPosition, GlintInEye, GrowlDeeply};
    [
        EagerForMore,      // Hardy
        GlintInEye,        // Lonely
        GettingInPosition, // Brave
        GlintInEye,        // Adamant
        GlintInEye,        // Naughty
        GettingInPosition, // Bold
        EagerForMore,      // Docile
        GlintInEye,        // Relaxed
        GettingInPosition, // Impish
        GrowlDeeply,       // Lax
        GrowlDeeply,       // Timid
        GlintInEye,        // Hasty
        EagerForMore,      // Serious
        GettingInPosition, // Jolly
        EagerForMore,      // Naive
        GettingInPosition, // Modest
        GrowlDeeply,       // Mild
        EagerForMore,      // Quiet
        EagerForMore,      // Bashful
        GrowlDeeply,       // Rash
        GettingInPosition, // Calm
        GlintInEye,        // Gentle
        GrowlDeeply,       // Sassy
        GrowlDeeply,       // Careful
        EagerForMore,      // Quirky
    ]
};

/// `gBattlePalaceNatureToMoveTarget`, indexed by nature id.
const MOVE_TARGET: [PalaceTarget; NATURES_COUNT] = {
    use PalaceTarget::{Random, Stronger, Weaker};
    [
        Stronger, // Hardy
        Stronger, // Lonely
        Weaker,   // Brave
        Stronger, // Adamant
        Weaker,   // Naughty
        Weaker,   // Bold
        Random,   // Docile
        Stronger, // Relaxed
        Stronger, // Impish
        Stronger, // Lax
        Weaker,   // Timid
        Weaker,   // Hasty
        Weaker,   // Serious
        Stronger, // Jolly
        Random,   // Naive
        Weaker,   // Modest
        Stronger, // Mild
        Weaker,   // Quiet
        Weaker,   // Bashful
        Stronger, // Rash
        Stronger, // Calm
        Stronger, // Gentle
        Weaker,   // Sassy
        Weaker,   // Careful
        Stronger, // Quirky
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    /// `gBattlePalaceNatureToMoveGroupLikelihood` as the four stored bytes
    /// per nature (slots 0-3), with `PALACE_STYLE` already expanded. Produced
    /// by evaluating the upstream table with a script, not from the
    /// production literals.
    const EXPECTED_LIKELIHOOD: [[u8; 4]; NATURES_COUNT] = [
        [61, 68, 61, 68],
        [20, 45, 84, 92],
        [70, 85, 32, 92],
        [38, 69, 70, 85],
        [20, 90, 70, 92],
        [30, 50, 32, 90],
        [56, 78, 56, 78],
        [25, 40, 75, 90],
        [69, 75, 28, 83],
        [35, 45, 29, 35],
        [62, 72, 30, 50],
        [58, 95, 88, 94],
        [34, 45, 29, 40],
        [35, 40, 35, 95],
        [56, 78, 56, 78],
        [35, 80, 34, 94],
        [44, 94, 34, 40],
        [56, 78, 56, 78],
        [30, 88, 30, 88],
        [30, 43, 27, 33],
        [40, 90, 25, 87],
        [18, 88, 90, 95],
        [88, 94, 22, 42],
        [42, 92, 42, 47],
        [56, 78, 56, 78],
    ];

    /// `sBattlePalaceNatureToFlavorTextId` as `B_MSG_*` values (glint in
    /// eye 0, getting in position 1, growl deeply 2, eager for more 3).
    const EXPECTED_FLAVOR: [u8; NATURES_COUNT] = [
        3, 0, 1, 0, 0, 1, 3, 0, 1, 2, 2, 0, 3, 1, 3, 1, 2, 3, 3, 2, 1, 0, 2, 2, 3,
    ];

    /// `gBattlePalaceNatureToMoveTarget` as `PALACE_TARGET_*` values
    /// (stronger 0, weaker 1, random 2).
    const EXPECTED_TARGET: [u8; NATURES_COUNT] = [
        0, 0, 1, 0, 1, 1, 2, 0, 0, 0, 1, 1, 1, 0, 2, 1, 0, 1, 1, 0, 0, 0, 1, 1, 0,
    ];

    #[test]
    fn likelihood_matches_upstream_for_every_nature() {
        for (nature, expected) in (0u8..).zip(EXPECTED_LIKELIHOOD) {
            let row = battle_palace_move_group_likelihood(nature).unwrap();
            assert_eq!(
                [
                    row.above_half_attack_below,
                    row.above_half_defense_below,
                    row.at_or_below_half_attack_below,
                    row.at_or_below_half_defense_below,
                ],
                expected,
                "nature {nature}"
            );
        }
    }

    #[test]
    fn likelihood_names_keep_the_hp_pairs_apart() {
        // Lonely: 20/25 above half HP, 84/8 at or below it. A swapped pair
        // would put 84 in the above-half attack field.
        let lonely = battle_palace_move_group_likelihood(1).unwrap();
        assert_eq!(lonely.above_half_attack_below, 20);
        assert_eq!(lonely.above_half_defense_below, 45);
        assert_eq!(lonely.at_or_below_half_attack_below, 84);
        assert_eq!(lonely.at_or_below_half_defense_below, 92);
    }

    #[test]
    fn flavor_text_matches_upstream_for_every_nature() {
        for (nature, expected) in (0u8..).zip(EXPECTED_FLAVOR) {
            let message = battle_palace_flavor_text(nature).unwrap();
            assert_eq!(message.id(), expected, "nature {nature}");
        }
    }

    #[test]
    fn move_target_matches_upstream_for_every_nature() {
        for (nature, expected) in (0u8..).zip(EXPECTED_TARGET) {
            let target = battle_palace_move_target(nature).unwrap();
            assert_eq!(target.id(), expected, "nature {nature}");
        }
    }

    #[test]
    fn variants_map_to_their_named_upstream_values() {
        assert_eq!(PalaceTarget::Stronger.id(), 0);
        assert_eq!(PalaceTarget::Weaker.id(), 1);
        assert_eq!(PalaceTarget::Random.id(), 2);
        assert_eq!(PalaceFlavorText::GlintInEye.id(), 0);
        assert_eq!(PalaceFlavorText::GettingInPosition.id(), 1);
        assert_eq!(PalaceFlavorText::GrowlDeeply.id(), 2);
        assert_eq!(PalaceFlavorText::EagerForMore.id(), 3);
    }

    #[test]
    fn lookups_reject_nature_ids_at_or_above_the_count() {
        for nature in [25u8, 26, 255] {
            let error = BattlePalaceError::NatureOutOfRange(nature);
            assert_eq!(battle_palace_move_group_likelihood(nature), Err(error));
            assert_eq!(battle_palace_flavor_text(nature), Err(error));
            assert_eq!(battle_palace_move_target(nature), Err(error));
        }
        assert_eq!(
            BattlePalaceError::NatureOutOfRange(25).to_string(),
            "nature id `25` is out of range"
        );
    }
}
