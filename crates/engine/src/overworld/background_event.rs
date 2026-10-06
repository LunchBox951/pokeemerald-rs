//! Directional background-event target resolution
//! (`GetInteractedBackgroundEventScript`, `pokeemerald/src/field_control_avatar.c:316-365`,
//! and `GetBackgroundEventAtPosition`, `:923-937`).
//!
//! Selection only: the result names what the launcher should start, and nothing
//! here runs a script, writes a flag or special variable, or plays a sound.
//! Object-event lookup and cross-kind precedence belong to the caller.
//! Secret-base eligibility depends on saved secret-base state
//! (`TrySetCurSecretBase`) that `EventData` does not hold, so a secret-base
//! target is the north-facing entrance context only.

use assets::items::ItemId;
use assets::{BgEventKind, FacingDirection, MapEvents, SecretBaseId};

use super::collision::ELEVATION_TRANSITION;
use super::direction::Direction;
use crate::event_data::EventData;

/// Upstream's script for a sign whose `script` is null.
const NULL_SIGN_SCRIPT: &str = "EventScript_TestSignpostMsg";
/// `"0x0"` in map data is a null script pointer.
const NO_SCRIPT: &str = "0x0";
/// `EventScript_HiddenItemScript` adds one item.
const HIDDEN_ITEM_QUANTITY: u16 = 1;

/// The context `EventScript_HiddenItemScript` receives for an uncollected hidden item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HiddenItemContext {
    /// The item found (`VAR_0x8005`).
    pub item: ItemId,
    /// How many are added.
    pub quantity: u16,
    /// The absolute flag set on pickup (`VAR_0x8004`).
    pub flag: u16,
}

/// The secret-base entrance the player faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretBaseContext {
    /// The entrance's secret-base id (`VAR_0x8004`).
    pub secret_base_id: SecretBaseId,
}

/// What a background event asks the launcher to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundEventTarget {
    /// A sign's script.
    Script {
        /// The upstream script symbol.
        script: &'static str,
    },
    /// An uncollected hidden item.
    HiddenItem(HiddenItemContext),
    /// A secret-base entrance.
    SecretBase(SecretBaseContext),
}

const fn facing_matches(required: FacingDirection, facing: Direction) -> bool {
    matches!(
        (required, facing),
        (FacingDirection::Any, _)
            | (FacingDirection::North, Direction::North)
            | (FacingDirection::South, Direction::South)
            | (FacingDirection::East, Direction::East)
            | (FacingDirection::West, Direction::West)
    )
}

/// Resolves the background event at the map-local target tile `(x, y)`.
///
/// The first event in declaration order at that tile whose elevation is zero or
/// equal to `elevation` is the only candidate; a wrong-facing sign, a collected
/// hidden item, or a secret base faced from a non-north direction yields `None`
/// without falling through to a later event at the same tile.
#[must_use]
pub fn resolve_background_event(
    events: &MapEvents,
    x: i32,
    y: i32,
    elevation: u8,
    facing: Direction,
    event_data: &EventData,
) -> Option<BackgroundEventTarget> {
    let event = events.bg_events.iter().find(|event| {
        i32::from(event.x) == x
            && i32::from(event.y) == y
            && (event.elevation == ELEVATION_TRANSITION || event.elevation == elevation)
    })?;
    match event.kind {
        BgEventKind::Sign {
            facing: required,
            script,
        } => {
            if script == NO_SCRIPT {
                Some(BackgroundEventTarget::Script {
                    script: NULL_SIGN_SCRIPT,
                })
            } else if facing_matches(required, facing) {
                Some(BackgroundEventTarget::Script { script })
            } else {
                None
            }
        }
        BgEventKind::HiddenItem { item, flag } => {
            let flag = assets::hidden_item_flags::resolve(flag)?;
            if event_data.flag_get(flag).ok()? {
                return None;
            }
            Some(BackgroundEventTarget::HiddenItem(HiddenItemContext {
                item,
                quantity: HIDDEN_ITEM_QUANTITY,
                flag,
            }))
        }
        BgEventKind::SecretBase(secret_base_id) => (facing == Direction::North).then_some(
            BackgroundEventTarget::SecretBase(SecretBaseContext { secret_base_id }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use assets::{BgEvent, MapEventsTable, MapId};

    use super::*;

    const ALL: [Direction; 4] = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ];
    const CANDY_FLAG: &str = "FLAG_HIDDEN_ITEM_PETALBURG_CITY_RARE_CANDY";
    const CANDY_FLAG_ID: u16 = 0x253;

    const fn sign(
        x: u16,
        y: u16,
        elevation: u8,
        facing: FacingDirection,
        script: &'static str,
    ) -> BgEvent {
        BgEvent {
            x,
            y,
            elevation,
            kind: BgEventKind::Sign { facing, script },
        }
    }

    const fn candy(x: u16, y: u16, elevation: u8) -> BgEvent {
        BgEvent {
            x,
            y,
            elevation,
            kind: BgEventKind::HiddenItem {
                item: ItemId(68),
                flag: CANDY_FLAG,
            },
        }
    }

    fn events(bg_events: &'static [BgEvent]) -> MapEvents {
        MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events,
        }
    }

    fn resolve(
        events: &MapEvents,
        elevation: u8,
        facing: Direction,
        data: &EventData,
    ) -> Option<BackgroundEventTarget> {
        resolve_background_event(events, 3, 4, elevation, facing, data)
    }

    fn script(script: &'static str) -> BackgroundEventTarget {
        BackgroundEventTarget::Script { script }
    }

    #[test]
    fn sign_requires_its_facing() {
        let data = EventData::new();
        for (required, only) in [
            (FacingDirection::North, Some(Direction::North)),
            (FacingDirection::South, Some(Direction::South)),
            (FacingDirection::East, Some(Direction::East)),
            (FacingDirection::West, Some(Direction::West)),
            (FacingDirection::Any, None),
        ] {
            let map = events(Box::leak(Box::new([sign(3, 4, 0, required, "Sign")])));
            for facing in ALL {
                let expected = if only.is_none_or(|d| d == facing) {
                    Some(script("Sign"))
                } else {
                    None
                };
                assert_eq!(
                    resolve(&map, 0, facing, &data),
                    expected,
                    "{required:?} {facing:?}"
                );
            }
        }
    }

    #[test]
    fn null_sign_script_resolves_to_the_test_signpost_for_any_facing_requirement() {
        const BG: [BgEvent; 1] = [sign(3, 4, 0, FacingDirection::North, "0x0")];
        let map = events(&BG);
        assert_eq!(
            resolve(&map, 0, Direction::South, &EventData::new()),
            Some(script("EventScript_TestSignpostMsg"))
        );
    }

    #[test]
    fn event_elevation_zero_matches_any_player_elevation_and_nonzero_must_be_equal() {
        const BG: [BgEvent; 2] = [
            sign(3, 4, 3, FacingDirection::Any, "Three"),
            sign(1, 1, 0, FacingDirection::Any, "Transition"),
        ];
        let map = events(&BG);
        let data = EventData::new();
        assert_eq!(
            resolve(&map, 3, Direction::North, &data),
            Some(script("Three"))
        );
        assert_eq!(resolve(&map, 0, Direction::North, &data), None);
        assert_eq!(resolve(&map, 4, Direction::North, &data), None);
        for elevation in [0, 3, 15] {
            assert_eq!(
                resolve_background_event(&map, 1, 1, elevation, Direction::North, &data),
                Some(script("Transition"))
            );
        }
    }

    #[test]
    fn duplicate_positions_keep_declaration_order() {
        const BG: [BgEvent; 3] = [
            sign(3, 4, 2, FacingDirection::Any, "WrongElevation"),
            sign(3, 4, 0, FacingDirection::Any, "First"),
            sign(3, 4, 0, FacingDirection::Any, "Second"),
        ];
        let map = events(&BG);
        assert_eq!(
            resolve(&map, 5, Direction::North, &EventData::new()),
            Some(script("First"))
        );
    }

    #[test]
    fn rejected_first_candidate_does_not_fall_through_to_a_later_duplicate() {
        const BG: [BgEvent; 2] = [
            sign(3, 4, 0, FacingDirection::North, "North"),
            sign(3, 4, 0, FacingDirection::Any, "Any"),
        ];
        let map = events(&BG);
        assert_eq!(resolve(&map, 0, Direction::South, &EventData::new()), None);
    }

    #[test]
    fn missing_position_resolves_to_nothing() {
        const BG: [BgEvent; 1] = [sign(3, 4, 0, FacingDirection::Any, "Sign")];
        let map = events(&BG);
        let data = EventData::new();
        assert_eq!(
            resolve_background_event(&map, 4, 3, 0, Direction::North, &data),
            None
        );
        assert_eq!(
            resolve_background_event(&map, -3, 4, 0, Direction::North, &data),
            None
        );
    }

    #[test]
    fn uncollected_hidden_item_resolves_from_every_facing() {
        const BG: [BgEvent; 1] = [candy(3, 4, 3)];
        let map = events(&BG);
        for facing in ALL {
            assert_eq!(
                resolve(&map, 3, facing, &EventData::new()),
                Some(BackgroundEventTarget::HiddenItem(HiddenItemContext {
                    item: ItemId(68),
                    quantity: 1,
                    flag: CANDY_FLAG_ID,
                }))
            );
        }
    }

    #[test]
    fn collected_hidden_item_is_suppressed_without_falling_through() {
        const BG: [BgEvent; 2] = [candy(3, 4, 0), sign(3, 4, 0, FacingDirection::Any, "Later")];
        let map = events(&BG);
        let mut data = EventData::new();
        data.flag_set(CANDY_FLAG_ID).unwrap();
        for facing in ALL {
            assert_eq!(resolve(&map, 0, facing, &data), None);
        }
    }

    #[test]
    fn secret_base_requires_facing_north_and_keeps_its_id() {
        const BG: [BgEvent; 1] = [BgEvent {
            x: 3,
            y: 4,
            elevation: 0,
            kind: BgEventKind::SecretBase(SecretBaseId("SECRET_BASE_YELLOW_CAVE2_1")),
        }];
        let map = events(&BG);
        let data = EventData::new();
        assert_eq!(
            resolve(&map, 0, Direction::North, &data),
            Some(BackgroundEventTarget::SecretBase(SecretBaseContext {
                secret_base_id: SecretBaseId("SECRET_BASE_YELLOW_CAVE2_1"),
            }))
        );
        for facing in [Direction::South, Direction::East, Direction::West] {
            assert_eq!(resolve(&map, 0, facing, &data), None);
        }
    }

    #[test]
    fn every_bundled_hidden_item_flag_resolves_to_an_ordinary_flag() {
        let mut seen = 0;
        for map in MapEventsTable::new().iter() {
            for event in map.bg_events {
                if let BgEventKind::HiddenItem { flag, .. } = event.kind {
                    let id = assets::hidden_item_flags::resolve(flag)
                        .unwrap_or_else(|| panic!("unresolved {flag}"));
                    assert!(EventData::new().flag_get(id).is_ok(), "{flag}");
                    seen += 1;
                }
            }
        }
        assert!(seen > 0);
        assert_eq!(assets::hidden_item_flags::HIDDEN_ITEM_FLAG_COUNT, 112);
    }
}
