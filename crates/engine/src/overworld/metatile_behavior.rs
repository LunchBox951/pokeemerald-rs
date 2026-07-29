//! Metatile behavior classification (S-5), ported from
//! `pokeemerald/src/metatile_behavior.c`.
//!
//! Upstream's `constants/metatile_behaviors.h` defines roughly 200 `MB_*`
//! ids, and `metatile_behavior.c` has a matching `MetatileBehavior_Is*`
//! predicate for most of them (tall grass, ice, currents, ladders,
//! escalators, a dozen map-specific warp variants, secret-base furniture,
//! ...). Per the issue #108 scope, this module ports **only** the
//! predicates the v1 north-star path needs (protagonist's room -> downstairs
//! -> Littleroot outdoor -> Route 101): whether a tile is a door/warp
//! trigger, plus the ids upstream `GetAdjustedInitialDirection`
//! (`src/overworld.c:929-951`) branches on to pick the facing a warp *lands*
//! the player in ([`crate::overworld::warp::warp_in_facing`]). Everything
//! else this crate doesn't decode is deliberately left unclassified rather
//! than guessed at — see [`is_warp_trigger`]'s doc for how that fail-closed
//! policy plays out for warps specifically.
//!
//! **Naming an id is not making it triggerable.** The arrow-warp/door-alias
//! ids below exist because a warp *destination* tile can carry them (a front
//! door lands you on a `MB_SOUTH_ARROW_WARP` doormat), and the arrival facing
//! depends on which one it is. [`is_warp_trigger`]'s own membership is
//! unchanged by their presence: they still cannot *start* a warp here.
//!
//! **Ordinary walkability is not decided here.** Whether a tile can be
//! stepped onto at all is the [`MetatileCell::collision`](assets::MetatileCell::collision)
//! bit's job (`crate::overworld::collision`), independent of behavior — this
//! matches upstream, where `MB_TALL_GRASS`/`MB_ICE`/etc. are all ordinarily
//! passable and only *behavior-specific* systems (forced movement, warps)
//! branch on the `MB_*` id. Forced movement (currents, slopes, ice sliding)
//! is out of v1 scope entirely: this port does not special-case those
//! behaviors, so stepping onto e.g. an ice tile is currently indistinguishable
//! from stepping onto ordinary ground. That is a deliberate, documented gap,
//! not a silent behavioral claim — no test here asserts ice sliding works.

/// `MB_NORMAL` (0, `constants/metatile_behaviors.h`): ordinary ground, no
/// special handling. Not tested against directly by any function in this
/// module (every id *other* than a supported door/warp behavior is already
/// treated as ordinary ground) — kept as a named constant purely so callers
/// and tests can spell out "this tile is plain ground" instead of a bare
/// `0`.
pub const MB_NORMAL: u8 = 0;

/// `MB_NON_ANIMATED_DOOR` (`0x60`): a warp tile with no open/close animation.
/// Upstream uses this for interior staircases and other "just teleport"
/// warps as well as literal unanimated doors (`MetatileBehavior_IsNonAnimDoor`
/// also matches `MB_WATER_DOOR`/`MB_DEEP_SOUTH_WARP`, both out of v1 scope
/// and not ported here). This is the behavior the v1 north-star path's
/// house-interior stairs are assumed to use.
pub const MB_NON_ANIMATED_DOOR: u8 = 0x60;

/// `MB_ANIMATED_DOOR` (`0x69`): a warp tile that plays an open/close animation
/// (`MetatileBehavior_IsWarpDoor`/`MetatileBehavior_IsDoor`) — ordinary
/// building entrance doors. This port has no rendering/animation, so
/// [`is_warp_trigger`] treats it identically to
/// [`MB_NON_ANIMATED_DOOR`]: both trigger a warp once the player's tile
/// matches a [`WarpEvent`](assets::WarpEvent), with no distinction for the
/// door-approach-before-stepping-on-it timing upstream's `TryDoorWarp`
/// applies (see the module docs on that simplification in `crate::overworld::warp`).
pub const MB_ANIMATED_DOOR: u8 = 0x69;

/// `MB_STAIRS_OUTSIDE_ABANDONED_SHIP` (`0x1B`): the second id upstream
/// `MetatileBehavior_IsNorthArrowWarp` matches
/// (`metatile_behavior.c:304-310`). Named here only so
/// [`crate::overworld::warp::warp_in_facing`] can reproduce that predicate's
/// full membership; it is **not** a [`is_warp_trigger`] id (see that
/// function's fail-closed policy).
pub const MB_STAIRS_OUTSIDE_ABANDONED_SHIP: u8 = 0x1B;

/// `MB_SHOAL_CAVE_ENTRANCE` (`0x1C`): the third id upstream
/// `MetatileBehavior_IsSouthArrowWarp` matches
/// (`metatile_behavior.c:313-319`) — see
/// [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`] for why it is named but not
/// triggerable here.
pub const MB_SHOAL_CAVE_ENTRANCE: u8 = 0x1C;

/// `MB_EAST_ARROW_WARP` (`0x62`): upstream `MetatileBehavior_IsEastArrowWarp`
/// (`metatile_behavior.c:288-294`). Arrow warps are not ported as *triggers*
/// (module docs; upstream's separate `TryArrowWarp` path), but they are
/// reachable as warp *destinations*, so
/// [`crate::overworld::warp::warp_in_facing`] must still classify them.
pub const MB_EAST_ARROW_WARP: u8 = 0x62;

/// `MB_WEST_ARROW_WARP` (`0x63`): upstream `MetatileBehavior_IsWestArrowWarp`
/// (`metatile_behavior.c:296-302`) — see [`MB_EAST_ARROW_WARP`].
pub const MB_WEST_ARROW_WARP: u8 = 0x63;

/// `MB_NORTH_ARROW_WARP` (`0x64`): upstream
/// `MetatileBehavior_IsNorthArrowWarp` (`metatile_behavior.c:304-310`) — see
/// [`MB_EAST_ARROW_WARP`].
pub const MB_NORTH_ARROW_WARP: u8 = 0x64;

/// `MB_SOUTH_ARROW_WARP` (`0x65`): upstream
/// `MetatileBehavior_IsSouthArrowWarp` (`metatile_behavior.c:313-319`) — the
/// behavior of the doormat tile a house's front door warps *onto* (e.g.
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`'s warp #1 at `(8, 8)`) — see
/// [`MB_EAST_ARROW_WARP`].
pub const MB_SOUTH_ARROW_WARP: u8 = 0x65;

/// `MB_WATER_DOOR` (`0x6C`): the second id upstream
/// `MetatileBehavior_IsNonAnimDoor` matches (`metatile_behavior.c:262-269`) —
/// see [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`].
pub const MB_WATER_DOOR: u8 = 0x6C;

/// `MB_WATER_SOUTH_ARROW_WARP` (`0x6D`): the second id upstream
/// `MetatileBehavior_IsSouthArrowWarp` matches
/// (`metatile_behavior.c:313-319`) — see [`MB_EAST_ARROW_WARP`].
pub const MB_WATER_SOUTH_ARROW_WARP: u8 = 0x6D;

/// `MB_DEEP_SOUTH_WARP` (`0x6E`): matched by *both*
/// `MetatileBehavior_IsDeepSouthWarp` and `MetatileBehavior_IsNonAnimDoor`
/// upstream (`metatile_behavior.c:262-269, 272-278`); the order those two
/// are tested in is what decides its arrival facing — see
/// [`crate::overworld::warp::warp_in_facing`].
pub const MB_DEEP_SOUTH_WARP: u8 = 0x6E;

/// `MB_PETALBURG_GYM_DOOR` (`0x8D`): the second id upstream
/// `MetatileBehavior_IsDoor` matches (`metatile_behavior.c:228-235`) — see
/// [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`].
pub const MB_PETALBURG_GYM_DOOR: u8 = 0x8D;

/// Whether `behavior` is one of the two door ids this slice ports, mirroring
/// the union of upstream `MetatileBehavior_IsNonAnimDoor` and
/// `MetatileBehavior_IsWarpDoor`/`MetatileBehavior_IsDoor` restricted to the
/// ids this module models (`MB_PETALBURG_GYM_DOOR`, `MB_WATER_DOOR`, and
/// `MB_DEEP_SOUTH_WARP` are not ported — out of the v1 north-star path).
#[must_use]
pub const fn is_door(behavior: u8) -> bool {
    matches!(behavior, MB_NON_ANIMATED_DOOR | MB_ANIMATED_DOOR)
}

/// Whether stepping onto/standing on a tile with this behavior can trigger a
/// warp at all, mirroring the *shape* of upstream `IsWarpMetatileBehavior`
/// (`field_control_avatar.c`) restricted to the behaviors this slice ports.
///
/// Upstream's real `IsWarpMetatileBehavior` also accepts ladders,
/// escalators, and half a dozen map-specific warp ids (Lavaridge gym,
/// Aqua Hideout, Mt. Pyre hole, Mossdeep gym, Union Room) — none of those are
/// on the v1 north-star path, and none are modelled by this module. A
/// [`WarpEvent`](assets::WarpEvent) that exists at a position whose metatile
/// behavior isn't recognized here **fails closed**: [`is_warp_trigger`]
/// returns `false` and the warp does not fire, rather than assuming every
/// `WarpEvent` is reachable regardless of the tile it sits on.
#[must_use]
pub const fn is_warp_trigger(behavior: u8) -> bool {
    is_door(behavior)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_ids_are_recognized() {
        assert_eq!(MB_NON_ANIMATED_DOOR, 0x60);
        assert_eq!(MB_ANIMATED_DOOR, 0x69);
        assert!(is_door(MB_NON_ANIMATED_DOOR));
        assert!(is_door(MB_ANIMATED_DOOR));
        assert!(!is_door(0x65), "MB_SOUTH_ARROW_WARP is not a door");
    }

    #[test]
    fn normal_ground_is_not_a_door() {
        assert!(!is_door(MB_NORMAL));
    }

    #[test]
    fn unported_warp_behaviors_fail_closed() {
        // MB_LADDER (0x61) and MB_UP_ESCALATOR (0x6A): real upstream warp
        // triggers this slice deliberately does not port (see the module
        // docs). They must not be silently treated as a warp trigger.
        const MB_LADDER: u8 = 0x61;
        const MB_UP_ESCALATOR: u8 = 0x6A;
        assert!(!is_warp_trigger(MB_LADDER));
        assert!(!is_warp_trigger(MB_UP_ESCALATOR));
    }

    #[test]
    fn door_behaviors_are_warp_triggers() {
        assert!(is_warp_trigger(MB_NON_ANIMATED_DOOR));
        assert!(is_warp_trigger(MB_ANIMATED_DOOR));
    }

    /// The ids named only for arrival-facing classification
    /// (`crate::overworld::warp::warp_in_facing`) must not widen the trigger
    /// set: naming an id is not making it triggerable (module docs).
    #[test]
    fn arrival_facing_ids_are_not_warp_triggers() {
        for behavior in [
            MB_STAIRS_OUTSIDE_ABANDONED_SHIP,
            MB_SHOAL_CAVE_ENTRANCE,
            MB_EAST_ARROW_WARP,
            MB_WEST_ARROW_WARP,
            MB_NORTH_ARROW_WARP,
            MB_SOUTH_ARROW_WARP,
            MB_WATER_DOOR,
            MB_WATER_SOUTH_ARROW_WARP,
            MB_DEEP_SOUTH_WARP,
            MB_PETALBURG_GYM_DOOR,
        ] {
            assert!(
                !is_warp_trigger(behavior),
                "{behavior:#04x} must stay outside this slice's trigger set"
            );
        }
    }

    /// The ids' numeric values, against their positions in upstream's
    /// `constants/metatile_behaviors.h` enum (which assigns no explicit
    /// values, so each id's number is its index).
    #[test]
    fn arrival_facing_ids_match_upstreams_enum_positions() {
        assert_eq!(MB_STAIRS_OUTSIDE_ABANDONED_SHIP, 0x1B);
        assert_eq!(MB_SHOAL_CAVE_ENTRANCE, 0x1C);
        assert_eq!(MB_EAST_ARROW_WARP, 0x62);
        assert_eq!(MB_WEST_ARROW_WARP, 0x63);
        assert_eq!(MB_NORTH_ARROW_WARP, 0x64);
        assert_eq!(MB_SOUTH_ARROW_WARP, 0x65);
        assert_eq!(MB_WATER_DOOR, 0x6C);
        assert_eq!(MB_WATER_SOUTH_ARROW_WARP, 0x6D);
        assert_eq!(MB_DEEP_SOUTH_WARP, 0x6E);
        assert_eq!(MB_PETALBURG_GYM_DOOR, 0x8D);
    }
}
