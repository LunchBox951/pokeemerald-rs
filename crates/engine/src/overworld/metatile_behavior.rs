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
//! trigger. Everything else this crate doesn't decode is deliberately left
//! unclassified rather than guessed at — see [`is_warp_trigger`]'s doc for
//! how that fail-closed policy plays out for warps specifically.
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

/// `MB_NON_ANIMATED_DOOR` (101): a warp tile with no open/close animation.
/// Upstream uses this for interior staircases and other "just teleport"
/// warps as well as literal unanimated doors (`MetatileBehavior_IsNonAnimDoor`
/// also matches `MB_WATER_DOOR`/`MB_DEEP_SOUTH_WARP`, both out of v1 scope
/// and not ported here). This is the behavior the v1 north-star path's
/// house-interior stairs are assumed to use.
pub const MB_NON_ANIMATED_DOOR: u8 = 101;

/// `MB_ANIMATED_DOOR` (110): a warp tile that plays an open/close animation
/// (`MetatileBehavior_IsWarpDoor`/`MetatileBehavior_IsDoor`) — ordinary
/// building entrance doors. This port has no rendering/animation, so
/// [`is_warp_trigger`] treats it identically to
/// [`MB_NON_ANIMATED_DOOR`]: both trigger a warp once the player's tile
/// matches a [`WarpEvent`](assets::WarpEvent), with no distinction for the
/// door-approach-before-stepping-on-it timing upstream's `TryDoorWarp`
/// applies (see the module docs on that simplification in `crate::overworld::warp`).
pub const MB_ANIMATED_DOOR: u8 = 110;

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
        assert!(is_door(MB_NON_ANIMATED_DOOR));
        assert!(is_door(MB_ANIMATED_DOOR));
    }

    #[test]
    fn normal_ground_is_not_a_door() {
        assert!(!is_door(MB_NORMAL));
    }

    #[test]
    fn unported_warp_behaviors_fail_closed() {
        // MB_LADDER (102) and MB_UP_ESCALATOR (111): real upstream warp
        // triggers this slice deliberately does not port (see the module
        // docs). They must not be silently treated as a warp trigger.
        const MB_LADDER: u8 = 102;
        const MB_UP_ESCALATOR: u8 = 111;
        assert!(!is_warp_trigger(MB_LADDER));
        assert!(!is_warp_trigger(MB_UP_ESCALATOR));
    }

    #[test]
    fn door_behaviors_are_warp_triggers() {
        assert!(is_warp_trigger(MB_NON_ANIMATED_DOOR));
        assert!(is_warp_trigger(MB_ANIMATED_DOOR));
    }
}
