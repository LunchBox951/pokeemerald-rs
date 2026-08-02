//! `pokeemerald/src/tileset_anims.c`'s per-*primary*-tileset VRAM tile
//! animation cadence (issue #160): which raw combined-tileset tile ranges
//! [`super::OverworldScene::compose`] overwrites with which extracted
//! `tileset/<name>/anim/<anim-name>/<n>` frame, on which schedule.
//!
//! # Scope
//!
//! Every tileset this port bundles (`crates/xtask/src/extract/mod.rs`'s
//! `TILESETS`) that actually carries an `anim/` directory is a *primary*
//! tileset -- `general` and `building` -- confirmed against upstream's own
//! tileset table (`pokeemerald/src/data/tilesets/headers.h`): both declare
//! `.isSecondary = FALSE` and a non-`NULL` `.callback`
//! (`InitTilesetAnim_General`/`InitTilesetAnim_Building`), while the three
//! secondary tilesets this port bundles (`petalburg`, `brendans_mays_house`,
//! `lab`) either have no `anim/` directory at all or a `NULL` callback --
//! `gTileset_Petalburg`'s `InitTilesetAnim_Petalburg` only syncs the
//! secondary counter and then assigns `sSecondaryTilesetAnimCallback =
//! NULL`, so no secondary callback ever runs for it
//! (`tileset_anims.c:676-681`) -- so neither has any observable animation.
//! Every animated range this slice ports therefore targets the room's
//! *primary* tile block -- combined-tileset indices `0..512`
//! ([`super::viewport`]'s own `NUM_TILES_IN_PRIMARY`), never the
//! secondary's `512..` block -- and [`AnimatedTileset::load`] only ever
//! looks at the room's `primary_tileset` name.
//!
//! Every other upstream tileset animation (Rustboro's fountain, Mauville's
//! flowers, Sootopolis's stormy water, the palette-blend-only Battle Dome
//! floor lights, ...) targets a tileset outside the five this port
//! extracts, and is out of scope with them (parent module docs).
//!
//! # The cadence, as a pure function of an elapsed-tick counter
//!
//! `tileset_anims.c` itself is stateful and imperative: a per-room counter
//! increments once every real (rendered) frame (`UpdateTilesetAnimations`),
//! and each region's own callback -- e.g. `QueueAnimTiles_General_Flower`
//! -- fires only on the frames where the counter satisfies `counter %
//! interval == phase`, queuing a DMA transfer of that step's frame into
//! VRAM; the previous frame's pixels simply stay in VRAM (an implicit
//! "hold") on every frame in between `(no-verbatim)`.
//!
//! Since [`super::OverworldScene::compose`] is a pure function of an
//! explicit `tick` (mirroring [`crate::title::TitleScene::compose`]'s own
//! `frame` parameter), this module reproduces the same *observable*
//! sequence with a pure function instead: [`latched_frame`] computes,
//! given `tick`, which frame a region last had queued (or `None` if the
//! room hasn't been open long enough for that region to have fired even
//! once -- see its own doc comment). No DMA queue, no stored "current
//! frame" state, and no dependency on `sPrimaryTilesetAnimCounterMax`
//! (256) -- see [`latched_frame`]'s doc comment for why dropping that
//! wraparound is still exactly behaviorally faithful for every region this
//! slice covers.
//!
//! `tick` itself is [`crate::flow::OverworldPhase`]'s own frame counter,
//! reset to 0 exactly when upstream's `InitTilesetAnimations` runs -- every
//! fresh room load (`OverworldPhase::load_default`) and every warp
//! (`OverworldPhase::warp_to`) -- matching `InitMapView`/`overworld.c`'s
//! own `InitTilesetAnimations` call sites, all of which are map (re)loads.
//!
//! # Data pinned against `tileset_anims.c`
//!
//! | region | `anim_name` | start tile | tiles | phase | interval | sequence | source lines |
//! |---|---|---|---|---|---|---|---|
//! | General flower | `flower` | 508 | 4 | 0 | 16 | `[0,1,0,2]` | `:82-87,634-635,652-656` |
//! | General water | `water` | 432 | 30 | 1 | 16 | `[0,1,2,3,4,5,6,7]` | `:89-107,636-637,658-662` |
//! | General sand/water edge | `sand_water_edge` | 464 | 10 | 2 | 16 | `[0,1,2,3,4,5,6,0]` | `:109-126,638-639,664-668` |
//! | General waterfall | `waterfall` | 496 | 6 | 3 | 16 | `[0,1,2,3]` | `:128-138,640-641,670-674` |
//! | General land/water edge | `land_water_edge` | 480 | 10 | 4 | 16 | `[0,1,2,3]` | `:140-150,642-643,958-962` |
//! | Building TV turned on | `tv_turned_on` | 496 | 4 | 0 | 8 | `[0,1]` | `:424-428,648-649,1113-1117` |
//!
//! The `tiles` column is each region's own upstream copy length -- the `N`
//! in `QueueAnimTiles_*`'s `... , N * TILE_SIZE_4BPP)` call -- the one
//! constant the DMA transfer sizes itself by, so [`AnimatedTileset::load`]
//! requires every extracted frame to be exactly that many tiles.
//!
//! (`General`'s own per-tick dispatch: `tileset_anims.c:632-644`;
//! `Building`'s: `:646-650`. Line numbers above are relative to that same
//! file.)

use assets::AssetPack;
use rendering::BitDepth;

use super::{pack_4bpp_region, OverworldSceneError};

/// One upstream `QueueAnimTiles_*` region's static shape (module docs'
/// table) -- everything [`AnimatedTileset::load`] needs to fetch and patch
/// it, short of the pack itself.
struct AnimRegionSpec {
    /// The pack id segment under `tileset/<tileset>/anim/<anim_name>/<n>`.
    anim_name: &'static str,
    /// Combined-tileset tile index the region's frames overwrite --
    /// upstream's own `TILE_OFFSET_4BPP(N)` literal (module docs: never
    /// `NUM_TILES_IN_PRIMARY`-shifted, since every region here targets the
    /// primary block).
    start_tile: u16,
    /// `timer % interval == phase` -- which of upstream's per-tick dispatch
    /// arms this region is (`TilesetAnim_General`/`TilesetAnim_Building`'s
    /// own `if` chain).
    phase: u16,
    /// See [`AnimRegionSpec::phase`].
    interval: u16,
    /// Upstream's copy length for this region in 8x8 tiles -- the `N` in
    /// `QueueAnimTiles_*`'s `N * TILE_SIZE_4BPP` argument (module docs'
    /// table). Every frame must be exactly this many tiles, or
    /// [`AnimatedTileset::load`] rejects the pack.
    tiles: u16,
    /// Upstream's own frame-pointer array, transcribed as the numbered
    /// `anim_name` pack entry each array slot names (module docs' table) --
    /// e.g. General's flower array repeats frame 0 between frames 1 and 2.
    sequence: &'static [u8],
}

const GENERAL_REGIONS: [AnimRegionSpec; 5] = [
    AnimRegionSpec {
        anim_name: "flower",
        start_tile: 508,
        phase: 0,
        interval: 16,
        tiles: 4,
        sequence: &[0, 1, 0, 2],
    },
    AnimRegionSpec {
        anim_name: "water",
        start_tile: 432,
        phase: 1,
        interval: 16,
        tiles: 30,
        sequence: &[0, 1, 2, 3, 4, 5, 6, 7],
    },
    AnimRegionSpec {
        anim_name: "sand_water_edge",
        start_tile: 464,
        phase: 2,
        interval: 16,
        tiles: 10,
        sequence: &[0, 1, 2, 3, 4, 5, 6, 0],
    },
    AnimRegionSpec {
        anim_name: "waterfall",
        start_tile: 496,
        phase: 3,
        interval: 16,
        tiles: 6,
        sequence: &[0, 1, 2, 3],
    },
    AnimRegionSpec {
        anim_name: "land_water_edge",
        start_tile: 480,
        phase: 4,
        interval: 16,
        tiles: 10,
        sequence: &[0, 1, 2, 3],
    },
];

const BUILDING_REGIONS: [AnimRegionSpec; 1] = [AnimRegionSpec {
    anim_name: "tv_turned_on",
    start_tile: 496,
    phase: 0,
    interval: 8,
    tiles: 4,
    sequence: &[0, 1],
}];

/// One [`AnimRegionSpec`] with its frames already decoded/packed from the
/// pack -- `sequence.len()` packed 4bpp byte buffers, one per array slot,
/// in upstream's own array order.
///
/// `#[derive(Debug)]` (rather than hand-rolled, which would just print the
/// same byte vectors some other way): [`super::OverworldScene`] derives
/// `Debug` and embeds an [`AnimatedTileset`], so this has to too.
#[derive(Debug)]
struct Region {
    start_tile: u16,
    phase: u32,
    interval: u32,
    /// `frames[i]` is `sequence[i]`'s packed 4bpp bytes (module docs).
    frames: Vec<Vec<u8>>,
}

/// A room's own primary-tileset tile animations, decoded once
/// ([`Self::load`]) and re-applied every [`super::OverworldScene::compose`]
/// call ([`Self::patch`]).
#[derive(Debug)]
pub(super) struct AnimatedTileset {
    regions: Vec<Region>,
}

impl AnimatedTileset {
    /// Decode every animated region [`GENERAL_REGIONS`]/[`BUILDING_REGIONS`]
    /// declares for `primary_tileset_name` -- empty (a no-op
    /// [`Self::patch`]) for any other tileset name, which covers every
    /// secondary tileset this port bundles (module docs).
    ///
    /// # Errors
    ///
    /// Whatever [`AssetPack::image`]/[`pack_4bpp_region`] returns for a
    /// missing or malformed frame entry, plus
    /// [`OverworldSceneError::AnimFrameSizeMismatch`] for a frame whose
    /// packed bytes are not exactly its region's upstream copy length
    /// (module docs' `tiles` column) -- rejected here so [`Self::patch`]'s
    /// in-place writes can never slice out of bounds or leave a region
    /// partially stale mid-compose. Neither is reachable against a real
    /// pack built by `cargo xtask extract` (module docs' table is pinned
    /// against the same `tileset_anims.c` the extraction pipeline reads).
    pub(super) fn load(
        pack: &AssetPack,
        primary_tileset_name: &str,
    ) -> Result<Self, OverworldSceneError> {
        let specs: &[AnimRegionSpec] = match primary_tileset_name {
            "general" => &GENERAL_REGIONS,
            "building" => &BUILDING_REGIONS,
            _ => &[],
        };
        let mut regions = Vec::with_capacity(specs.len());
        for spec in specs {
            // `latched_frame`'s own derivation assumes upstream's dispatch
            // shape (`timer % interval == phase`), which is only ever
            // satisfiable for `phase < interval`; a table typo the other way
            // round would silently produce a region that never fires.
            debug_assert!(
                spec.phase < spec.interval,
                "{}: phase {} must be less than interval {} (module docs' table)",
                spec.anim_name,
                spec.phase,
                spec.interval
            );
            // With frame sizes pinned to `tiles` below, this is the only
            // remaining way a table typo could send `patch` out of the
            // primary block `viewport::combined_world_tileset` pads to.
            debug_assert!(
                usize::from(spec.start_tile) + usize::from(spec.tiles)
                    <= super::viewport::NUM_TILES_IN_PRIMARY,
                "{}: tiles {}..{} must stay inside the primary block",
                spec.anim_name,
                spec.start_tile,
                spec.start_tile + spec.tiles
            );
            let mut frames = Vec::with_capacity(spec.sequence.len());
            for &n in spec.sequence {
                let id = format!("tileset/{primary_tileset_name}/anim/{}/{n}", spec.anim_name);
                let image = pack.image(&id)?;
                let packed = pack_4bpp_region(
                    "tileset/anim",
                    image,
                    0,
                    0,
                    image.width as usize,
                    image.height as usize,
                )?;
                // A corrupt or hand-built pack can supply a tile-aligned
                // frame whose dimensions and payload agree with each other
                // but not with the region: reject anything that isn't
                // exactly upstream's own copy length (the spec's `tiles`
                // column) rather than let `patch` write a wrong-size
                // region -- oversized frames would spill into a
                // neighboring region's tiles (or past the primary block,
                // panicking mid-compose), undersized ones would leave part
                // of the region stale (review findings on #192).
                let expected = usize::from(spec.tiles) * BitDepth::Bpp4.tile_byte_len();
                if packed.len() != expected {
                    return Err(OverworldSceneError::AnimFrameSizeMismatch {
                        anim_name: spec.anim_name,
                        expected_tiles: spec.tiles,
                        frame_bytes: packed.len(),
                    });
                }
                frames.push(packed);
            }
            regions.push(Region {
                start_tile: spec.start_tile,
                phase: u32::from(spec.phase),
                interval: u32::from(spec.interval),
                frames,
            });
        }
        Ok(Self { regions })
    }

    /// Whether this room has no animated regions at all -- true for every
    /// primary tileset outside [`GENERAL_REGIONS`]/[`BUILDING_REGIONS`]
    /// (module docs), where [`Self::patch`] is a guaranteed no-op and
    /// [`super::OverworldScene::compose`] can skip its whole per-frame
    /// copy/patch/decode pass.
    pub(super) fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Overwrite every animated region's tile range in `bytes` (a combined
    /// primary+secondary tileset's packed 4bpp bytes, [`super::viewport`]'s
    /// `combined_world_tileset` -- module docs: every region here only ever
    /// touches the primary block, `0..512`) with the frame
    /// [`latched_frame`] selects for `tick`, leaving a region untouched if
    /// it hasn't fired yet.
    ///
    /// # Panics
    ///
    /// Never: [`Self::load`] pins every frame to exactly its region's
    /// `tiles * `[`BitDepth::Bpp4.tile_byte_len()`](BitDepth::tile_byte_len)
    /// bytes ([`OverworldSceneError::AnimFrameSizeMismatch`]), every
    /// region's `start_tile + tiles` stays inside the primary block
    /// (`load`'s own debug assertion), and `bytes` is always at least that
    /// long (`viewport`'s `combined_world_tileset` pads the primary block
    /// to exactly `NUM_TILES_IN_PRIMARY` tiles).
    pub(super) fn patch(&self, bytes: &mut [u8], tick: u32) {
        let tile_len = BitDepth::Bpp4.tile_byte_len();
        for region in &self.regions {
            let Some(index) = latched_frame(
                u64::from(tick),
                u64::from(region.phase),
                u64::from(region.interval),
                region.frames.len(),
            ) else {
                continue;
            };
            let frame = &region.frames[index];
            let start = usize::from(region.start_tile) * tile_len;
            bytes[start..start + frame.len()].copy_from_slice(frame);
        }
    }
}

/// Which of a region's frames (index into its own `sequence`) is latched at
/// `tick`, or `None` if this region has never fired by `tick` (module docs:
/// the room has been open for fewer than `phase` ticks -- or, if `phase ==
/// 0`, fewer than `interval` ticks).
///
/// # Derivation
///
/// Upstream's own dispatch (`TilesetAnim_General`/`_Building`) fires this
/// region on real (rendered) frame `n` (`n = 1, 2, 3, ...` -- upstream's
/// `UpdateTilesetAnimations` increments its counter *before* invoking the
/// callback, so the callback's own argument starts at 1, never 0) whenever
/// `n % interval == phase`, queuing frame `(n / interval) %
/// ARRAY_COUNT(...)` (`QueueAnimTiles_*`'s own shape -- its `timer`
/// argument there is already upstream's `counter / interval`). Between
/// fires, the most recently queued frame simply stays in VRAM. So the frame
/// visible at this port's own `tick` (module docs: `tick` counts real
/// frames the same way, starting from 0 at room load, one *before*
/// upstream's first callback argument) is whichever `n <= tick` most
/// recently satisfied that condition -- the largest such `n`, computed
/// directly below rather than searched for.
///
/// This ignores `sPrimaryTilesetAnimCounterMax` (always 256,
/// `InitTilesetAnim_General`/`_Building`) entirely -- upstream wraps its
/// counter back to 0 there, but every region this slice covers has
/// `interval * frame_count` (its own full cycle length) an exact divisor of
/// 256 (16*4=64, 16*8=128, 16*4=64, 16*4=64, 8*2=16 -- module docs' table),
/// so the wrapped and unwrapped sequences are pixel-for-pixel identical at
/// every tick; modeling the wrap would only add a modulus with no
/// observable effect for any region here.
fn latched_frame(tick: u64, phase: u64, interval: u64, frame_count: usize) -> Option<usize> {
    let first_fire = if phase == 0 { interval } else { phase };
    if tick < first_fire {
        return None;
    }
    let last_fire = phase + interval * ((tick - phase) / interval);
    #[allow(clippy::cast_possible_truncation)] // `frame_count` is always a small const array len.
    Some(((last_fire / interval) % frame_count as u64) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- The cadence tables themselves ---------------------------------------

    /// Every field of every [`AnimRegionSpec`] this module ships, against
    /// literals transcribed straight out of `tileset_anims.c` (module docs'
    /// table names the exact source lines for each row) rather than read
    /// back out of the constants under test. Without this, only
    /// `flower.start_tile` was load-bearing anywhere -- a typo in any other
    /// region's start tile, phase, interval, or frame order would render
    /// wrong pixels with every test still green, since no bundled map puts
    /// those regions on screen (`super::super::tests`' own real-pack
    /// coverage notes).
    #[test]
    fn every_region_spec_matches_the_transcribed_tileset_anims_c_table() {
        /// `(anim_name, start_tile, tiles, phase, interval, sequence)`.
        type Row = (&'static str, u16, u16, u16, u16, &'static [u8]);

        // `QueueAnimTiles_General_*`'s own `TILE_OFFSET_4BPP(N)` literals,
        // their `N * TILE_SIZE_4BPP` copy lengths, and
        // `gTilesetAnims_General_*` array contents, plus `TilesetAnim_General`'s
        // per-phase dispatch arms (`:632-644`); Building's are `:646-650`.
        let expected: [Row; 5] = [
            ("flower", 508, 4, 0, 16, &[0, 1, 0, 2]),
            ("water", 432, 30, 1, 16, &[0, 1, 2, 3, 4, 5, 6, 7]),
            ("sand_water_edge", 464, 10, 2, 16, &[0, 1, 2, 3, 4, 5, 6, 0]),
            ("waterfall", 496, 6, 3, 16, &[0, 1, 2, 3]),
            ("land_water_edge", 480, 10, 4, 16, &[0, 1, 2, 3]),
        ];
        let building_expected: [Row; 1] = [("tv_turned_on", 496, 4, 0, 8, &[0, 1])];

        for (spec, row) in GENERAL_REGIONS
            .iter()
            .zip(expected)
            .chain(BUILDING_REGIONS.iter().zip(building_expected))
        {
            let (anim_name, start_tile, tiles, phase, interval, sequence) = row;
            assert_eq!(spec.anim_name, anim_name);
            assert_eq!(spec.start_tile, start_tile, "{anim_name}: start tile");
            assert_eq!(spec.tiles, tiles, "{anim_name}: copy length in tiles");
            assert_eq!(spec.phase, phase, "{anim_name}: phase");
            assert_eq!(spec.interval, interval, "{anim_name}: interval");
            assert_eq!(spec.sequence, sequence, "{anim_name}: frame sequence");
        }
        // `zip` truncates, so pin the table lengths too: a dropped or added
        // region must fail here rather than go unchecked.
        assert_eq!(GENERAL_REGIONS.len(), expected.len());
        assert_eq!(BUILDING_REGIONS.len(), building_expected.len());
    }

    // -- `latched_frame` (module docs' derivation) --------------------------

    /// `(phase, interval, frame_count)` -- [`latched_frame`]'s own cadence
    /// arguments, taken from the shipped spec rather than re-typed, so the
    /// hand-traced expectations below are pinned to the table this module
    /// actually loads.
    fn cadence(spec: &AnimRegionSpec) -> (u64, u64, usize) {
        (
            u64::from(spec.phase),
            u64::from(spec.interval),
            spec.sequence.len(),
        )
    }

    #[test]
    fn latched_frame_is_none_before_the_first_fire() {
        // Flower: phase 0, interval 16 -- the first fire is at tick 16, not
        // tick 0 (module docs: upstream's callback argument starts at 1, so
        // a `phase == 0` region's first `n` with `n % interval == 0` is
        // `interval` itself).
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[0]);
        assert_eq!(latched_frame(0, phase, interval, frames), None);
        assert_eq!(latched_frame(15, phase, interval, frames), None);

        // Water: phase 1, interval 16 -- fires on the very first tick.
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[1]);
        assert_eq!(latched_frame(0, phase, interval, frames), None);
    }

    #[test]
    fn latched_frame_matches_general_flowers_hand_traced_cadence() {
        // `gTilesetAnims_General_Flower = {Frame0, Frame1, Frame0, Frame2}`
        // (`tileset_anims.c:82-87`), phase 0, interval 16: fires at tick 16,
        // 32, 48, 64, ... queuing sequence positions 1, 2, 3, 0, ... in turn
        // (`QueueAnimTiles_General_Flower(timer / 16)`, `:652-656`).
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[0]);
        assert_eq!(latched_frame(16, phase, interval, frames), Some(1)); // Frame1
        assert_eq!(latched_frame(31, phase, interval, frames), Some(1)); // holds until the next fire
        assert_eq!(latched_frame(32, phase, interval, frames), Some(2)); // Frame0 (sequence[2])
        assert_eq!(latched_frame(48, phase, interval, frames), Some(3)); // Frame2
        assert_eq!(latched_frame(64, phase, interval, frames), Some(0)); // wraps: Frame0 (sequence[0])
    }

    #[test]
    fn latched_frame_matches_general_waters_hand_traced_cadence() {
        // Phase 1, interval 16, 8 frames (`tileset_anims.c:89-107,658-662`):
        // fires at tick 1, 17, 33, ..., one array position per fire.
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[1]);
        assert_eq!(latched_frame(1, phase, interval, frames), Some(0));
        assert_eq!(latched_frame(16, phase, interval, frames), Some(0)); // still holding frame 0
        assert_eq!(latched_frame(17, phase, interval, frames), Some(1));
        // One full 8-frame cycle later.
        assert_eq!(latched_frame(1 + 16 * 8, phase, interval, frames), Some(0));
    }

    #[test]
    fn latched_frame_matches_building_tv_turned_ons_hand_traced_cadence() {
        // Phase 0, interval 8, 2 frames (`tileset_anims.c:424-428,1113-1117`):
        // fires at tick 8, 16, 24, ..., alternating.
        let (phase, interval, frames) = cadence(&BUILDING_REGIONS[0]);
        assert_eq!(latched_frame(0, phase, interval, frames), None);
        assert_eq!(latched_frame(7, phase, interval, frames), None);
        assert_eq!(latched_frame(8, phase, interval, frames), Some(1));
        assert_eq!(latched_frame(15, phase, interval, frames), Some(1));
        assert_eq!(latched_frame(16, phase, interval, frames), Some(0));
        assert_eq!(latched_frame(24, phase, interval, frames), Some(1));
    }

    #[test]
    fn latched_frame_ignores_the_256_counter_wrap_without_changing_the_sequence() {
        // Every region's own cycle (`interval * frame_count`) divides 256
        // (module docs), so a tick far past 256 still lands on exactly the
        // frame upstream's wrapped counter would show. General flower's
        // cycle is 64: tick 64 + 256 = 320 must match tick 64.
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[0]);
        assert_eq!(
            latched_frame(320, phase, interval, frames),
            latched_frame(64, phase, interval, frames)
        );
        // Building TV's cycle is 16: tick 8 + 5*256 must match tick 8.
        let (phase, interval, frames) = cadence(&BUILDING_REGIONS[0]);
        assert_eq!(
            latched_frame(8 + 5 * 256, phase, interval, frames),
            latched_frame(8, phase, interval, frames)
        );
    }

    // -- `AnimatedTileset::load` -------------------------------------------

    /// A trivial (zero-entry) pack: enough to call [`AnimatedTileset::load`]
    /// against, since a non-`general`/`building` name never looks up any
    /// entry (`specs` is empty).
    fn empty_pack() -> AssetPack {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&assets::pack::MAGIC);
        bytes.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // 0 directory entries.
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-tileset-anims-test-{}.pack",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        let pack = AssetPack::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        pack
    }

    #[test]
    fn load_is_a_no_op_for_every_non_animated_tileset_name() {
        let pack = empty_pack();
        for name in ["petalburg", "brendans_mays_house", "lab", "unknown"] {
            let tiles = AnimatedTileset::load(&pack, name).unwrap();
            assert!(
                tiles.regions.is_empty(),
                "{name} must resolve to no animated regions"
            );
        }
    }

    // -- `AnimatedTileset::patch` --------------------------------------------

    #[test]
    fn patch_overwrites_only_the_latched_regions_own_tile_range() {
        let tiles = AnimatedTileset {
            regions: vec![Region {
                start_tile: 2,
                phase: 0,
                interval: 4,
                frames: vec![vec![0xAA; 32], vec![0xBB; 32]],
            }],
        };
        let mut bytes = vec![0u8; 4 * 32];

        // Before the first fire (tick < interval, phase 0): untouched.
        tiles.patch(&mut bytes, 0);
        assert!(bytes.iter().all(|&b| b == 0));

        // First fire, at tick == interval: `latched_frame(4, 0, 4, 2)` ==
        // `Some(1)` (`(4/4) % 2 == 1`), so tile index 2 (`start_tile`) gets
        // frame 1's bytes -- and nothing else in the buffer changes.
        tiles.patch(&mut bytes, 4);
        assert_eq!(&bytes[0..2 * 32], [0u8; 2 * 32].as_slice());
        assert_eq!(&bytes[2 * 32..3 * 32], [0xBBu8; 32].as_slice());
        assert_eq!(&bytes[3 * 32..], [0u8; 32].as_slice());
    }
}
