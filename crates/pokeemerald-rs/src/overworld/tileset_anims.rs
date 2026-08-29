//! Loads and applies the animations for bundled primary tilesets.
//!
//! Each region selects numbered animation assets on its own cadence and patches
//! a fixed range in the combined tileset's primary tile block. Bundled secondary
//! tilesets have no active animation callback.
//!
//! `tick` is the number of rendered frames since the current room loaded. It
//! resets to zero on fresh loads and warps, matching the upstream
//! `InitTilesetAnimations` call sites (`pokeemerald/src/overworld.c:529,1867,
//! 1942,2039`). No region fires at tick zero, so the base tiles remain unchanged.
//! After the first dispatch, each frame stays latched until that region fires
//! again. General's complete cadence repeats every 128 ticks once every region
//! has fired; the last first dispatch occurs at tick 16.
//!
//! ## Configured regions
//!
//! | primary tileset | asset | start tile | tiles | phase | interval | sequence |
//! |---|---|---:|---:|---:|---:|---|
//! | General | `flower` | 508 | 4 | 0 | 16 | `[0, 1, 0, 2]` |
//! | General | `water` | 432 | 30 | 1 | 16 | `[0, 1, 2, 3, 4, 5, 6, 7]` |
//! | General | `sand_water_edge` | 464 | 10 | 2 | 16 | `[0, 1, 2, 3, 4, 5, 6, 0]` |
//! | General | `waterfall` | 496 | 6 | 3 | 16 | `[0, 1, 2, 3]` |
//! | General | `land_water_edge` | 480 | 10 | 4 | 16 | `[0, 1, 2, 3]` |
//! | Building | `tv_turned_on` | 496 | 4 | 0 | 8 | `[0, 1]` |
//!
//! `start tile` is a combined-tileset tile index and `tiles` is the exact
//! packed frame length that [`AnimatedTileset::load`] validates. A region fires
//! when the counter modulo `interval` equals `phase`; `sequence` selects the
//! numbered assets in dispatch order.

use assets::AssetPack;
use rendering::BitDepth;

use super::{pack_4bpp_region, OverworldSceneError};

const UPSTREAM_COUNTER_PERIOD: u16 = 256;

#[derive(Debug, PartialEq, Eq)]
struct AnimationRegionSpec {
    asset_name: &'static str,
    start_tile: u16,
    tile_count: u16,
    fire_phase: u16,
    fire_interval: u16,
    frame_sequence: &'static [u8],
}

const GENERAL_REGIONS: [AnimationRegionSpec; 5] = [
    AnimationRegionSpec {
        asset_name: "flower",
        start_tile: 508,
        tile_count: 4,
        fire_phase: 0,
        fire_interval: 16,
        frame_sequence: &[0, 1, 0, 2],
    },
    AnimationRegionSpec {
        asset_name: "water",
        start_tile: 432,
        tile_count: 30,
        fire_phase: 1,
        fire_interval: 16,
        frame_sequence: &[0, 1, 2, 3, 4, 5, 6, 7],
    },
    AnimationRegionSpec {
        asset_name: "sand_water_edge",
        start_tile: 464,
        tile_count: 10,
        fire_phase: 2,
        fire_interval: 16,
        frame_sequence: &[0, 1, 2, 3, 4, 5, 6, 0],
    },
    AnimationRegionSpec {
        asset_name: "waterfall",
        start_tile: 496,
        tile_count: 6,
        fire_phase: 3,
        fire_interval: 16,
        frame_sequence: &[0, 1, 2, 3],
    },
    AnimationRegionSpec {
        asset_name: "land_water_edge",
        start_tile: 480,
        tile_count: 10,
        fire_phase: 4,
        fire_interval: 16,
        frame_sequence: &[0, 1, 2, 3],
    },
];

const BUILDING_REGIONS: [AnimationRegionSpec; 1] = [AnimationRegionSpec {
    asset_name: "tv_turned_on",
    start_tile: 496,
    tile_count: 4,
    fire_phase: 0,
    fire_interval: 8,
    frame_sequence: &[0, 1],
}];

#[derive(Debug)]
struct LoadedAnimationRegion {
    start_tile: u16,
    fire_phase: u32,
    fire_interval: u32,
    frames_in_sequence: Vec<Vec<u8>>,
}

/// Animation frames for a room's primary tileset.
#[derive(Debug)]
pub(super) struct AnimatedTileset {
    regions: Vec<LoadedAnimationRegion>,
}

impl AnimatedTileset {
    /// Loads animation regions for `primary_tileset_name`.
    ///
    /// Unrecognized names produce an empty animation set.
    ///
    /// # Errors
    ///
    /// Returns an error when an animation asset is missing or malformed, or
    /// when its packed size does not match the configured tile count.
    pub(super) fn load(
        pack: &AssetPack,
        primary_tileset_name: &str,
    ) -> Result<Self, OverworldSceneError> {
        let region_specs: &[AnimationRegionSpec] = match primary_tileset_name {
            "general" => &GENERAL_REGIONS,
            "building" => &BUILDING_REGIONS,
            _ => &[],
        };
        let mut regions = Vec::with_capacity(region_specs.len());
        for spec in region_specs {
            debug_assert!(
                spec.fire_phase < spec.fire_interval,
                "{}: fire phase {} must be less than interval {}",
                spec.asset_name,
                spec.fire_phase,
                spec.fire_interval
            );
            debug_assert!(!spec.frame_sequence.is_empty());
            debug_assert_eq!(
                UPSTREAM_COUNTER_PERIOD
                    % (spec.fire_interval
                        * u16::try_from(spec.frame_sequence.len())
                            .expect("animation frame count must fit in u16")),
                0,
                "{}: animation cycle must divide the upstream counter period",
                spec.asset_name
            );
            debug_assert!(
                usize::from(spec.start_tile) + usize::from(spec.tile_count)
                    <= super::viewport::NUM_TILES_IN_PRIMARY,
                "{}: tiles {}..{} must stay inside the primary block",
                spec.asset_name,
                spec.start_tile,
                spec.start_tile + spec.tile_count
            );

            let mut frames_in_sequence = Vec::with_capacity(spec.frame_sequence.len());
            for &frame_number in spec.frame_sequence {
                let asset_id = format!(
                    "tileset/{primary_tileset_name}/anim/{}/{frame_number}",
                    spec.asset_name
                );
                let image = pack.image(&asset_id)?;
                let packed_frame = pack_4bpp_region(
                    "tileset/anim",
                    image,
                    0,
                    0,
                    image.width as usize,
                    image.height as usize,
                )?;
                let expected_frame_bytes =
                    usize::from(spec.tile_count) * BitDepth::Bpp4.tile_byte_len();
                if packed_frame.len() != expected_frame_bytes {
                    return Err(OverworldSceneError::AnimFrameSizeMismatch {
                        anim_name: spec.asset_name,
                        expected_tiles: spec.tile_count,
                        frame_bytes: packed_frame.len(),
                    });
                }
                frames_in_sequence.push(packed_frame);
            }
            regions.push(LoadedAnimationRegion {
                start_tile: spec.start_tile,
                fire_phase: u32::from(spec.fire_phase),
                fire_interval: u32::from(spec.fire_interval),
                frames_in_sequence,
            });
        }
        Ok(Self { regions })
    }

    /// Returns whether this tileset has no animated regions.
    pub(super) fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Applies every frame selected at `tick` to its primary-tile range.
    /// Regions remain unchanged until their first dispatch.
    pub(super) fn patch(&self, bytes: &mut [u8], tick: u32) {
        let tile_len = BitDepth::Bpp4.tile_byte_len();
        for region in &self.regions {
            let Some(index) = latched_frame(
                u64::from(tick),
                u64::from(region.fire_phase),
                u64::from(region.fire_interval),
                region.frames_in_sequence.len(),
            ) else {
                continue;
            };
            let frame = &region.frames_in_sequence[index];
            let start_byte = usize::from(region.start_tile) * tile_len;
            let end_byte = start_byte + frame.len();
            bytes[start_byte..end_byte].copy_from_slice(frame);
        }
    }
}

/// Returns the frame most recently dispatched by `tick`, or `None` before the
/// region's first dispatch.
///
/// Upstream increments its counter before dispatch and transfers only queued
/// regions (`pokeemerald/src/tileset_anims.c:564-569,586-598`), leaving the
/// other tile bytes latched. Every configured cycle divides its 256-tick counter
/// period, so unwrapped elapsed ticks select the same frame after each wrap.
fn latched_frame(
    tick: u64,
    fire_phase: u64,
    fire_interval: u64,
    frame_count: usize,
) -> Option<usize> {
    let first_fire = if fire_phase == 0 {
        fire_interval
    } else {
        fire_phase
    };
    if tick < first_fire {
        return None;
    }
    let last_fire = fire_phase + fire_interval * ((tick - fire_phase) / fire_interval);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "frame_count is a small static table length"
    )]
    Some(((last_fire / fire_interval) % frame_count as u64) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_specs_match_the_upstream_ranges_and_cadence() {
        let expected_general = [
            AnimationRegionSpec {
                asset_name: "flower",
                start_tile: 508,
                tile_count: 4,
                fire_phase: 0,
                fire_interval: 16,
                frame_sequence: &[0, 1, 0, 2],
            },
            AnimationRegionSpec {
                asset_name: "water",
                start_tile: 432,
                tile_count: 30,
                fire_phase: 1,
                fire_interval: 16,
                frame_sequence: &[0, 1, 2, 3, 4, 5, 6, 7],
            },
            AnimationRegionSpec {
                asset_name: "sand_water_edge",
                start_tile: 464,
                tile_count: 10,
                fire_phase: 2,
                fire_interval: 16,
                frame_sequence: &[0, 1, 2, 3, 4, 5, 6, 0],
            },
            AnimationRegionSpec {
                asset_name: "waterfall",
                start_tile: 496,
                tile_count: 6,
                fire_phase: 3,
                fire_interval: 16,
                frame_sequence: &[0, 1, 2, 3],
            },
            AnimationRegionSpec {
                asset_name: "land_water_edge",
                start_tile: 480,
                tile_count: 10,
                fire_phase: 4,
                fire_interval: 16,
                frame_sequence: &[0, 1, 2, 3],
            },
        ];
        let expected_building = [AnimationRegionSpec {
            asset_name: "tv_turned_on",
            start_tile: 496,
            tile_count: 4,
            fire_phase: 0,
            fire_interval: 8,
            frame_sequence: &[0, 1],
        }];

        assert_eq!(GENERAL_REGIONS, expected_general);
        assert_eq!(BUILDING_REGIONS, expected_building);
    }

    fn cadence(spec: &AnimationRegionSpec) -> (u64, u64, usize) {
        (
            u64::from(spec.fire_phase),
            u64::from(spec.fire_interval),
            spec.frame_sequence.len(),
        )
    }

    #[test]
    fn latched_frame_is_none_before_the_first_fire() {
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[0]);
        assert_eq!(latched_frame(0, phase, interval, frames), None);
        assert_eq!(latched_frame(15, phase, interval, frames), None);

        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[1]);
        assert_eq!(latched_frame(0, phase, interval, frames), None);
    }

    #[test]
    fn latched_frame_matches_general_flower_cadence() {
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[0]);
        assert_eq!(latched_frame(16, phase, interval, frames), Some(1));
        assert_eq!(latched_frame(31, phase, interval, frames), Some(1));
        assert_eq!(latched_frame(32, phase, interval, frames), Some(2));
        assert_eq!(latched_frame(48, phase, interval, frames), Some(3));
        assert_eq!(latched_frame(64, phase, interval, frames), Some(0));
    }

    #[test]
    fn latched_frame_matches_general_water_cadence() {
        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[1]);
        assert_eq!(latched_frame(1, phase, interval, frames), Some(0));
        assert_eq!(latched_frame(16, phase, interval, frames), Some(0));
        assert_eq!(latched_frame(17, phase, interval, frames), Some(1));
        assert_eq!(
            latched_frame(1 + interval * frames as u64, phase, interval, frames),
            Some(0)
        );
    }

    #[test]
    fn latched_frame_matches_building_tv_cadence() {
        let (phase, interval, frames) = cadence(&BUILDING_REGIONS[0]);
        assert_eq!(latched_frame(0, phase, interval, frames), None);
        assert_eq!(latched_frame(7, phase, interval, frames), None);
        assert_eq!(latched_frame(8, phase, interval, frames), Some(1));
        assert_eq!(latched_frame(15, phase, interval, frames), Some(1));
        assert_eq!(latched_frame(16, phase, interval, frames), Some(0));
        assert_eq!(latched_frame(24, phase, interval, frames), Some(1));
    }

    #[test]
    fn latched_frame_matches_the_wrapped_upstream_counter() {
        let counter_period = u64::from(UPSTREAM_COUNTER_PERIOD);

        let (phase, interval, frames) = cadence(&GENERAL_REGIONS[0]);
        assert_eq!(
            latched_frame(64 + counter_period, phase, interval, frames),
            latched_frame(64, phase, interval, frames)
        );

        let (phase, interval, frames) = cadence(&BUILDING_REGIONS[0]);
        assert_eq!(
            latched_frame(8 + 5 * counter_period, phase, interval, frames),
            latched_frame(8, phase, interval, frames)
        );
    }

    fn empty_pack() -> AssetPack {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&assets::pack::MAGIC);
        bytes.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
        let directory_entry_count = 0u32;
        bytes.extend_from_slice(&directory_entry_count.to_le_bytes());
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

    #[test]
    fn patch_overwrites_only_the_latched_regions_own_tile_range() {
        let tile_len = BitDepth::Bpp4.tile_byte_len();
        let tiles = AnimatedTileset {
            regions: vec![LoadedAnimationRegion {
                start_tile: 2,
                fire_phase: 0,
                fire_interval: 4,
                frames_in_sequence: vec![vec![0xAA; tile_len], vec![0xBB; tile_len]],
            }],
        };
        let mut bytes = vec![0u8; 4 * tile_len];

        tiles.patch(&mut bytes, 0);
        assert!(bytes.iter().all(|&byte| byte == 0));

        tiles.patch(&mut bytes, 4);
        assert!(bytes[..2 * tile_len].iter().all(|&byte| byte == 0));
        assert!(bytes[2 * tile_len..3 * tile_len]
            .iter()
            .all(|&byte| byte == 0xBB));
        assert!(bytes[3 * tile_len..].iter().all(|&byte| byte == 0));
    }
}
