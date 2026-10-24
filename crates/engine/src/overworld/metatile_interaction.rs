//! Classifies the script-backed targets of a metatile interaction.
//!
//! This mirrors the script-returning branches of `GetInteractedMetatileScript`
//! and names the target without launching it. The furniture and poster
//! branches only update secret-base variables and return no script, so they
//! classify as nothing.

use super::direction::Direction;
use super::metatile_behavior as mb;

/// The script-backed thing a metatile interaction would reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetatileInteractionTarget {
    /// A television screen, read facing north.
    Tv,
    /// A PC.
    Pc,
    /// Sootopolis's closed gym door.
    ClosedSootopolisDoor,
    /// Sky Pillar's closed door.
    SkyPillarClosedDoor,
    /// A cable-club results box; both behaviors share one script.
    CableBoxResults,
    /// A Pokeblock feeder.
    PokeblockFeeder,
    /// A Trick House puzzle door.
    TrickHousePuzzleDoor,
    /// A region map.
    RegionMap,
    /// The Running Shoes manual.
    RunningShoesManual,
    /// A picture bookshelf.
    PictureBookshelf,
    /// A bookshelf.
    Bookshelf,
    /// A Pokemon Center bookshelf.
    PokemonCenterBookshelf,
    /// A vase.
    Vase,
    /// A trash can.
    EmptyTrashCan,
    /// A shop shelf.
    ShopShelf,
    /// A blueprint.
    Blueprint,
    /// A wireless-club results box, read facing north.
    WirelessBoxResults,
    /// A questionnaire.
    Questionnaire,
    /// The Trainer Hill timer.
    TrainerHillTimer,
    /// A secret-base PC at matching elevation.
    SecretBasePc,
    /// A secret-base record-mixing PC at matching elevation.
    RecordMixingSecretBasePc,
    /// A secret-base sand ornament at matching elevation.
    SecretBaseSandOrnament,
    /// A secret-base shield or toy TV at matching elevation.
    SecretBaseShieldOrToyTv,
}

/// Classifies the metatile behavior a player at `direction` interacts with.
///
/// `interaction_elevation` is the elevation of the interaction position and
/// `tile_elevation` the raw elevation of the target map cell. Secret-base
/// targets require them to be exactly equal, with no wildcard elevations.
/// Behaviors without a script return `None`.
#[must_use]
pub const fn classify_metatile_interaction(
    behavior: u8,
    direction: Direction,
    interaction_elevation: u8,
    tile_elevation: u8,
) -> Option<MetatileInteractionTarget> {
    use MetatileInteractionTarget as T;
    if mb::is_player_facing_tv_screen(behavior, direction) {
        return Some(T::Tv);
    }
    if mb::is_player_facing_wireless_box_results(behavior, direction) {
        return Some(T::WirelessBoxResults);
    }
    if mb::is_cable_box_results_2(behavior, direction) {
        return Some(T::CableBoxResults);
    }
    let ungated = match behavior {
        mb::MB_PC => Some(T::Pc),
        mb::MB_CLOSED_SOOTOPOLIS_DOOR => Some(T::ClosedSootopolisDoor),
        mb::MB_SKY_PILLAR_CLOSED_DOOR => Some(T::SkyPillarClosedDoor),
        mb::MB_CABLE_BOX_RESULTS_1 => Some(T::CableBoxResults),
        mb::MB_POKEBLOCK_FEEDER => Some(T::PokeblockFeeder),
        mb::MB_TRICK_HOUSE_PUZZLE_DOOR => Some(T::TrickHousePuzzleDoor),
        mb::MB_REGION_MAP => Some(T::RegionMap),
        mb::MB_RUNNING_SHOES_INSTRUCTION => Some(T::RunningShoesManual),
        mb::MB_PICTURE_BOOK_SHELF => Some(T::PictureBookshelf),
        mb::MB_BOOKSHELF => Some(T::Bookshelf),
        mb::MB_POKEMON_CENTER_BOOKSHELF => Some(T::PokemonCenterBookshelf),
        mb::MB_VASE => Some(T::Vase),
        mb::MB_TRASH_CAN => Some(T::EmptyTrashCan),
        mb::MB_SHOP_SHELF => Some(T::ShopShelf),
        mb::MB_BLUEPRINT => Some(T::Blueprint),
        mb::MB_QUESTIONNAIRE => Some(T::Questionnaire),
        mb::MB_TRAINER_HILL_TIMER => Some(T::TrainerHillTimer),
        _ => None,
    };
    if ungated.is_some() {
        return ungated;
    }
    if interaction_elevation != tile_elevation {
        return None;
    }
    match behavior {
        mb::MB_SECRET_BASE_PC => Some(T::SecretBasePc),
        mb::MB_SECRET_BASE_REGISTER_PC => Some(T::RecordMixingSecretBasePc),
        mb::MB_SECRET_BASE_SAND_ORNAMENT => Some(T::SecretBaseSandOrnament),
        mb::MB_SECRET_BASE_TV_SHIELD => Some(T::SecretBaseShieldOrToyTv),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::MetatileInteractionTarget as T;
    use super::*;

    const DIRECTIONS: [Direction; 4] = [
        Direction::South,
        Direction::North,
        Direction::West,
        Direction::East,
    ];
    const MB_WATERFALL: u8 = 0x13;
    // Furniture and poster branches return no script upstream.
    const BOOKKEEPING: [u8; 4] = [0xC6, 0xC3, 0xB5, 0xC7];

    /// `(behavior, target, north_only, elevation_gated)` for every script branch.
    const BRANCHES: [(u8, T, bool, bool); 24] = [
        (mb::MB_TELEVISION, T::Tv, true, false),
        (mb::MB_PC, T::Pc, false, false),
        (
            mb::MB_CLOSED_SOOTOPOLIS_DOOR,
            T::ClosedSootopolisDoor,
            false,
            false,
        ),
        (
            mb::MB_SKY_PILLAR_CLOSED_DOOR,
            T::SkyPillarClosedDoor,
            false,
            false,
        ),
        (mb::MB_CABLE_BOX_RESULTS_1, T::CableBoxResults, false, false),
        (mb::MB_POKEBLOCK_FEEDER, T::PokeblockFeeder, false, false),
        (
            mb::MB_TRICK_HOUSE_PUZZLE_DOOR,
            T::TrickHousePuzzleDoor,
            false,
            false,
        ),
        (mb::MB_REGION_MAP, T::RegionMap, false, false),
        (
            mb::MB_RUNNING_SHOES_INSTRUCTION,
            T::RunningShoesManual,
            false,
            false,
        ),
        (mb::MB_PICTURE_BOOK_SHELF, T::PictureBookshelf, false, false),
        (mb::MB_BOOKSHELF, T::Bookshelf, false, false),
        (
            mb::MB_POKEMON_CENTER_BOOKSHELF,
            T::PokemonCenterBookshelf,
            false,
            false,
        ),
        (mb::MB_VASE, T::Vase, false, false),
        (mb::MB_TRASH_CAN, T::EmptyTrashCan, false, false),
        (mb::MB_SHOP_SHELF, T::ShopShelf, false, false),
        (mb::MB_BLUEPRINT, T::Blueprint, false, false),
        (
            mb::MB_WIRELESS_BOX_RESULTS,
            T::WirelessBoxResults,
            true,
            false,
        ),
        (mb::MB_CABLE_BOX_RESULTS_2, T::CableBoxResults, true, false),
        (mb::MB_QUESTIONNAIRE, T::Questionnaire, false, false),
        (mb::MB_TRAINER_HILL_TIMER, T::TrainerHillTimer, false, false),
        (mb::MB_SECRET_BASE_PC, T::SecretBasePc, false, true),
        (
            mb::MB_SECRET_BASE_REGISTER_PC,
            T::RecordMixingSecretBasePc,
            false,
            true,
        ),
        (
            mb::MB_SECRET_BASE_SAND_ORNAMENT,
            T::SecretBaseSandOrnament,
            false,
            true,
        ),
        (
            mb::MB_SECRET_BASE_TV_SHIELD,
            T::SecretBaseShieldOrToyTv,
            false,
            true,
        ),
    ];

    const ELEVATIONS: [(u8, u8); 8] = [
        (3, 3),
        (3, 4),
        (0, 3),
        (3, 0),
        (3, 15),
        (15, 3),
        (0, 0),
        (15, 15),
    ];

    fn expected(behavior: u8, direction: Direction, elevations: (u8, u8)) -> Option<T> {
        BRANCHES
            .iter()
            .find(|row| row.0 == behavior)
            .and_then(|&(_, target, north_only, gated)| {
                let facing_ok = !north_only || direction == Direction::North;
                let elevation_ok = !gated || elevations.0 == elevations.1;
                (facing_ok && elevation_ok).then_some(target)
            })
    }

    #[test]
    fn every_behavior_matches_the_branch_table() {
        for behavior in 0..=u8::MAX {
            for direction in DIRECTIONS {
                for elevations in ELEVATIONS {
                    assert_eq!(
                        classify_metatile_interaction(
                            behavior,
                            direction,
                            elevations.0,
                            elevations.1
                        ),
                        expected(behavior, direction, elevations),
                        "behavior {behavior:#04x} {direction:?} {elevations:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn representative_targets_classify() {
        let table = [
            (mb::MB_PC, Direction::North, (3, 3), Some(T::Pc)),
            (mb::MB_PC, Direction::South, (3, 4), Some(T::Pc)),
            (mb::MB_TELEVISION, Direction::North, (3, 3), Some(T::Tv)),
            (mb::MB_TELEVISION, Direction::North, (3, 4), Some(T::Tv)),
            (mb::MB_TELEVISION, Direction::South, (3, 3), None),
            (mb::MB_TELEVISION, Direction::West, (3, 3), None),
            (mb::MB_TELEVISION, Direction::East, (3, 3), None),
            (
                mb::MB_BOOKSHELF,
                Direction::North,
                (3, 3),
                Some(T::Bookshelf),
            ),
            (
                mb::MB_BOOKSHELF,
                Direction::East,
                (3, 4),
                Some(T::Bookshelf),
            ),
            (mb::MB_WIRELESS_BOX_RESULTS, Direction::South, (3, 3), None),
            (
                mb::MB_CABLE_BOX_RESULTS_1,
                Direction::East,
                (3, 4),
                Some(T::CableBoxResults),
            ),
            (
                mb::MB_CABLE_BOX_RESULTS_2,
                Direction::North,
                (3, 4),
                Some(T::CableBoxResults),
            ),
            (mb::MB_CABLE_BOX_RESULTS_2, Direction::West, (3, 3), None),
            (
                mb::MB_SECRET_BASE_TV_SHIELD,
                Direction::South,
                (3, 3),
                Some(T::SecretBaseShieldOrToyTv),
            ),
            (mb::MB_SECRET_BASE_PC, Direction::North, (3, 4), None),
            (mb::MB_SECRET_BASE_PC, Direction::North, (15, 3), None),
            (mb::MB_NORMAL, Direction::North, (3, 3), None),
            (MB_WATERFALL, Direction::North, (3, 3), None),
        ];
        for (behavior, direction, (interaction, tile), target) in table {
            assert_eq!(
                classify_metatile_interaction(behavior, direction, interaction, tile),
                target,
                "behavior {behavior:#04x} {direction:?} {interaction}/{tile}"
            );
        }
    }

    #[test]
    fn bookkeeping_branches_return_no_script() {
        for behavior in BOOKKEEPING {
            for direction in DIRECTIONS {
                for (interaction, tile) in [(3, 3), (3, 4)] {
                    assert_eq!(
                        classify_metatile_interaction(behavior, direction, interaction, tile),
                        None
                    );
                }
            }
        }
    }
}
