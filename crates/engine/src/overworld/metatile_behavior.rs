//! Metatile behavior classification (S-5), ported from
//! `pokeemerald/src/metatile_behavior.c`.
//!
//! Upstream's `constants/metatile_behaviors.h` defines roughly 200 `MB_*`
//! ids, and `metatile_behavior.c` has a matching `MetatileBehavior_Is*`
//! predicate for most of them (tall grass, ice, currents, ladders,
//! escalators, a dozen map-specific warp variants, secret-base furniture,
//! ...). Per the issue #108 scope (extended by issue #174 for arrow warps),
//! this module ports **only** the predicates the v1 north-star path needs
//! (protagonist's room -> downstairs -> Littleroot outdoor -> Route 101):
//! whether a tile is a door/warp trigger, the direction-gated arrow-warp
//! trigger predicates (upstream `TryArrowWarp`/`IsArrowWarpMetatileBehavior`,
//! `crate::overworld::warp::is_arrow_warp_trigger`), plus the ids upstream
//! `GetAdjustedInitialDirection` (`src/overworld.c:929-951`) branches on to
//! pick the facing a warp *lands* the player in
//! ([`crate::overworld::warp::warp_in_facing`]), plus (issue #169) the
//! land-wild-encounter predicate [`is_land_wild_encounter`] that gates
//! [`crate::overworld::wild_encounter`]'s per-step roll. Everything else this crate
//! doesn't decode is deliberately left unclassified rather than guessed at —
//! see [`is_warp_trigger`]'s doc for how that fail-closed policy plays out
//! for warps specifically.
//!
//! **Naming an id is not automatically a door-shaped trigger.** The
//! door-alias ids below ([`MB_WATER_DOOR`], [`MB_DEEP_SOUTH_WARP`],
//! [`MB_PETALBURG_GYM_DOOR`]) exist only because a warp *destination* tile
//! can carry them (a front door lands you on a `MB_SOUTH_ARROW_WARP`
//! doormat), and the arrival facing depends on which one it is —
//! [`is_warp_trigger`]'s membership is unchanged by their presence: they
//! still cannot *start* a door-shaped warp here. The arrow-warp ids
//! ([`MB_NORTH_ARROW_WARP`], [`MB_SOUTH_ARROW_WARP`],
//! [`MB_WEST_ARROW_WARP`], [`MB_EAST_ARROW_WARP`], and their
//! [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`]/[`MB_SHOAL_CAVE_ENTRANCE`]/
//! [`MB_WATER_SOUTH_ARROW_WARP`] aliases) are different: they *are* warp
//! triggers, just gated on facing/holding the matching direction rather than
//! [`is_warp_trigger`]'s unconditional membership test — see
//! [`is_north_arrow_warp`] and its siblings, and
//! [`crate::overworld::warp::is_arrow_warp_trigger`] which dispatches
//! between them by direction.
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

/// `MB_TALL_GRASS` (`0x02`): the grass a wild encounter is rolled in — the
/// tile the v1 north-star path's first encounter happens on (issue #169).
/// See [`is_land_wild_encounter`].
pub const MB_TALL_GRASS: u8 = 0x02;

/// `MB_LONG_GRASS` (`0x03`): the taller grass variant (Routes 119/120), also
/// a land-encounter tile — see [`is_land_wild_encounter`].
pub const MB_LONG_GRASS: u8 = 0x03;

/// `MB_UNUSED_05` (`0x05`): an id upstream still flags
/// `TILE_FLAG_HAS_ENCOUNTERS` in `sTileBitAttributes`
/// (`metatile_behavior.c:14`) despite its name — carried so
/// [`is_land_wild_encounter`] reproduces upstream's id set exactly rather
/// than a tidied-up version of it.
pub const MB_UNUSED_05: u8 = 0x05;

/// `MB_DEEP_SAND` (`0x06`): the deep desert sand wild Pokémon appear in — a
/// land-encounter tile (`metatile_behavior.c:15`).
pub const MB_DEEP_SAND: u8 = 0x06;

/// `MB_CAVE` (`0x08`): cave floor — a land-encounter tile
/// (`metatile_behavior.c:17`).
pub const MB_CAVE: u8 = 0x08;

/// `MB_INDOOR_ENCOUNTER` (`0x0B`): indoor floor that still rolls encounters
/// (`metatile_behavior.c:20`) — the Battle Pyramid's rooms.
pub const MB_INDOOR_ENCOUNTER: u8 = 0x0B;

/// `MB_ASHGRASS` (`0x24`): Route 113's ash-covered grass — a land-encounter
/// tile (`metatile_behavior.c:40`).
pub const MB_ASHGRASS: u8 = 0x24;

/// `MB_FOOTPRINTS` (`0x25`): the soft desert ground that records footprints —
/// a land-encounter tile (`metatile_behavior.c:41`).
pub const MB_FOOTPRINTS: u8 = 0x25;

/// `MB_NON_ANIMATED_DOOR` (`0x60`): a warp tile with no open/close animation.
/// Upstream uses this for interior staircases and other "just teleport"
/// warps as well as literal unanimated doors (`MetatileBehavior_IsNonAnimDoor`
/// also matches [`MB_WATER_DOOR`]/[`MB_DEEP_SOUTH_WARP`], modeled below only
/// for arrival-facing classification — see [`is_door`]). This is the behavior
/// the v1 north-star path's house-interior stairs are assumed to use.
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
/// (`metatile_behavior.c:304-310`) — so [`is_north_arrow_warp`] matches it
/// too, and it is a warp *trigger* when facing/holding North (issue #174),
/// even though it is **not** a [`is_warp_trigger`] id (that predicate is
/// door-shaped only; see its own doc).
pub const MB_STAIRS_OUTSIDE_ABANDONED_SHIP: u8 = 0x1B;

/// `MB_SHOAL_CAVE_ENTRANCE` (`0x1C`): the third id upstream
/// `MetatileBehavior_IsSouthArrowWarp` matches
/// (`metatile_behavior.c:313-319`) — see [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`]
/// for the North/South-arrow-alias shape; this one is [`is_south_arrow_warp`]'s.
pub const MB_SHOAL_CAVE_ENTRANCE: u8 = 0x1C;

/// `MB_EAST_ARROW_WARP` (`0x62`): upstream `MetatileBehavior_IsEastArrowWarp`
/// (`metatile_behavior.c:288-294`). A direction-gated warp trigger
/// ([`is_east_arrow_warp`], consumed by
/// [`crate::overworld::warp::is_arrow_warp_trigger`] — upstream
/// `TryArrowWarp`/`IsArrowWarpMetatileBehavior`, issue #174) and, independent
/// of that, still reachable as a warp *destination*, so
/// [`crate::overworld::warp::warp_in_facing`] classifies it too.
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
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`'s warp #1 at `(8, 8)`, whose
/// arrow trigger is what lets the player leave through the front door again
/// — issue #174) — see [`MB_EAST_ARROW_WARP`].
pub const MB_SOUTH_ARROW_WARP: u8 = 0x65;

/// `MB_WATER_DOOR` (`0x6C`): the second id upstream
/// `MetatileBehavior_IsNonAnimDoor` matches (`metatile_behavior.c:262-269`) —
/// a door-alias only, matched by neither [`is_warp_trigger`] nor any
/// arrow-warp predicate; its only v1 consumer is
/// [`crate::overworld::warp::warp_in_facing`].
pub const MB_WATER_DOOR: u8 = 0x6C;

/// `MB_WATER_SOUTH_ARROW_WARP` (`0x6D`): the second id upstream
/// `MetatileBehavior_IsSouthArrowWarp` matches
/// (`metatile_behavior.c:313-319`) — see [`MB_EAST_ARROW_WARP`]; matched by
/// [`is_south_arrow_warp`] like [`MB_SOUTH_ARROW_WARP`] itself.
pub const MB_WATER_SOUTH_ARROW_WARP: u8 = 0x6D;

/// `MB_DEEP_SOUTH_WARP` (`0x6E`): matched by *both*
/// `MetatileBehavior_IsDeepSouthWarp` and `MetatileBehavior_IsNonAnimDoor`
/// upstream (`metatile_behavior.c:262-269, 272-278`); the order those two
/// are tested in is what decides its arrival facing — see
/// [`crate::overworld::warp::warp_in_facing`]. A door-alias only, like
/// [`MB_WATER_DOOR`] — not matched by any arrow-warp predicate.
pub const MB_DEEP_SOUTH_WARP: u8 = 0x6E;

/// `MB_PETALBURG_GYM_DOOR` (`0x8D`): the second id upstream
/// `MetatileBehavior_IsDoor` matches (`metatile_behavior.c:228-235`) — a
/// door-alias only, like [`MB_WATER_DOOR`].
pub const MB_PETALBURG_GYM_DOOR: u8 = 0x8D;

/// Whether `behavior` is one of the two door ids that act as warp *triggers*
/// in this slice, mirroring the union of upstream
/// `MetatileBehavior_IsNonAnimDoor` and
/// `MetatileBehavior_IsWarpDoor`/`MetatileBehavior_IsDoor` restricted to those
/// trigger ids. The functions' remaining ids — [`MB_PETALBURG_GYM_DOOR`],
/// [`MB_WATER_DOOR`], and [`MB_DEEP_SOUTH_WARP`] — are modeled as constants
/// but deliberately excluded here: their only v1 consumer is
/// [`crate::overworld::warp::warp_in_facing`]'s arrival-facing
/// classification, and a test pins them outside [`is_warp_trigger`].
#[must_use]
pub const fn is_door(behavior: u8) -> bool {
    matches!(behavior, MB_NON_ANIMATED_DOOR | MB_ANIMATED_DOOR)
}

/// Whether stepping onto/standing on a tile with this behavior can trigger a
/// **door-shaped** warp (i.e. one that fires regardless of the player's
/// facing/held direction), mirroring the *shape* of upstream
/// `IsWarpMetatileBehavior` (`field_control_avatar.c`) restricted to the
/// behaviors this slice ports.
///
/// Upstream's real `IsWarpMetatileBehavior` also accepts ladders,
/// escalators, and half a dozen map-specific warp ids (Lavaridge gym,
/// Aqua Hideout, Mt. Pyre hole, Mossdeep gym, Union Room) — none of those are
/// on the v1 north-star path, and none are modelled by this module. A
/// [`WarpEvent`](assets::WarpEvent) that exists at a position whose metatile
/// behavior isn't recognized here **fails closed**: [`is_warp_trigger`]
/// returns `false` and the door-shaped warp does not fire, rather than
/// assuming every `WarpEvent` is reachable regardless of the tile it sits
/// on.
///
/// **Arrow warps are a separate predicate, not a subset of this one.**
/// Upstream keeps `IsWarpMetatileBehavior` (this function's model, consumed
/// by `TryStartWarpEventScript`) and `IsArrowWarpMetatileBehavior` (consumed
/// by `TryArrowWarp`) as two independent checks with disjoint id sets and
/// different gating (unconditional here vs. direction-matched there) — see
/// [`is_north_arrow_warp`]/[`is_south_arrow_warp`]/[`is_west_arrow_warp`]/
/// [`is_east_arrow_warp`] and [`crate::overworld::warp::is_arrow_warp_trigger`],
/// which callers must check in addition to this function to reproduce
/// upstream's full warp-trigger surface (issue #174).
#[must_use]
pub const fn is_warp_trigger(behavior: u8) -> bool {
    is_door(behavior)
}

/// Whether standing on a tile with `behavior` can roll a **land** wild
/// encounter — upstream `MetatileBehavior_IsLandWildEncounter`
/// (`metatile_behavior.c:819-826`), the gate `StandardWildEncounter`
/// (`wild_encounter.c:595`) checks before anything else on the land path
/// (issue #169).
///
/// Upstream composes it from two table lookups:
/// `MetatileBehavior_IsEncounterTile` (`:135-142`, `TILE_FLAG_HAS_ENCOUNTERS`)
/// AND NOT `MetatileBehavior_IsSurfableWaterOrUnderwater` (`:280-287`,
/// `TILE_FLAG_SURFABLE`), both indexing the ~200-entry `sTileBitAttributes`
/// array (`:9`). Re-expressed here as the *derived* id set `(no-verbatim)`
/// rather than transcribing that whole array for one predicate: exactly the
/// eight ids upstream flags `TILE_FLAG_HAS_ENCOUNTERS` without also flagging
/// `TILE_FLAG_SURFABLE` — [`MB_TALL_GRASS`], [`MB_LONG_GRASS`],
/// [`MB_UNUSED_05`], [`MB_DEEP_SAND`], [`MB_CAVE`], [`MB_INDOOR_ENCOUNTER`],
/// [`MB_ASHGRASS`], [`MB_FOOTPRINTS`] (`:12-20`, `:40-41`). The set is
/// complete, not a v1 subset: every land-encounter behavior in the game is
/// here, so a future map needs no change to this predicate.
///
/// The *surfable* half of that table — `MetatileBehavior_IsWaterWildEncounter`
/// (`:828-835`) and its six encounter-flagged water ids — is deliberately not
/// modelled: surfing does not exist in this port, so a water encounter is
/// unreachable and a predicate for it would be an untested claim. Water
/// encounters are enumerated as NOT-modelled on this slice's ledger entry
/// rather than approximated here.
#[must_use]
pub const fn is_land_wild_encounter(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_TALL_GRASS
            | MB_LONG_GRASS
            | MB_UNUSED_05
            | MB_DEEP_SAND
            | MB_CAVE
            | MB_INDOOR_ENCOUNTER
            | MB_ASHGRASS
            | MB_FOOTPRINTS
    )
}

/// Whether `behavior` is a tile upstream `TryArrowWarp` treats as an
/// east-facing arrow warp — `MetatileBehavior_IsEastArrowWarp`
/// (`metatile_behavior.c:288-294`).
#[must_use]
pub const fn is_east_arrow_warp(behavior: u8) -> bool {
    behavior == MB_EAST_ARROW_WARP
}

/// `MetatileBehavior_IsWestArrowWarp` (`metatile_behavior.c:296-302`) — see
/// [`is_east_arrow_warp`].
#[must_use]
pub const fn is_west_arrow_warp(behavior: u8) -> bool {
    behavior == MB_WEST_ARROW_WARP
}

/// `MetatileBehavior_IsNorthArrowWarp` (`metatile_behavior.c:304-310`) — see
/// [`is_east_arrow_warp`]. Matches [`MB_NORTH_ARROW_WARP`] and its
/// [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`] alias, exactly as upstream does.
#[must_use]
pub const fn is_north_arrow_warp(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_NORTH_ARROW_WARP | MB_STAIRS_OUTSIDE_ABANDONED_SHIP
    )
}

/// `MetatileBehavior_IsSouthArrowWarp` (`metatile_behavior.c:313-319`) — see
/// [`is_east_arrow_warp`]. Matches [`MB_SOUTH_ARROW_WARP`] and its
/// [`MB_WATER_SOUTH_ARROW_WARP`]/[`MB_SHOAL_CAVE_ENTRANCE`] aliases, exactly
/// as upstream does — the doormat a house's front door lands on
/// (`MB_SOUTH_ARROW_WARP`'s own doc) is a south arrow warp under this
/// predicate.
#[must_use]
pub const fn is_south_arrow_warp(behavior: u8) -> bool {
    matches!(
        behavior,
        MB_SOUTH_ARROW_WARP | MB_WATER_SOUTH_ARROW_WARP | MB_SHOAL_CAVE_ENTRANCE
    )
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

    /// `is_warp_trigger` is the **door-shaped** trigger predicate only
    /// (upstream `IsWarpMetatileBehavior`) — none of the arrow-warp ids (nor
    /// their door-alias siblings) widen it, even though the arrow ids among
    /// them *are* triggers under the separate direction-gated
    /// `is_north_arrow_warp`-family predicates (updated for issue #174: this
    /// test used to pin the whole list outside *any* warp trigger, back when
    /// arrow warps were not ported at all — see
    /// `arrow_warp_predicates_match_only_their_own_direction` below for what
    /// replaced that claim for the arrow ids specifically).
    #[test]
    fn arrival_facing_only_ids_are_not_door_shaped_warp_triggers() {
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
                "{behavior:#04x} must stay outside is_warp_trigger's door-shaped set"
            );
        }
    }

    /// The door-alias-only ids ([`MB_WATER_DOOR`], [`MB_DEEP_SOUTH_WARP`],
    /// [`MB_PETALBURG_GYM_DOOR`]) must also stay outside every arrow-warp
    /// predicate in every direction — unlike [`MB_STAIRS_OUTSIDE_ABANDONED_SHIP`]
    /// and [`MB_SHOAL_CAVE_ENTRANCE`], which alias into the North/South arrow
    /// predicates respectively (the next test).
    #[test]
    fn door_alias_ids_match_no_arrow_warp_predicate() {
        for behavior in [MB_WATER_DOOR, MB_DEEP_SOUTH_WARP, MB_PETALBURG_GYM_DOOR] {
            assert!(!is_north_arrow_warp(behavior));
            assert!(!is_south_arrow_warp(behavior));
            assert!(!is_west_arrow_warp(behavior));
            assert!(!is_east_arrow_warp(behavior));
        }
    }

    /// Each arrow-warp predicate matches only its own direction's ids
    /// (including the North/South aliases), mirroring upstream's four
    /// separate `MetatileBehavior_Is*ArrowWarp` functions
    /// (`metatile_behavior.c:288-319`) — issue #174.
    #[test]
    fn arrow_warp_predicates_match_only_their_own_direction() {
        assert!(is_east_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(is_west_arrow_warp(MB_WEST_ARROW_WARP));
        assert!(is_north_arrow_warp(MB_NORTH_ARROW_WARP));
        assert!(is_north_arrow_warp(MB_STAIRS_OUTSIDE_ABANDONED_SHIP));
        assert!(is_south_arrow_warp(MB_SOUTH_ARROW_WARP));
        assert!(is_south_arrow_warp(MB_WATER_SOUTH_ARROW_WARP));
        assert!(is_south_arrow_warp(MB_SHOAL_CAVE_ENTRANCE));

        // Cross-direction: none of the four id groups match a predicate
        // that isn't theirs.
        assert!(!is_west_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(!is_north_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(!is_south_arrow_warp(MB_EAST_ARROW_WARP));
        assert!(!is_east_arrow_warp(MB_WEST_ARROW_WARP));
        assert!(!is_south_arrow_warp(MB_NORTH_ARROW_WARP));
        assert!(!is_north_arrow_warp(MB_SOUTH_ARROW_WARP));

        // Ordinary ground and an unported warp behavior match nothing.
        assert!(!is_east_arrow_warp(MB_NORMAL));
        assert!(!is_north_arrow_warp(MB_NORMAL));
        assert!(!is_south_arrow_warp(MB_NORMAL));
        assert!(!is_west_arrow_warp(MB_NORMAL));
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

    /// The land-encounter ids' numeric values, against their positions in
    /// upstream's `constants/metatile_behaviors.h` enum — the same
    /// index-is-the-value reasoning as the test above.
    #[test]
    fn land_encounter_ids_match_upstreams_enum_positions() {
        assert_eq!(MB_TALL_GRASS, 0x02);
        assert_eq!(MB_LONG_GRASS, 0x03);
        assert_eq!(MB_UNUSED_05, 0x05);
        assert_eq!(MB_DEEP_SAND, 0x06);
        assert_eq!(MB_CAVE, 0x08);
        assert_eq!(MB_INDOOR_ENCOUNTER, 0x0B);
        assert_eq!(MB_ASHGRASS, 0x24);
        assert_eq!(MB_FOOTPRINTS, 0x25);
    }

    /// `MetatileBehavior_IsLandWildEncounter` (`metatile_behavior.c:819-826`)
    /// over the *whole* `u8` id space: exactly the eight ids upstream flags
    /// `TILE_FLAG_HAS_ENCOUNTERS` without `TILE_FLAG_SURFABLE` match, and
    /// nothing else does. Exhaustive rather than sampled, so a future id
    /// added to the set cannot slip in unnoticed.
    #[test]
    fn only_upstreams_eight_non_surfable_encounter_ids_are_land_encounters() {
        let expected = [
            MB_TALL_GRASS,
            MB_LONG_GRASS,
            MB_UNUSED_05,
            MB_DEEP_SAND,
            MB_CAVE,
            MB_INDOOR_ENCOUNTER,
            MB_ASHGRASS,
            MB_FOOTPRINTS,
        ];
        let matching: Vec<u8> = (0..=u8::MAX)
            .filter(|b| is_land_wild_encounter(*b))
            .collect();
        assert_eq!(matching, expected);
    }

    /// The two neighbours most likely to be confused with grass: ordinary
    /// ground, `MB_SHORT_GRASS` (`0x07`, decorative — upstream gives it
    /// `TILE_FLAG_UNUSED` only, `metatile_behavior.c:16`), and
    /// `MB_LONG_GRASS_SOUTH_EDGE` (`0x09`, the long-grass border tile, also
    /// encounter-free at `:18`). None of the three rolls an encounter.
    #[test]
    fn decorative_grass_neighbours_do_not_roll_encounters() {
        assert!(!is_land_wild_encounter(MB_NORMAL));
        assert!(!is_land_wild_encounter(0x07), "MB_SHORT_GRASS");
        assert!(!is_land_wild_encounter(0x09), "MB_LONG_GRASS_SOUTH_EDGE");
        // Surfable water is encounter-flagged upstream but excluded by the
        // `IsSurfableWaterOrUnderwater` half of the predicate.
        assert!(!is_land_wild_encounter(0x10), "MB_POND_WATER");
        assert!(!is_land_wild_encounter(0x12), "MB_DEEP_WATER");
    }
}
