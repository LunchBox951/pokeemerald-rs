//! Shared synthetic-scene fixtures for the overworld tests: a throwaway
//! pack writer plus [`super::OverworldScene`] builders over it.

use std::mem::size_of;

use super::OverworldSceneError;
use assets::{AssetPack, LayoutId, MapLayout};

/// One directory entry for [`write_synthetic_pack`]; a private copy of
/// `assets::pack::tests`' own entry shape, which that module does not
/// export.
pub(super) struct Entry {
    pub(super) id: String,
    pub(super) kind_tag: u8,
    pub(super) meta: Vec<u8>,
    pub(super) payload: Vec<u8>,
}

/// The map a synthetic scene is built for until a test names its own
/// ([`super::OverworldScene::in_room_for_test`]): a real map whose events
/// hold no objects, so no sprite sheet is needed to load it.
const SYNTHETIC_ROOM_MAP: assets::MapId = assets::MapId("MAP_ROUTE122");

impl super::OverworldScene {
    /// [`Self::runtime`] answering from `events` in place of the visit's own,
    /// for the one test that injects a trigger stack.
    #[must_use]
    pub(crate) fn runtime_over_events_for_test<'s>(
        &'s self,
        events: &'s assets::MapEvents,
    ) -> engine::overworld::MapRuntime<'s> {
        self.runtime_over(events)
    }

    /// Re-labels a synthetic-grid scene as `map`: the map's real header
    /// (keeping the scene's own synthetic layout id) and its events, or the
    /// scene's own events when they already belong to `map`'s events owner
    /// (an explicit set, such as Oldale Town's repositioned one, is kept).
    ///
    /// The synthetic grid stands in for `map`'s real one, so a phase test
    /// can drive `map`'s real warps, NPCs, and triggers over a grid it
    /// controls. Test-only: a loaded scene gets its identity from
    /// [`super::OverworldScene::from_pack`]'s validated inputs.
    ///
    /// Relabel only to the map the scene's phase names: the overworld phase
    /// constructors refuse a scene whose [`super::OverworldScene::map_id`]
    /// differs from the phase's own.
    ///
    /// # Panics
    ///
    /// Panics if `map` has no header or events.
    #[must_use]
    pub(crate) fn in_room_for_test(mut self, map: assets::MapId) -> Self {
        let header = assets::MapHeaderTable::new()
            .header(map)
            .expect("a test room is a real map");
        let generated = assets::MapEventsTable::new()
            .resolve(map)
            .expect("a test room is a real map");
        self.map_id = map;
        self.header = assets::MapHeader {
            layout: self.layout.id,
            ..*header
        };
        if self.events.id != generated.id {
            self.events = *generated;
        }
        self
    }
}

const IMAGE_KIND_TAG: u8 = 0;
pub(super) const PALETTE_KIND_TAG: u8 = 1;
const RAW_KIND_TAG: u8 = 2;

const METATILE_RAW_ENTRY_COUNT: usize = 8;
const BORDER_CELL_COUNT: usize = 4;
/// `METATILE_LAYER_TYPE_COVERED`'s bit: the bottom/middle layers draw the
/// opaque tile and the top stays transparent, so the player OBJ (drawn
/// between the middle and top layers) is never hidden by this fixture's
/// otherwise-uniform world content.
const COVERED_LAYER_TYPE_BITS: u16 = 1 << 12;
const WALKABLE_ELEVATION: u8 = 3;
const FIRST_SPECIAL_METATILE_ID: u16 = 1;
const GROUND_PALETTE_INDEX: u8 = 5;
const SPRITE_PIXEL_PALETTE_INDEX: u8 = 9;
/// The synthetic running sheet's opaque index, distinct from walking's.
const RUN_SPRITE_PIXEL_PALETTE_INDEX: u8 = 10;
const RED_BGR555: u16 = 0x001F;
const BLUE_BGR555: u16 = 0x7C00;
const TILESET_PALETTE_BANK_COUNT: u8 = 16;
const SPRITE_PALETTE_COLOR_COUNT: u16 = 16;

/// The scratch path [`synthetic_scene_result_with_connections_and_events`]
/// writes its pack to.
pub(super) fn scene_scratch_pack_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pokeemerald-rs-overworld-test-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ))
}

pub(crate) use crate::pack_test_support::write_scratch_exclusive;

/// Plants a symlink at `path` pointing at a fresh victim file holding
/// `sentinel`, runs `write`, and asserts the victim is untouched. Returns
/// after removing the planted entry and the victim.
#[cfg(unix)]
pub(crate) fn assert_planted_symlink_untouched(path: &std::path::Path, write: impl FnOnce()) {
    let victim = std::env::temp_dir().join(format!(
        "pokeemerald-rs-symlink-victim-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        path.file_name().unwrap().to_string_lossy()
    ));
    let sentinel = b"victim must survive";
    // The victim's path is as predictable as `path`, so it is claimed the
    // same way: a link planted there cannot turn the setup itself into the
    // overwrite these regressions guard against.
    write_scratch_exclusive(&victim, sentinel);
    let _ = std::fs::remove_file(path);
    std::os::unix::fs::symlink(&victim, path).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(write));
    let after = std::fs::read(&victim).unwrap();
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(&victim);
    assert_eq!(
        after, sentinel,
        "a planted symlink's target was overwritten"
    );
    assert!(
        outcome.is_ok(),
        "the writer must replace a planted symlink, not panic on it"
    );
}

/// Serializes `entries`, sorted by id as the pack format requires, into a
/// pack file's bytes.
pub(super) fn write_synthetic_pack(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.id.cmp(&b.id));

    let header_size = assets::pack::MAGIC.len() + size_of::<u32>() + size_of::<u32>();
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += size_of::<u16>()
            + e.id.len()
            + size_of::<u8>()
            + size_of::<u64>()
            + size_of::<u64>()
            + e.meta.len();
    }
    let mut offset = header_size + directory_size;
    let mut offsets = Vec::new();
    for e in &entries {
        offsets.push(offset);
        offset += e.payload.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&assets::pack::MAGIC);
    out.extend_from_slice(&assets::pack::FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
    for (e, &off) in entries.iter().zip(&offsets) {
        out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
        out.extend_from_slice(e.id.as_bytes());
        out.push(e.kind_tag);
        out.extend_from_slice(&u64::try_from(off).unwrap().to_le_bytes());
        out.extend_from_slice(&u64::try_from(e.payload.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&e.meta);
    }
    for e in &entries {
        out.extend_from_slice(&e.payload);
    }
    out
}

pub(super) fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

fn synthetic_overworld_pack_bytes(width: u16, height: u16) -> Vec<u8> {
    synthetic_overworld_pack_bytes_for("general", width, height)
}

/// [`synthetic_overworld_pack_bytes`], parameterized on the tileset pack
/// name so a test can also build the *unanimated*-primary shape, e.g.
/// `petalburg` -- a real bundled secondary tileset [`super::tileset_anims`]
/// declares no regions for.
pub(super) fn synthetic_overworld_pack_bytes_for(
    tileset: &str,
    width: u16,
    height: u16,
) -> Vec<u8> {
    write_synthetic_pack(synthetic_overworld_pack_entries_for(tileset, width, height))
}

/// The entries behind [`synthetic_overworld_pack_bytes_for`]: one opaque
/// metatile covering a `width` x `height` layout for `tileset`, plus
/// Brendan's walking sheet/palette. Exposed directly so a test can corrupt
/// one entry deliberately before writing the pack.
///
/// [`push_general_anim_frames`]'s frames (fabricated only for `"general"`)
/// land in tile padding this default fixture's one metatile never references,
/// so `AnimatedTileset` patching stays exercised without changing a composed
/// pixel; [`super::tests::synthetic_animated_flower_pack_bytes`] overrides that to observe
/// animated output.
pub(super) fn synthetic_overworld_pack_entries_for(
    tileset: &str,
    width: u16,
    height: u16,
) -> Vec<Entry> {
    let tile_pixels = vec![GROUND_PALETTE_INDEX; 8 * 8];

    let metatiles: Vec<u8> = std::iter::repeat_n(0u16.to_le_bytes(), METATILE_RAW_ENTRY_COUNT)
        .flatten()
        .collect();
    let metatile_attrs = COVERED_LAYER_TYPE_BITS.to_le_bytes().to_vec();

    let grid_cell = assets::MetatileCell {
        metatile_id: 0,
        collision: 0,
        elevation: WALKABLE_ELEVATION,
    }
    .pack();
    let cells = usize::from(width) * usize::from(height);
    let grid: Vec<u8> = std::iter::repeat_n(grid_cell.to_le_bytes(), cells)
        .flatten()
        .collect();
    let border: Vec<u8> = std::iter::repeat_n(grid_cell.to_le_bytes(), BORDER_CELL_COUNT)
        .flatten()
        .collect();

    let bank0_payload = {
        let mut p = vec![0u8; usize::from(GROUND_PALETTE_INDEX) * 2];
        p.extend_from_slice(&RED_BGR555.to_le_bytes());
        p
    };

    let mut entries = vec![
        Entry {
            id: format!("tileset/{tileset}/tiles"),
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(8, 8, 4),
            payload: tile_pixels,
        },
        Entry {
            id: format!("tileset/{tileset}/metatiles"),
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: metatiles,
        },
        Entry {
            id: format!("tileset/{tileset}/metatile-attributes"),
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: metatile_attrs,
        },
        Entry {
            id: "layout/map_test/map".into(),
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: grid,
        },
        Entry {
            id: "layout/map_test/border".into(),
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: border,
        },
        Entry {
            id: "sprite/brendan/walking".into(),
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(144, 32, 8),
            // Opaque and distinct from GROUND_PALETTE_INDEX, so a composed
            // frame can tell the sprite layer from the world layer.
            payload: vec![SPRITE_PIXEL_PALETTE_INDEX; 144 * 32],
        },
        Entry {
            id: "sprite/brendan/running".into(),
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(144, 32, 8),
            // A different opaque index from walking, so a composed frame can
            // tell which sheet the avatar drew from.
            payload: vec![RUN_SPRITE_PIXEL_PALETTE_INDEX; 144 * 32],
        },
        Entry {
            id: "sprite/palette/brendan".into(),
            kind_tag: PALETTE_KIND_TAG,
            meta: SPRITE_PALETTE_COLOR_COUNT.to_le_bytes().to_vec(),
            payload: {
                let mut p = vec![0u8; usize::from(SPRITE_PIXEL_PALETTE_INDEX) * 2];
                p.extend_from_slice(&BLUE_BGR555.to_le_bytes());
                p.resize(usize::from(SPRITE_PALETTE_COLOR_COUNT) * 2, 0);
                p
            },
        },
    ];
    for slot in 0..TILESET_PALETTE_BANK_COUNT {
        let id = format!("tileset/{tileset}/palette/{slot:02}");
        entries.push(Entry {
            id,
            kind_tag: PALETTE_KIND_TAG,
            meta: 0u16.to_le_bytes().to_vec(),
            payload: vec![],
        });
    }
    if let Some(bank0) = entries
        .iter_mut()
        .find(|e| e.id == format!("tileset/{tileset}/palette/00"))
    {
        bank0.meta = (u16::from(GROUND_PALETTE_INDEX) + 1).to_le_bytes().to_vec();
        bank0.payload = bank0_payload;
    }

    if tileset == "general" {
        push_general_anim_frames(&mut entries);
    }
    push_unconditional_sprite_palettes(&mut entries);

    entries
}

/// [`synthetic_overworld_pack_entries_for`]'s `"general"` entries, with an
/// extra metatile per entry of `specials` appended: same collision,
/// elevation, and tile/palette bytes as the uniform fixture, differing only
/// in the one byte `MapRuntime::metatile_behavior` reads, so every special
/// cell renders identically to an ordinary one.
fn synthetic_overworld_pack_entries_with_special_tiles(
    width: u16,
    height: u16,
    specials: &[((u16, u16), u8)],
) -> Vec<Entry> {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);

    for (index, ((sx, sy), special_behavior)) in specials.iter().copied().enumerate() {
        let metatile: Vec<u8> = std::iter::repeat_n(0u16.to_le_bytes(), METATILE_RAW_ENTRY_COUNT)
            .flatten()
            .collect();
        let metatiles = entries
            .iter_mut()
            .find(|e| e.id == "tileset/general/metatiles")
            .expect("the general fixture always fabricates its own metatiles entry");
        metatiles.payload.extend_from_slice(&metatile);

        let attr = (COVERED_LAYER_TYPE_BITS | u16::from(special_behavior)).to_le_bytes();
        let attrs = entries
            .iter_mut()
            .find(|e| e.id == "tileset/general/metatile-attributes")
            .expect("the general fixture always fabricates its own metatile-attributes entry");
        attrs.payload.extend_from_slice(&attr);

        assert!(
            sx < width && sy < height,
            "special tile ({sx}, {sy}) must lie inside the {width}x{height} fixture"
        );
        let metatile_id = u16::try_from(index + usize::from(FIRST_SPECIAL_METATILE_ID))
            .expect("a fixture never needs 65k special tiles");
        let cell = assets::MetatileCell {
            metatile_id,
            collision: 0,
            elevation: WALKABLE_ELEVATION,
        }
        .pack();
        let grid = entries
            .iter_mut()
            .find(|e| e.id == "layout/map_test/map")
            .expect("the general fixture always fabricates its own layout entry");
        let idx = (usize::from(sy) * usize::from(width) + usize::from(sx)) * 2;
        grid.payload[idx..idx + 2].copy_from_slice(&cell.to_le_bytes());
    }

    entries
}

/// One frame per entry of every animated region [`super::tileset_anims`]
/// declares for `general`: `AnimatedTileset::load` rejects a frame whose
/// packed length disagrees with its region's declared tile count.
fn push_general_anim_frames(entries: &mut Vec<Entry>) {
    for (anim, frame_count, tiles) in [
        ("flower", 3u8, 4u32),
        ("water", 8, 30),
        ("sand_water_edge", 7, 10),
        ("waterfall", 4, 6),
        ("land_water_edge", 4, 10),
    ] {
        for n in 0..frame_count {
            entries.push(Entry {
                id: format!("tileset/general/anim/{anim}/{n}"),
                kind_tag: IMAGE_KIND_TAG,
                meta: image_meta(8, 8 * tiles, 4),
                payload: vec![GROUND_PALETTE_INDEX; 8 * 8 * tiles as usize],
            });
        }
    }
}

/// The sprite palettes `OverworldScene::from_pack` loads unconditionally --
/// the four generic `npc_1..4` banks and the other playable protagonist's
/// own (see `npc::build_combined_palette`) -- as empty placeholders, so
/// these lookups succeed regardless of whether a fixture's object events
/// reference them. This fixture's player is Brendan, so the "other
/// protagonist" bank is May's.
fn push_unconditional_sprite_palettes(entries: &mut Vec<Entry>) {
    let mut placeholder = |id: String| {
        entries.push(Entry {
            id,
            kind_tag: PALETTE_KIND_TAG,
            meta: 0u16.to_le_bytes().to_vec(),
            payload: vec![],
        });
    };
    for n in 1..=4u8 {
        placeholder(format!("sprite/palette/npc_{n}"));
    }
    placeholder("sprite/palette/may".into());
}

static NO_OBJECT_EVENTS: assets::MapEvents = assets::MapEvents {
    id: SYNTHETIC_ROOM_MAP,
    shared_events_map: None,
    object_events: &[],
    warp_events: &[],
    coord_events: &[],
    bg_events: &[],
};

/// An [`super::OverworldScene`] over a freshly written, then removed,
/// synthetic pack: a `width` x `height` room of one uniform opaque
/// metatile, Brendan's sprite, and no object events.
///
/// `pub(crate)` so `crate::flow::overworld_phase`'s headless tests can build
/// an `OverworldPhase` around one without a real pack.
pub(crate) fn synthetic_scene(width: u16, height: u16) -> super::OverworldScene {
    synthetic_scene_result(
        synthetic_overworld_pack_bytes(width, height),
        "gTileset_General",
        width,
        height,
    )
    .expect("synthetic pack should decode cleanly")
}

/// [`synthetic_scene`] with its one tile painted as a non-repeating run of
/// seven distinct opaque colours instead of one, so a BG mosaic block above one
/// pixel visibly flattens the composed frame.
pub(crate) fn synthetic_patterned_scene(width: u16, height: u16) -> super::OverworldScene {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);
    let tiles = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/tiles")
        .expect("the general fixture always fabricates its own tiles entry");
    tiles.payload = (0..8u8)
        .flat_map(|y| (0..8u8).map(move |x| 1 + (x ^ (y * 3)) % 7))
        .collect();
    let bank0 = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/00")
        .expect("the general fixture always fabricates its own bank-0 palette");
    bank0.meta = 8u16.to_le_bytes().to_vec();
    bank0.payload = (0..8u16)
        .flat_map(|index| (index * 0x0841).to_le_bytes())
        .collect();
    synthetic_scene_result(
        write_synthetic_pack(entries),
        "gTileset_General",
        width,
        height,
    )
    .expect("the patterned synthetic pack should decode cleanly")
}

/// [`synthetic_scene`], loaded with `events` instead of no object events,
/// plus an opaque 16x32 people sheet for each of `sprite_paths` so those
/// events' sprites bind and draw.
pub(crate) fn synthetic_scene_with_events(
    width: u16,
    height: u16,
    events: &'static assets::MapEvents,
    sprite_paths: &[&str],
) -> super::OverworldScene {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);
    for path in sprite_paths {
        entries.push(Entry {
            id: format!("sprite/{path}"),
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(144, 32, 8),
            payload: vec![3u8; 144 * 32],
        });
    }
    synthetic_scene_result_with_connections_and_events(
        write_synthetic_pack(entries),
        "gTileset_General",
        width,
        height,
        &[],
        events,
    )
    .expect("synthetic pack with object-event sprites should decode cleanly")
}

/// [`synthetic_scene`], but the cell at `special_pos` is a second, distinct
/// metatile with `special_behavior` instead of ordinary ground
/// (`MB_NORMAL`); otherwise identical: fully walkable, uniform elevation
/// throughout.
///
/// No bundled real map can exercise a legal step into an arrow-warp tile's
/// own direction: every arrow tile this port's data reaches has that
/// direction's neighboring cell impassable.
pub(crate) fn synthetic_scene_with_special_tile(
    width: u16,
    height: u16,
    special_pos: (u16, u16),
    special_behavior: u8,
) -> super::OverworldScene {
    synthetic_scene_with_special_tiles(width, height, &[(special_pos, special_behavior)])
}

/// [`synthetic_scene_with_special_tile`] for more than one tile: each entry
/// gets its own metatile id and behavior, everything else stays the uniform
/// walkable fixture.
///
/// A land-encounter tile and a door-shaped warp tile coexisting on one map
/// drives upstream's `ProcessPlayerFieldInput` precedence
/// (`field_control_avatar.c:155-172`) through a real `OverworldPhase::step`,
/// which no single-behavior fixture can express.
pub(crate) fn synthetic_scene_with_special_tiles(
    width: u16,
    height: u16,
    specials: &[((u16, u16), u8)],
) -> super::OverworldScene {
    synthetic_scene_result(
        write_synthetic_pack(synthetic_overworld_pack_entries_with_special_tiles(
            width, height, specials,
        )),
        "gTileset_General",
        width,
        height,
    )
    .expect("synthetic pack with special tiles should decode cleanly")
}

/// [`synthetic_scene`], but the cell at `pos` carries `elevation` instead of
/// the fixture's uniform ground elevation.
pub(crate) fn synthetic_scene_with_cell_elevation(
    width: u16,
    height: u16,
    pos: (u16, u16),
    elevation: u8,
) -> super::OverworldScene {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);
    assert!(
        pos.0 < width && pos.1 < height,
        "elevated tile {pos:?} must lie inside the {width}x{height} fixture"
    );
    let cell = assets::MetatileCell {
        metatile_id: 0,
        collision: 0,
        elevation,
    }
    .pack();
    let grid = entries
        .iter_mut()
        .find(|e| e.id == "layout/map_test/map")
        .expect("the general fixture always fabricates its own layout entry");
    let idx = (usize::from(pos.1) * usize::from(width) + usize::from(pos.0)) * 2;
    grid.payload[idx..idx + 2].copy_from_slice(&cell.to_le_bytes());
    synthetic_scene_result(
        write_synthetic_pack(entries),
        "gTileset_General",
        width,
        height,
    )
    .expect("synthetic pack with an elevated cell should decode cleanly")
}

pub(crate) fn synthetic_scene_with_special_tiles_at_elevations(
    width: u16,
    height: u16,
    specials: &[((u16, u16), u8, u8)],
) -> super::OverworldScene {
    let behaviors: Vec<((u16, u16), u8)> = specials
        .iter()
        .map(|&(pos, behavior, _)| (pos, behavior))
        .collect();
    let mut entries =
        synthetic_overworld_pack_entries_with_special_tiles(width, height, &behaviors);
    for (index, &((sx, sy), _, elevation)) in specials.iter().enumerate() {
        let metatile_id = u16::try_from(index + usize::from(FIRST_SPECIAL_METATILE_ID))
            .expect("a fixture never needs 65k special tiles");
        let cell = assets::MetatileCell {
            metatile_id,
            collision: 0,
            elevation,
        }
        .pack();
        let grid = entries
            .iter_mut()
            .find(|e| e.id == "layout/map_test/map")
            .expect("the general fixture always fabricates its own layout entry");
        let idx = (usize::from(sy) * usize::from(width) + usize::from(sx)) * 2;
        grid.payload[idx..idx + 2].copy_from_slice(&cell.to_le_bytes());
    }
    synthetic_scene_result(
        write_synthetic_pack(entries),
        "gTileset_General",
        width,
        height,
    )
    .expect("synthetic pack with elevated special tiles should decode cleanly")
}

/// [`synthetic_scene`]'s fallible core, parameterized on the pack bytes and
/// the layout's tileset symbol, so a test can assert on rejection errors and
/// on non-`general` fixtures too.
pub(super) fn synthetic_scene_result(
    pack_bytes: Vec<u8>,
    tileset_symbol: &'static str,
    width: u16,
    height: u16,
) -> Result<super::OverworldScene, OverworldSceneError> {
    synthetic_scene_result_with_connections(pack_bytes, tileset_symbol, width, height, &[])
}

/// [`synthetic_scene_result`], with `connections` threaded onto the
/// synthetic room's [`assets::MapHeader`]. A connection's `target` still
/// resolves against the real generated `assets::MapHeaderTable`/
/// `assets::LayoutTable` -- there is no synthetic map-table stand-in -- so a
/// target must be a real bundled `MapId` whose pack entry `pack_bytes` also
/// supplies.
pub(super) fn synthetic_scene_result_with_connections(
    pack_bytes: Vec<u8>,
    tileset_symbol: &'static str,
    width: u16,
    height: u16,
    connections: &'static [assets::MapConnection],
) -> Result<super::OverworldScene, OverworldSceneError> {
    synthetic_scene_result_with_connections_and_events(
        pack_bytes,
        tileset_symbol,
        width,
        height,
        connections,
        &NO_OBJECT_EVENTS,
    )
}

/// [`synthetic_scene_result_with_connections`], with `events` -- rather than
/// the always-empty [`NO_OBJECT_EVENTS`] -- threaded into
/// `super::OverworldScene::from_pack`, so a test can exercise NPC rendering
/// over a synthetic pack instead of needing a real one.
pub(super) fn synthetic_scene_result_with_connections_and_events(
    pack_bytes: Vec<u8>,
    tileset_symbol: &'static str,
    width: u16,
    height: u16,
    connections: &'static [assets::MapConnection],
    events: &'static assets::MapEvents,
) -> Result<super::OverworldScene, OverworldSceneError> {
    // pid + thread id keep concurrently-running tests' scratch packs from
    // colliding on the same path.
    let path = scene_scratch_pack_path();
    write_scratch_exclusive(&path, pack_bytes);
    let pack = AssetPack::load(&path).unwrap();
    let layout = MapLayout {
        id: LayoutId("LAYOUT_MAP_TEST"),
        name: "MapTest",
        width,
        height,
        primary_tileset: tileset_symbol,
        secondary_tileset: tileset_symbol,
    };
    let header = assets::MapHeader {
        id: events.id,
        group: 0,
        num: 0,
        name: "MapTest",
        layout: LayoutId("LAYOUT_MAP_TEST"),
        music: assets::MusicId(0),
        region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
        requires_flash: false,
        weather: assets::Weather::None,
        map_type: assets::MapType::Route,
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: false,
        battle_scene: assets::BattleScene::Normal,
        connections,
    };
    let scene = super::OverworldScene::from_pack(
        &pack,
        &header,
        &layout,
        super::PlayerCharacter::Brendan,
        events,
        &engine::event_data::EventData::new(),
    );
    let _ = std::fs::remove_file(path);
    scene
}

/// Writes a synthetic pack at `path` that loads the *real* bundled
/// `MAP_OLDALE_TOWN` header and layout (`general`/`petalburg`, 20x20) through
/// [`super::load_room_from_source`], with an opaque people sheet for each of
/// `sprite_paths`, so a phase test can complete a real map transition.
pub(crate) fn write_oldale_layout_pack(path: &std::path::Path, sprite_paths: &[&str]) {
    let mut entries: Vec<Entry> = synthetic_overworld_pack_entries_for("general", 20, 20)
        .into_iter()
        .map(|mut e| {
            if e.id == "layout/map_test/map" {
                e.id = "layout/oldale_town/map".into();
            } else if e.id == "layout/map_test/border" {
                e.id = "layout/oldale_town/border".into();
            }
            e
        })
        .collect();
    entries.extend(
        synthetic_overworld_pack_entries_for("petalburg", 20, 20)
            .into_iter()
            .filter(|e| e.id.starts_with("tileset/petalburg/")),
    );
    for sprite in sprite_paths {
        entries.push(Entry {
            id: format!("sprite/{sprite}"),
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(144, 32, 8),
            payload: vec![3u8; 144 * 32],
        });
    }
    write_scratch_exclusive(path, write_synthetic_pack(entries));
}

/// Writes a synthetic pack at `path` that loads the *real* bundled
/// `MAP_ROUTE101` and `MAP_OLDALE_TOWN` headers and layouts (both
/// `general`/`petalburg`, 20x20, joined by an offset-0 north/south
/// connection) through [`super::load_room_from_source`], with each of
/// `oldale_specials` a distinct metatile carrying its own behavior on
/// Oldale's grid, and an opaque people sheet for each of `sprite_paths`.
///
/// Both layouts share the one `general` attribute table, so a phase test can
/// cross the real connection, through `OverworldPhase::cross_connection`,
/// onto a behaviour-bearing landing tile.
pub(crate) fn write_route101_oldale_layout_pack(
    path: &std::path::Path,
    oldale_specials: &[((u16, u16), u8)],
    sprite_paths: &[&str],
) {
    let rename = |mut e: Entry, layout: &str| {
        if e.id == "layout/map_test/map" {
            e.id = format!("layout/{layout}/map");
        } else if e.id == "layout/map_test/border" {
            e.id = format!("layout/{layout}/border");
        }
        e
    };
    let mut entries: Vec<Entry> =
        synthetic_overworld_pack_entries_with_special_tiles(20, 20, oldale_specials)
            .into_iter()
            .map(|e| rename(e, "oldale_town"))
            .collect();
    entries.extend(
        synthetic_overworld_pack_entries_for("general", 20, 20)
            .into_iter()
            .filter(|e| e.id.starts_with("layout/map_test/"))
            .map(|e| rename(e, "route101")),
    );
    entries.extend(
        synthetic_overworld_pack_entries_for("petalburg", 20, 20)
            .into_iter()
            .filter(|e| e.id.starts_with("tileset/petalburg/")),
    );
    for sprite in sprite_paths {
        entries.push(Entry {
            id: format!("sprite/{sprite}"),
            kind_tag: IMAGE_KIND_TAG,
            meta: image_meta(144, 32, 8),
            payload: vec![3u8; 144 * 32],
        });
    }
    write_scratch_exclusive(path, write_synthetic_pack(entries));
}
