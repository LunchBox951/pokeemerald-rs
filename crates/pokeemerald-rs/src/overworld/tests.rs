//! Unit tests for [`super::OverworldScene`] and its private helpers.
//!
//! [`overworld_scene_from_pack_composes_a_non_blank_deterministic_frame`]
//! builds a small **synthetic** pack in memory (mirroring
//! `assets::pack::tests`' fixture style -- CI has no `pokeemerald/`
//! checkout and no real pack) and exercises the full
//! `OverworldScene::from_pack` + `compose` pipeline against it. The one
//! exception, [`real_pack_composes_non_blank_deterministic_overworld_frames`],
//! is `#[ignore]`d and needs a real local pack.

use std::collections::HashMap;

use super::{
    layout_pack_name, pack_4bpp_region, resolve_tileset_pack_name, OverworldSceneError,
    DEFAULT_ROOM_MAP_ID,
};
use assets::{AssetPack, ImageRef, LayoutId, MapLayout, MovementType, ObjectEvent, TrainerType};
use engine::overworld::{Direction, PlayerState};

// -- `DEFAULT_ROOM_MAP_ID` ----------------------------------------------------

/// [`DEFAULT_ROOM_MAP_ID`]'s own doc comment: kept as an independent literal
/// (rather than importing `crate::new_game::SPAWN_MAP_ID`, which would cycle
/// this module's dependency on `new_game`) -- pin here that the two still
/// agree, so a future edit to either can't silently drift the other.
#[test]
fn default_room_map_id_matches_new_games_spawn_map_id() {
    assert_eq!(DEFAULT_ROOM_MAP_ID, crate::new_game::SPAWN_MAP_ID);
}

// -- `resolve_tileset_pack_name` / `layout_pack_name` -----------------------

#[test]
fn resolve_tileset_pack_name_covers_every_bundled_tileset() {
    assert_eq!(resolve_tileset_pack_name("gTileset_General"), Ok("general"));
    assert_eq!(
        resolve_tileset_pack_name("gTileset_Building"),
        Ok("building")
    );
    assert_eq!(
        resolve_tileset_pack_name("gTileset_Petalburg"),
        Ok("petalburg")
    );
    assert_eq!(
        resolve_tileset_pack_name("gTileset_BrendansMaysHouse"),
        Ok("brendans_mays_house")
    );
    assert_eq!(resolve_tileset_pack_name("gTileset_Lab"), Ok("lab"));
}

#[test]
fn resolve_tileset_pack_name_rejects_an_unbundled_symbol() {
    assert_eq!(
        resolve_tileset_pack_name("gTileset_Underwater"),
        Err(OverworldSceneError::UnknownTileset("gTileset_Underwater"))
    );
}

#[test]
fn layout_pack_name_strips_the_prefix_and_lowercases() {
    assert_eq!(
        layout_pack_name(LayoutId("LAYOUT_LITTLEROOT_TOWN")),
        "littleroot_town"
    );
    assert_eq!(
        layout_pack_name(LayoutId(
            "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE"
        )),
        "littleroot_town_professor_birchs_lab_with_table"
    );
}

// -- `pack_4bpp_region` ------------------------------------------------------

#[test]
fn pack_4bpp_region_rejects_a_payload_shorter_than_declared_dimensions() {
    let pixels = vec![0u8; 63];
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = pack_4bpp_region("bogus", image, 0, 0, 8, 8).unwrap_err();
    assert_eq!(
        err,
        OverworldSceneError::ImagePixelCountMismatch {
            label: "bogus",
            width: 8,
            height: 8,
            actual: 63,
        }
    );
}

#[test]
fn pack_4bpp_region_rejects_a_region_not_a_multiple_of_8() {
    let pixels = vec![0u8; 8 * 8];
    let image = ImageRef {
        width: 8,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let err = pack_4bpp_region("bogus", image, 0, 0, 5, 8).unwrap_err();
    assert_eq!(
        err,
        OverworldSceneError::ImageNotTileAligned {
            label: "bogus",
            width: 8,
            height: 8,
        }
    );
}

#[test]
fn pack_4bpp_region_crops_the_requested_sub_rectangle() {
    // 16x8 (2x1 tiles): left tile all pixel value 3, right tile all value 4.
    let mut pixels = vec![3u8; 16 * 8];
    for y in 0..8 {
        pixels[y * 16 + 8..y * 16 + 16].fill(4);
    }
    let image = ImageRef {
        width: 16,
        height: 8,
        bit_depth: 8,
        pixels: &pixels,
    };
    let packed = pack_4bpp_region("test", image, 8, 0, 8, 8).unwrap();
    let tileset = rendering::Tileset::decode(rendering::BitDepth::Bpp4, &packed).unwrap();
    assert_eq!(tileset.len(), 1);
    assert_eq!(tileset.tile(0).unwrap().index(0, 0), 4);
}

// -- `OverworldSceneError` ---------------------------------------------------

#[test]
fn is_pack_missing_is_true_only_for_not_found() {
    let missing = OverworldSceneError::from(assets::PackError::NotFound("/x".into()));
    assert!(missing.is_pack_missing());
    let other = OverworldSceneError::from(assets::PackError::BadMagic);
    assert!(!other.is_pack_missing());
}

#[test]
fn load_default_room_reports_pack_missing_when_no_pack_is_extracted() {
    // Mirrors `crate::title::tests::load_default_reports_pack_missing_when_no_pack_is_extracted`'s
    // reasoning: this crate's tests run with `cargo test`'s cwd set to
    // `crates/pokeemerald-rs`, which never has `assets-pack/`.
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = super::load_default_room(&engine::event_data::EventData::new()).unwrap_err();
    assert!(err.is_pack_missing());
}

#[test]
fn load_repo_default_room_looks_only_at_the_checkout_pack() {
    // The point of the repo-pinned loader: it must never consult
    // `AssetPack::default_path`'s earlier rungs. With no checkout pack
    // extracted it reports "pack missing" even where a user pack *is*
    // installed (which `load_default_room` would happily load instead),
    // so `xtask`'s smoke e2e can never validate the wrong bytes
    // `(test-ratchet)`.
    if pack_format::repo_pack_path().is_file() {
        return;
    }
    let err = super::load_repo_default_room(&engine::event_data::EventData::new()).unwrap_err();
    assert!(err.is_pack_missing());
}

#[test]
fn load_room_reports_pack_missing_when_no_pack_is_extracted() {
    // Same reasoning as `load_default_room_reports_pack_missing_when_no_pack_is_extracted`
    // (this function's own doc comment): `load_room` fails at the same
    // `AssetPack::load_default` call, before it ever reaches the `map_id`
    // lookup that's the only thing distinguishing it from `load_default_room`.
    if AssetPack::default_path().is_file() {
        return;
    }
    let err = super::load_room(
        assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"),
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .unwrap_err();
    assert!(err.is_pack_missing());
}

// -- End-to-end against a synthetic pack -------------------------------------

/// One directory entry for [`write_synthetic_pack`], mirroring
/// `assets::pack::tests`' own fixture-building style (that module's helper
/// is private to `assets`, so this is a small independent copy rather than
/// a shared one).
struct Entry {
    id: &'static str,
    kind_tag: u8,
    meta: Vec<u8>,
    payload: Vec<u8>,
}

/// Serialize `entries` (sorted by id, as the real format requires -- see
/// `assets::pack`'s module docs) into a version-1 pack file's bytes.
fn write_synthetic_pack(mut entries: Vec<Entry>) -> Vec<u8> {
    entries.sort_by(|a, b| a.id.cmp(b.id));

    let header_size = 8 + 4 + 4;
    let mut directory_size = 0usize;
    for e in &entries {
        directory_size += 2 + e.id.len() + 1 + 8 + 8 + e.meta.len();
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

fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

/// A minimal pack covering exactly what [`super::OverworldScene::from_pack`]
/// needs for one synthetic room: a single-tile "general" tileset (used as
/// both primary and secondary), a `width` x `height` layout whose every
/// cell -- and whose border block -- is that tileset's one opaque metatile,
/// and Brendan's walking sheet/palette. Deliberately the tileset
/// [`super::tileset_anims`] animates most (issue #160 review): with the
/// fabricated `tileset/general/anim/...` entries below, every ordinary
/// `from_pack` test keeps exercising the animated copy/patch/decode
/// compose path in default CI, not just in the ignored real-pack tests.
/// The animated regions all land in the primary block's padding here
/// (this fixture's single metatile only references tile 0), so patched
/// frames never change a composed pixel.
fn synthetic_overworld_pack_bytes(width: u16, height: u16) -> Vec<u8> {
    synthetic_overworld_pack_bytes_for("general", width, height)
}

/// [`synthetic_overworld_pack_bytes`], parameterized on the tileset pack
/// name so a test can also build the *unanimated*-primary shape (e.g.
/// `petalburg` -- a real bundled secondary tileset [`super::tileset_anims`]
/// declares no regions for). Animation frame entries are fabricated only
/// for `general`, the animated case.
fn synthetic_overworld_pack_bytes_for(tileset: &str, width: u16, height: u16) -> Vec<u8> {
    write_synthetic_pack(synthetic_overworld_pack_entries_for(tileset, width, height))
}

/// The entries behind [`synthetic_overworld_pack_bytes_for`], exposed so a
/// test can corrupt one deliberately before writing the pack (e.g. the
/// wrong-size animation-frame rejection test below).
fn synthetic_overworld_pack_entries_for(tileset: &str, width: u16, height: u16) -> Vec<Entry> {
    // A single opaque 8x8 tile, every pixel palette index 5.
    let tile_pixels = vec![5u8; 8 * 8];

    // metatiles.bin: one metatile (id 0), every one of its 8 raw entries
    // pointing at combined tile index 0, palette bank 0, no flip -- raw
    // `u16` value `0x0000` for all 8.
    let metatiles: Vec<u8> = std::iter::repeat_n(0u16.to_le_bytes(), 8)
        .flatten()
        .collect();
    // metatile_attributes.bin: metatile 0 is `METATILE_LAYER_TYPE_COVERED`
    // (1) -- bottom/middle draw the opaque tile, top stays transparent, so
    // the player OBJ (drawn between the middle and top layers) is never
    // hidden by this fixture's otherwise-uniform world content.
    let metatile_attrs = (1u16 << 12).to_le_bytes().to_vec();

    // A `width` x `height` grid, every cell metatile 0, elevation 3
    // (`MetatileCell{metatile_id:0, collision:0, elevation:3}.pack()`).
    let grid_cell = assets::MetatileCell {
        metatile_id: 0,
        collision: 0,
        elevation: 3,
    }
    .pack();
    let cells = usize::from(width) * usize::from(height);
    let grid: Vec<u8> = std::iter::repeat_n(grid_cell.to_le_bytes(), cells)
        .flatten()
        .collect();
    let border: Vec<u8> = std::iter::repeat_n(grid_cell.to_le_bytes(), 4)
        .flatten()
        .collect();

    // Palette bank 0: color index 5 (matching `tile_pixels`) is bright red
    // (raw BGR555 `0x001F`); every other bank is empty.
    let bank0_payload = {
        let mut p = vec![0u8; 5 * 2]; // indices 0..4, unused, zeroed.
        p.extend_from_slice(&0x001Fu16.to_le_bytes()); // index 5: red.
        p
    };

    let mut entries = vec![
        Entry {
            id: leaked(format!("tileset/{tileset}/tiles")),
            kind_tag: 0,
            meta: image_meta(8, 8, 4),
            payload: tile_pixels,
        },
        Entry {
            id: leaked(format!("tileset/{tileset}/metatiles")),
            kind_tag: 2,
            meta: vec![],
            payload: metatiles,
        },
        Entry {
            id: leaked(format!("tileset/{tileset}/metatile-attributes")),
            kind_tag: 2,
            meta: vec![],
            payload: metatile_attrs,
        },
        Entry {
            id: "layout/map_test/map",
            kind_tag: 2,
            meta: vec![],
            payload: grid,
        },
        Entry {
            id: "layout/map_test/border",
            kind_tag: 2,
            meta: vec![],
            payload: border,
        },
        Entry {
            id: "sprite/brendan/walking",
            kind_tag: 0,
            meta: image_meta(144, 32, 8),
            // Every pixel palette index 9 (opaque, distinct from the world
            // tileset's index 5).
            payload: vec![9u8; 144 * 32],
        },
        Entry {
            id: "sprite/palette/brendan",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: {
                let mut p = vec![0u8; 9 * 2]; // indices 0..9, unused, zeroed.
                p.extend_from_slice(&0x7C00u16.to_le_bytes()); // index 9: blue.
                p.resize(32, 0);
                p
            },
        },
    ];
    for slot in 0..16u8 {
        let id: &'static str = leaked(format!("tileset/{tileset}/palette/{slot:02}"));
        entries.push(Entry {
            id,
            kind_tag: 1,
            meta: 0u16.to_le_bytes().to_vec(), // color_count 0, filled in below for bank 0.
            payload: vec![],
        });
    }
    // Overwrite the bank-0 placeholder with the real payload above.
    if let Some(bank0) = entries
        .iter_mut()
        .find(|e| e.id == format!("tileset/{tileset}/palette/00"))
    {
        bank0.meta = 6u16.to_le_bytes().to_vec();
        bank0.payload = bank0_payload;
    }

    if tileset == "general" {
        push_general_anim_frames(&mut entries);
    }
    push_unconditional_sprite_palettes(&mut entries);

    entries
}

/// [`synthetic_overworld_pack_entries_for`]'s `"general"` entries, with one
/// extra metatile per entry of `specials` (ids `1..`, each carrying that
/// entry's behavior) appended to `metatiles`/`metatile-attributes` and each
/// entry's own grid cell switched to reference its metatile -- everything
/// else (collision, elevation, tile/palette bytes) stays exactly as the
/// uniform fixture built it, so every special cell renders identically and
/// differs only in the one byte `MapRuntime::metatile_behavior` reads. See
/// [`synthetic_scene_with_special_tile`]/[`synthetic_scene_with_special_tiles`],
/// which wrap this into a scene.
fn synthetic_overworld_pack_entries_with_special_tiles(
    width: u16,
    height: u16,
    specials: &[((u16, u16), u8)],
) -> Vec<Entry> {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);

    for (index, ((sx, sy), special_behavior)) in specials.iter().copied().enumerate() {
        // metatiles.bin: each new metatile's raw entries are identical to
        // metatile 0's (same combined tile index 0) -- only the *attribute*
        // table below tells them apart, which is all `metatile_behavior`
        // consults.
        let metatile: Vec<u8> = std::iter::repeat_n(0u16.to_le_bytes(), 8)
            .flatten()
            .collect();
        let metatiles = entries
            .iter_mut()
            .find(|e| e.id == "tileset/general/metatiles")
            .expect("the general fixture always fabricates its own metatiles entry");
        metatiles.payload.extend_from_slice(&metatile);

        // metatile_attributes.bin: the new metatile keeps the same COVERED
        // layer type as metatile 0 (fixture docs on why COVERED -- the player
        // OBJ must never be hidden by it), only `behavior` differs.
        let attr = ((1u16 << 12) | u16::from(special_behavior)).to_le_bytes();
        let attrs = entries
            .iter_mut()
            .find(|e| e.id == "tileset/general/metatile-attributes")
            .expect("the general fixture always fabricates its own metatile-attributes entry");
        attrs.payload.extend_from_slice(&attr);

        // layout/map_test/map: retarget this position's own cell to the new
        // metatile id -- same collision (0, walkable) and elevation (3) as
        // every other cell in the uniform fixture, so only its behavior
        // differs.
        assert!(
            sx < width && sy < height,
            "special tile ({sx}, {sy}) must lie inside the {width}x{height} fixture"
        );
        let metatile_id =
            u16::try_from(index + 1).expect("a fixture never needs 65k special tiles");
        let cell = assets::MetatileCell {
            metatile_id,
            collision: 0,
            elevation: 3,
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

/// One frame per numbered entry of every animated region
/// [`super::tileset_anims`] declares for `general` (that module's own
/// cadence table), so `AnimatedTileset::load` resolves real entries in the
/// synthetic fixture and the animated compose path stays on in default CI
/// ([`synthetic_overworld_pack_bytes`]'s doc comment). Each frame is
/// exactly its region's transcribed upstream copy length (the table's
/// `tiles` column) -- `load` rejects any other size -- shaped as an
/// 8px-wide column of that many tiles.
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
                id: leaked(format!("tileset/general/anim/{anim}/{n}")),
                kind_tag: 0,
                meta: image_meta(8, 8 * tiles, 4),
                payload: vec![5u8; 8 * 8 * tiles as usize],
            });
        }
    }
}

/// Interns a formatted pack id for the synthetic [`Entry`]'s `&'static
/// str` id field -- test-only, so the leak is bounded and deliberate.
fn leaked(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}

/// The sprite palettes `OverworldScene::from_pack` loads *unconditionally*
/// -- the four generic `npc_1..4` banks and the other protagonist's own
/// (`npc::build_combined_palette`'s own doc comment) -- as empty (0-color)
/// placeholders, which is enough for those lookups to succeed regardless of
/// whether a fixture's object events reference any of them.
///
/// This fixture's player is `PlayerCharacter::Brendan`, so the "other
/// protagonist" bank reads May's palette.
fn push_unconditional_sprite_palettes(entries: &mut Vec<Entry>) {
    let mut placeholder = |id: &'static str| {
        entries.push(Entry {
            id,
            kind_tag: 1,
            meta: 0u16.to_le_bytes().to_vec(),
            payload: vec![],
        });
    };
    for n in 1..=4u8 {
        placeholder(&*Box::leak(
            format!("sprite/palette/npc_{n}").into_boxed_str(),
        ));
    }
    placeholder("sprite/palette/may");
}

/// An object-event-free [`assets::MapEvents`] for
/// [`super::OverworldScene::from_pack`]'s own tests, which don't exercise
/// NPC rendering (the `npc` module's own tests and the real-pack tests
/// below cover that) -- just need *a* `'static` events value to seed the
/// scene.
static NO_OBJECT_EVENTS: assets::MapEvents = assets::MapEvents {
    id: assets::MapId("MAP_TEST"),
    shared_events_map: None,
    object_events: &[],
    warp_events: &[],
    coord_events: &[],
    bg_events: &[],
};

/// An [`super::OverworldScene`] over a freshly written synthetic pack
/// (no local `cargo xtask extract` output needed): a `width` x `height`
/// room of one uniform opaque metatile, Brendan's sprite, and no object
/// events. The scratch pack file is removed before returning -- the scene
/// owns every byte it needs (that type's own docs).
///
/// `pub(crate)`: `crate::flow::overworld_phase`'s own headless tests build
/// an `OverworldPhase` around one of these, so they can drive
/// `OverworldPhase::step` without a real pack.
pub(crate) fn synthetic_scene(width: u16, height: u16) -> super::OverworldScene {
    synthetic_scene_result(
        synthetic_overworld_pack_bytes(width, height),
        "gTileset_General",
        width,
        height,
    )
    .expect("synthetic pack should decode cleanly")
}

/// [`synthetic_scene`], but the single cell at `special_pos` is a second,
/// distinct metatile (id 1) whose behavior is `special_behavior` instead of
/// ordinary ground (id 0, behavior `MB_NORMAL`) -- otherwise identical:
/// fully walkable (no collision anywhere, including `special_pos`'s own
/// neighbors), elevation 3 throughout.
///
/// `crate::flow::overworld_phase`'s own headless tests (issue #194) use this
/// to prove that a *legal, walkable* step in an arrow-warp tile's own
/// direction warps instead of stepping -- something no bundled real map can
/// exercise, since every arrow tile this port's own data reaches has its
/// arrow direction impassable (`OverworldPhase::step`'s "Warp timing" docs;
/// the Brendan's-house doormat's own `(8, 9)` is off-map).
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
/// `crate::flow::wild_encounter`'s own tests (issue #169's review follow-up)
/// need two at once -- a land-encounter tile *and* a door-shaped warp tile on
/// the same map -- to drive upstream's `ProcessPlayerFieldInput` precedence
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

/// [`synthetic_scene`], but the cell at `pos` carries `elevation` instead
/// of the fixture's uniform 3 (same metatile 0, walkable, `MB_NORMAL`).
/// `crate::flow`'s save-continue tests use `ELEVATION_MULTI_LEVEL` (15)
/// here to pin `saved_tile_placement`'s transition substitution -- a
/// branch no uniform-elevation fixture can reach.
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

/// [`synthetic_scene`]'s fallible core, parameterized on the pack bytes and
/// the layout's tileset symbol: writes the scratch pack, runs
/// [`super::OverworldScene::from_pack`], and returns its result -- so
/// tests can assert on rejection errors and on non-`general` fixtures too.
///
/// No declared connections (issue #253): every existing caller's fixture is
/// a single, self-contained room, so `synthetic_scene_result_with_connections`
/// is the one that exercises `header.connections` -- this is the same
/// fixture shape, with an empty connection list.
fn synthetic_scene_result(
    pack_bytes: Vec<u8>,
    tileset_symbol: &'static str,
    width: u16,
    height: u16,
) -> Result<super::OverworldScene, OverworldSceneError> {
    synthetic_scene_result_with_connections(pack_bytes, tileset_symbol, width, height, &[])
}

/// [`synthetic_scene_result`], with `connections` threaded onto the
/// synthetic room's own [`assets::MapHeader`] (issue #253) -- the one
/// fixture path that lets a test build a room whose declared connections
/// `super::OverworldScene::from_pack`'s own `resolve_connections` actually
/// walks. `connections`' own `target` ids still resolve against the *real*
/// generated `assets::MapHeaderTable`/`assets::LayoutTable` (mirroring
/// `crate::flow::overworld_phase::connections::MapConnections`'s identical
/// choice) -- there is no synthetic map-table stand-in -- so a connection
/// test target must be a real bundled `MapId` whose own pack entry this
/// fixture's `pack_bytes` also supplies.
fn synthetic_scene_result_with_connections(
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
/// over a synthetic pack instead of needing a real one. The S-2/#334
/// `HBlank`-interval-free budget regression fixture
/// (`hblank_budget_regression_fixture`, below) is the one caller: it needs
/// enough OAM entries to straddle the normal/reduced per-scanline budget
/// cutoff, which no bundled real map reliably provides on one screen.
fn synthetic_scene_result_with_connections_and_events(
    pack_bytes: Vec<u8>,
    tileset_symbol: &'static str,
    width: u16,
    height: u16,
    connections: &'static [assets::MapConnection],
    events: &'static assets::MapEvents,
) -> Result<super::OverworldScene, OverworldSceneError> {
    // pid + thread id distinguish concurrently-running tests' scratch
    // packs with no shared mutable state `(oop-boundaries)`: the test
    // harness runs each test on its own thread, and this function removes
    // the file before returning, so one thread never has two packs alive.
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-overworld-test-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, pack_bytes).unwrap();
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
        id: assets::MapId("MAP_TEST"),
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

/// Review regression (#192): [`super::tileset_anims`]' load-time size gate
/// must reject a frame whose packed bytes disagree with its region's
/// transcribed upstream copy length -- an oversized frame would silently
/// overwrite a neighboring region's tiles (flower's own 508..512 sits
/// right past waterfall's), an undersized one would leave the region
/// partially stale. Mirrors this module's other malformed-entry pins
/// (`UnknownTileset`, `ImagePixelCountMismatch`, `ImageNotTileAligned`).
#[test]
fn from_pack_rejects_an_anim_frame_of_the_wrong_size() {
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    // Swap flower/0 (upstream copies exactly 4 tiles) for a 6-tile 8x48
    // column -- tile-aligned, dimensions and payload in agreement, so only
    // the new exact-size gate can catch it.
    let frame = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/anim/flower/0")
        .expect("the general fixture fabricates flower/0");
    frame.meta = image_meta(8, 48, 4);
    frame.payload = vec![5u8; 8 * 48];

    let err = synthetic_scene_result(write_synthetic_pack(entries), "gTileset_General", 4, 4)
        .expect_err("a wrong-size anim frame must not load");
    assert_eq!(
        err,
        OverworldSceneError::AnimFrameSizeMismatch {
            anim_name: "flower",
            expected_tiles: 4,
            frame_bytes: 8 * 48 / 2,
        }
    );
    let shown = err.to_string();
    assert!(
        shown.contains("flower") && shown.contains("4 8x8 tiles"),
        "Display must name the region and the expected tile count: {shown}"
    );
}

/// Review regression (#192): a primary tileset [`super::tileset_anims`]
/// declares no regions for takes `from_pack`'s cached-decode branch, whose
/// observable contract is tick-invariance -- the same scene composes
/// pixel-identical frames at any two ticks. Before this test, that branch
/// was exercised by nothing (`petalburg` is only ever a *secondary*
/// tileset on bundled maps).
#[test]
fn an_unanimated_primary_tileset_composes_tick_invariant_frames() {
    let scene = synthetic_scene_result(
        synthetic_overworld_pack_bytes_for("petalburg", 4, 4),
        "gTileset_Petalburg",
        4,
        4,
    )
    .expect("the unanimated synthetic pack should decode cleanly");

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let tick0 = scene.compose(&player, &event_data, 0);
    let tick77 = scene.compose(&player, &event_data, 77);
    assert_eq!(
        tick0.pixels(),
        tick77.pixels(),
        "an unanimated primary tileset must compose tick-invariant frames"
    );
    assert!(
        tick0
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "the composed viewport must be non-blank"
    );
}

#[test]
fn overworld_scene_from_pack_composes_a_non_blank_deterministic_frame() {
    let scene = synthetic_scene(4, 4);

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let first = scene.compose(&player, &event_data, 0);
    let second = scene.compose(&player, &event_data, 0);
    assert_eq!(
        first.pixels(),
        second.pixels(),
        "composing the same player state twice must be deterministic"
    );
    assert!(
        first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "the composed viewport must be non-blank"
    );

    // The world's single opaque metatile (red, palette index 5) covers the
    // whole visible screen -- both the grid interior and the border
    // fallback resolve to it in this fixture.
    assert_eq!(
        first.pixel(0, 0),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888())
    );

    // The player OBJ (blue, palette index 9) must be visible at its fixed
    // screen position, drawn over the world BG layers.
    assert_eq!(
        first.pixel(
            usize::from(super::avatar::PLAYER_OBJ_X),
            usize::from(super::avatar::PLAYER_OBJ_Y)
        ),
        Some(rendering::Bgr555::from_raw(0x7C00).to_rgb888())
    );
}

/// Builds [`compose_applies_the_reduced_954_cycle_hblank_free_oam_budget`]'s
/// own fixture: a synthetic room with `FILLER_COUNT` transparent
/// `OBJ_EVENT_GFX_MOM` NPCs followed by one opaque `OBJ_EVENT_GFX_TWIN`
/// target, all stacked one metatile right of the player -- split out purely
/// to keep that test's own body under `clippy::too_many_lines`, not because
/// anything else reuses it.
fn hblank_budget_regression_fixture() -> (
    super::OverworldScene,
    PlayerState,
    engine::event_data::EventData,
) {
    const FILLER_COUNT: usize = 64;

    let mut pack_entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    // The filler NPCs' entire sheet is palette index 0 -- transparent on
    // real hardware regardless of which bank it draws from -- so every
    // filler spends OAM admission budget without ever painting a pixel.
    pack_entries.push(Entry {
        id: "sprite/mom",
        kind_tag: 0,
        meta: image_meta(144, 32, 8),
        payload: vec![0u8; 144 * 32],
    });
    // The target NPC's sheet is opaque (index 3) everywhere, so its
    // admission is the only thing that can put color at its screen
    // position.
    pack_entries.push(Entry {
        id: "sprite/twin",
        kind_tag: 0,
        meta: image_meta(144, 32, 8),
        payload: vec![3u8; 144 * 32],
    });

    let player_tile = (5_i16, 5_i16);
    // One metatile right of the player -- its own, non-overlapping 16px
    // OAM column, never the player's own index-0 entry's.
    let npc_tile = (player_tile.0 + 1, player_tile.1);

    let mut object_events = Vec::with_capacity(FILLER_COUNT + 1);
    for local_id in 1..=FILLER_COUNT {
        object_events.push(ObjectEvent {
            local_id: u8::try_from(local_id).unwrap(),
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x: npc_tile.0,
            y: npc_tile.1,
            elevation: 3,
            movement_type: MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "0",
        });
    }
    object_events.push(ObjectEvent {
        local_id: u8::try_from(FILLER_COUNT + 1).unwrap(),
        graphics_id: "OBJ_EVENT_GFX_TWIN",
        x: npc_tile.0,
        y: npc_tile.1,
        elevation: 3,
        movement_type: MovementType::FaceDown,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: "0x0",
        flag: "0",
    });
    let object_events: &'static [ObjectEvent] = Box::leak(object_events.into_boxed_slice());
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));

    let scene = synthetic_scene_result_with_connections_and_events(
        write_synthetic_pack(pack_entries),
        "gTileset_General",
        4,
        4,
        &[],
        events,
    )
    .expect("synthetic pack with 65 object events should decode cleanly");

    let player = PlayerState::new(
        (i32::from(player_tile.0), i32::from(player_tile.1)),
        3,
        Direction::South,
    );
    let event_data = engine::event_data::EventData::new();
    (scene, player, event_data)
}

/// S-2/#334 regression: `OverworldScene::compose` must run its `SpriteLayer`
/// under the reduced 954-cycle HBlank-interval-free OAM budget, not the
/// normal 1210-cycle one -- see `super::OverworldScene::compose`'s own doc
/// comment for the `overworld.c:2122-2123` `SetGpuReg(REG_OFFSET_DISPCNT,
/// ...)` citation this wires up.
///
/// Mirrors `crates/rendering/src/sprite.rs`'s own
/// `with_hblank_free_interval_applies_the_reduced_954_cycle_budget` test's
/// transparent-filler/opaque-target arrangement (module docs there), but
/// through the real `from_pack` -> `compose` pipeline instead of a bare
/// `SpriteLayer`, so a regression in *wiring* the flag through -- not just
/// in the budget math itself, which `rendering` already covers -- fails
/// this test too.
///
/// `FILLER_COUNT` (64) transparent `OBJ_EVENT_GFX_MOM` NPCs stack on one
/// screen position ahead of one opaque `OBJ_EVENT_GFX_TWIN` target, each
/// in its own on-screen (`x >= 0`) 16px-wide OAM column one metatile off
/// the player's index-0 entry (the fixture comment above), never
/// overlapping it: each entry costs `TRAVERSAL_COST (2) + (width - 2) == 16`
/// cycles once admitted (`oam_budget.rs`'s own cost model), so the target
/// -- OAM index `FILLER_COUNT + 1` once the player's own index-0 entry is
/// counted -- lands past the reduced budget's ~59-entry cutoff but well
/// inside the normal budget's ~75-entry one. The two control assertions
/// below pin those exact cutoffs against this arrangement before the real
/// regression check relies on them.
#[test]
fn compose_applies_the_reduced_954_cycle_hblank_free_oam_budget() {
    let (scene, player, event_data) = hblank_budget_regression_fixture();

    let target_x =
        usize::from(super::avatar::PLAYER_OBJ_X) + usize::try_from(super::METATILE_PX).unwrap();
    let target_y = usize::from(super::avatar::PLAYER_OBJ_Y);

    // Control: built independently over the exact same entries/tiles/palette
    // `compose` draws from, the normal 1210-cycle budget still admits the
    // target NPC, and the reduced 954-cycle one drops it -- pinning that
    // this arrangement actually straddles the two budgets' cutoffs.
    let sprite_entries = scene.sprites.entries(&player, &event_data);
    let normal_budget = rendering::SpriteLayer::new(
        &sprite_entries,
        scene.sprites.tiles(),
        scene.sprites.tiles(),
        scene.sprites.palette(),
    );
    assert!(
        normal_budget.resolve_pixel(target_x, target_y).is_some(),
        "control: the normal 1210-cycle budget must still admit the target NPC"
    );
    let reduced_budget = rendering::SpriteLayer::new(
        &sprite_entries,
        scene.sprites.tiles(),
        scene.sprites.tiles(),
        scene.sprites.palette(),
    )
    .with_hblank_free_interval(true);
    assert!(
        reduced_budget.resolve_pixel(target_x, target_y).is_none(),
        "control: the reduced 954-cycle budget must drop the target NPC"
    );

    // The regression: `OverworldScene::compose`'s own composed frame must
    // match the reduced-budget control above, not the normal one -- the
    // world's uniform red metatile (module docs on the sibling test above)
    // fills back in once the target NPC is gone, since every filler NPC
    // ahead of it is transparent.
    let frame = scene.compose(&player, &event_data, 0);
    assert_eq!(
        frame.pixel(target_x, target_y),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888()),
        "the overworld's own composed frame must drop the same late NPC the \
         954-cycle budget drops"
    );
}

/// A real, registered `MapId`/`LayoutId` (issue #253's own connection-test
/// target) with no bearing on this fixture beyond its identity and real
/// dimensions (11x9, `LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F`,
/// `crates/assets/src/map_layouts.rs`): `OverworldScene::from_pack`'s
/// `resolve_connections` walks the *real* generated `MapHeaderTable`/
/// `LayoutTable` to resolve a connection's target (mirroring
/// `crate::flow::overworld_phase::connections::MapConnections`'s identical
/// choice), so a synthetic connection test needs a real map id to point at
/// -- there is no synthetic map-table stand-in. Its own real house-interior
/// content is irrelevant here; only its declared width/height matter, and
/// this fixture supplies its own hand-built `map.bin` bytes for it below
/// rather than reading the real pack.
const CONNECTION_TARGET: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F");
const CONNECTION_TARGET_WIDTH: u16 = 11;
const CONNECTION_TARGET_HEIGHT: u16 = 9;

/// [`overworld_scene_from_pack_composes_a_non_blank_deterministic_frame`]'s
/// own synthetic fixture, extended with one declared South connection
/// (issue #253) whose target is [`CONNECTION_TARGET`]: a second physical
/// tile (index 1, palette index 6 -- distinct from the base fixture's index
/// 5) and a second metatile (id 1, `Covered`, same shape as the base
/// fixture's own metatile 0) referencing it, plus a
/// [`CONNECTION_TARGET_WIDTH`]x[`CONNECTION_TARGET_HEIGHT`]
/// `layout/littleroot_town_mays_house_1f/map` entry whose every cell is
/// that new metatile -- so a composed pixel sampling the connected map's
/// content is visibly distinct (color) from one sampling the active grid or
/// the border block (both still metatile 0, per the base fixture's own
/// docs).
fn connected_overworld_pack_entries(width: u16, height: u16) -> Vec<Entry> {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);

    // Extend `tileset/general/tiles` from one 8x8 tile (index 0, palette
    // index 5) to two, stacked vertically (8x16): a new opaque tile, index
    // 1, every pixel palette index 6.
    let tiles = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/tiles")
        .expect("the general fixture always fabricates its own tiles entry");
    tiles.meta = image_meta(8, 16, 4);
    tiles.payload.extend(std::iter::repeat_n(6u8, 8 * 8));

    // A second metatile (id 1), `Covered` like metatile 0 (module docs on
    // why -- the player OBJ must never be hidden by it), every raw entry
    // pointing at the new tile index 1.
    let metatile: Vec<u8> = std::iter::repeat_n(1u16.to_le_bytes(), 8)
        .flatten()
        .collect();
    entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/metatiles")
        .expect("the general fixture always fabricates its own metatiles entry")
        .payload
        .extend_from_slice(&metatile);
    let attr = (assets::MetatileLayerType::Covered as u16) << 12;
    entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/metatile-attributes")
        .expect("the general fixture always fabricates its own metatile-attributes entry")
        .payload
        .extend_from_slice(&attr.to_le_bytes());

    // Palette bank 0, index 6: distinct from index 5's red (green, raw
    // BGR555 `0x03E0`).
    let bank0 = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/00")
        .expect("the general fixture always fabricates its own bank-0 palette entry");
    bank0.meta = 7u16.to_le_bytes().to_vec();
    bank0.payload.extend_from_slice(&0x03E0u16.to_le_bytes());

    // The connected map's own `map.bin`: every cell metatile 1 (the new
    // marker tile), walkable, elevation 3 -- collision/elevation are
    // irrelevant here (rendering only), matching the base fixture's own
    // convention.
    let target_cell = assets::MetatileCell {
        metatile_id: 1,
        collision: 0,
        elevation: 3,
    }
    .pack();
    let target_cells = usize::from(CONNECTION_TARGET_WIDTH) * usize::from(CONNECTION_TARGET_HEIGHT);
    let target_grid: Vec<u8> = std::iter::repeat_n(target_cell.to_le_bytes(), target_cells)
        .flatten()
        .collect();
    entries.push(Entry {
        id: "layout/littleroot_town_mays_house_1f/map",
        kind_tag: 2,
        meta: vec![],
        payload: target_grid,
    });

    entries
}

/// Issue #253's own wiring test: a declared South connection actually
/// reaches composed pixels, and a position past a *different*, undeclared
/// edge still falls back to the border block -- the two acceptance-test
/// halves (I-4, V-4) at the synthetic-fixture level, ahead of the real-pack
/// Littleroot/Route 101 test below.
#[test]
fn compose_renders_a_declared_connections_tiles_past_the_active_grids_own_edge() {
    let connections: &'static [assets::MapConnection] = &[assets::MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: CONNECTION_TARGET,
    }];
    let scene = synthetic_scene_result_with_connections(
        write_synthetic_pack(connected_overworld_pack_entries(4, 4)),
        "gTileset_General",
        4,
        4,
        connections,
    )
    .expect("the connection fixture should decode cleanly");

    // At rest, standing at the active grid's own origin: the resting
    // viewport's bottom row samples world y == 4 -- one row south of the
    // 4x4 grid's own last row (module docs' anchor math: `anchor_y == -5`,
    // `VIEW_ROWS == 10`, so the last sampled row is `-5 + 9 == 4`).
    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let frame = scene.compose(&player, &event_data, 0);

    // Screen (112, 144): world (0, 4) -- south of the grid, within the
    // connected map's own width (target x == 0) -- must show the connected
    // map's own marker tile (green, palette index 6), not the border block.
    assert_eq!(
        frame.pixel(112, 144),
        Some(rendering::Bgr555::from_raw(0x03E0).to_rgb888()),
        "a cell covered by the declared South connection must render the \
         connected map's own tile"
    );

    // Screen (96, 144): world (-1, 4) -- simultaneously south of the grid
    // *and* west of it (x < 0), but this fixture declares no West
    // connection. The South connection's own bounds check
    // (`connected_cell_at`) does not cover negative x, so this must still
    // fall all the way back to the border block (red, palette index 5,
    // this fixture's border/grid color -- module docs on the base fixture).
    assert_eq!(
        frame.pixel(96, 144),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888()),
        "a position past an edge with no declared connection must still \
         use the border block"
    );
}

/// A connection declared in the header but whose target map/layout/grid
/// can't be resolved against the pack (here: no real generated `MapId`
/// matches) must not be surfaced as an error, and must render exactly as
/// if it were not declared at all -- [`super::ConnectedLayout`]'s own doc comment
/// on why an unresolvable connection is simply omitted. This is also the
/// state of every bundled interior room and of any bundled outdoor map's
/// declared connection into a not-yet-extracted neighbour (e.g. Route
/// 101's own connection into Oldale Town -- [`super::viewport::build_tilemaps`]'s
/// docs).
#[test]
fn an_unresolvable_connection_falls_back_to_the_border_exactly_like_no_connection() {
    let unresolvable: &'static [assets::MapConnection] = &[assets::MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: assets::MapId("MAP_THIS_ID_DOES_NOT_EXIST"),
    }];
    let with_connection = synthetic_scene_result_with_connections(
        write_synthetic_pack(synthetic_overworld_pack_entries_for("general", 4, 4)),
        "gTileset_General",
        4,
        4,
        unresolvable,
    )
    .expect("an unresolvable connection must not fail from_pack");
    let without_connection = synthetic_scene(4, 4);

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    assert_eq!(
        with_connection.compose(&player, &event_data, 0).pixels(),
        without_connection.compose(&player, &event_data, 0).pixels(),
        "a declared but unresolvable connection must render identically to \
         no connection at all"
    );
}

/// Review regression (#253), the *other* silent-omission path: the target
/// map and its layout both resolve against the real generated tables (it's
/// [`CONNECTION_TARGET`], a real bundled `MapId`), but the pack carries no
/// `layout/<name>/map` entry for it at all. Still not an error -- that's
/// exactly a not-yet-extracted neighbour -- and still observably identical
/// to no connection, the same as the unknown-`MapId` case above
/// ([`super::ConnectedLayout`]'s docs on the two omission modes).
#[test]
fn a_connection_whose_target_has_no_pack_entry_is_omitted_not_an_error() {
    let connections: &'static [assets::MapConnection] = &[assets::MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: CONNECTION_TARGET,
    }];
    // The *base* fixture, deliberately not `connected_overworld_pack_entries`:
    // it fabricates no `layout/littleroot_town_mays_house_1f/map` entry, so
    // `resolve_connections` clears the header/layout table lookups and then
    // fails at the pack lookup itself.
    let with_connection = synthetic_scene_result_with_connections(
        write_synthetic_pack(synthetic_overworld_pack_entries_for("general", 4, 4)),
        "gTileset_General",
        4,
        4,
        connections,
    )
    .expect("a bundled target with no pack entry must not fail from_pack");
    assert!(
        with_connection.connections.is_empty(),
        "the connection must be omitted, not stored"
    );

    let without_connection = synthetic_scene(4, 4);
    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    assert_eq!(
        with_connection.compose(&player, &event_data, 0).pixels(),
        without_connection.compose(&player, &event_data, 0).pixels(),
        "a connection whose target isn't in the pack must render identically \
         to no connection at all"
    );
}

/// Review regression (#253): a connected target's map entry is optional
/// only when it is absent. A present entry of another pack kind is corrupt
/// and must retain the exact pack lookup error rather than disappearing as
/// though the neighbour had not been bundled.
#[test]
fn a_connection_target_with_the_wrong_pack_entry_kind_is_an_error_not_a_silent_omission() {
    let connections: &'static [assets::MapConnection] = &[assets::MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: CONNECTION_TARGET,
    }];
    let mut entries = connected_overworld_pack_entries(4, 4);
    let target_grid = entries
        .iter_mut()
        .find(|e| e.id == "layout/littleroot_town_mays_house_1f/map")
        .expect("the connection fixture always fabricates the target's map entry");
    target_grid.kind_tag = 1;
    target_grid.meta = 0u16.to_le_bytes().to_vec();

    let err = synthetic_scene_result_with_connections(
        write_synthetic_pack(entries),
        "gTileset_General",
        4,
        4,
        connections,
    )
    .expect_err("a present connection grid of the wrong kind must be reported");
    assert_eq!(
        err,
        OverworldSceneError::Pack(assets::PackError::WrongKind {
            id: "layout/littleroot_town_mays_house_1f/map".to_owned(),
            expected: "raw blob",
            actual: "palette",
        }),
        "the connected map's pack lookup error must reach the caller verbatim"
    );
}

/// Review regression (#253): a connection target that *is* in the pack but
/// whose `map.bin` bytes are too short for its own declared dimensions is a
/// corrupt pack, not a missing neighbour -- it must surface as an
/// `OverworldSceneError` out of `from_pack` rather than being swallowed
/// into a silent border fallback. Mirrors the *active* map's own
/// `grid_bytes` validation, which has always propagated the identical
/// `AssetError` ([`super::ConnectedLayout`]'s docs on the asymmetry this
/// closes).
#[test]
fn a_connection_target_with_a_truncated_grid_is_an_error_not_a_silent_omission() {
    let connections: &'static [assets::MapConnection] = &[assets::MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: CONNECTION_TARGET,
    }];
    let mut entries = connected_overworld_pack_entries(4, 4);
    let target_grid = entries
        .iter_mut()
        .find(|e| e.id == "layout/littleroot_town_mays_house_1f/map")
        .expect("the connection fixture always fabricates the target's map entry");
    // One cell short of the target layout's own 11x9: present, decodable as
    // a pack entry, and rejected by `MapLayout::grid`.
    let full = target_grid.payload.len();
    target_grid.payload.truncate(full - 2);

    let err = synthetic_scene_result_with_connections(
        write_synthetic_pack(entries),
        "gTileset_General",
        4,
        4,
        connections,
    )
    .expect_err("a present-but-truncated connection grid must be reported");
    assert_eq!(
        err,
        OverworldSceneError::Asset(assets::AssetError::LayoutGridTooShort(
            "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
            usize::from(CONNECTION_TARGET_WIDTH) * usize::from(CONNECTION_TARGET_HEIGHT) * 2,
            full - 2,
        )),
        "the connected map's own grid error must reach the caller verbatim"
    );
}

/// Loads the *real* local pack (`cargo xtask extract`'s output) and
/// exercises the full `load_default_room` + `compose` pipeline against
/// the protagonist's bedroom (Brendan's house 2F) -- proof this module's
/// decoding agrees with the real extracted overworld assets, not just the
/// synthetic fixture above. Needs a local pack: run `cargo xtask extract`
/// first, then `cargo test -p pokeemerald-rs -- --ignored`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_overworld_frames() {
    // The production fresh-save flag store, so the bedroom renders exactly
    // as it would for a real new game -- same reasoning as
    // `fresh_save_event_data` below: a fixture that claims to *be*
    // production state has to come from production state.
    let event_data = fresh_save_event_data();
    let scene = super::load_default_room(&event_data).expect("run `cargo xtask extract` first");

    // A couple of on-foot player states across the bedroom's 9x8 interior
    // (`LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`), including one mid-step,
    // so the scroll-lag path is exercised too.
    let standing = PlayerState::new((5, 5), 3, Direction::South);
    let frame_a = scene.compose(&standing, &event_data, 0);
    let frame_b = scene.compose(&standing, &event_data, 0);
    assert_eq!(
        frame_a.pixels(),
        frame_b.pixels(),
        "composing the same player state twice must be deterministic"
    );
    assert!(
        frame_a
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "a standing player's frame must be non-blank"
    );

    let facing_north = PlayerState::new((5, 4), 3, Direction::North);
    let frame_c = scene.compose(&facing_north, &event_data, 0);
    assert!(
        frame_c
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "a different facing/position must also compose non-blank"
    );
    assert_ne!(
        frame_a.pixels(),
        frame_c.pixels(),
        "a different player position/facing must change the composed frame"
    );
}

// -- Real-pack NPC rendering (issue #161) ------------------------------------

/// Brendan's House 1F: the only bundled map whose *fresh-save* object
/// events include NPCs this module actually draws (Mom at `(2, 6)` and the
/// rival's mom at `(2, 7)`, hidden on a fresh male save); the bedroom's own two events both resolve to
/// nothing renderable on a fresh save, which is why these tests use 1F.
const ONE_F: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

/// `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM`
/// (`include/constants/flags.h:809`) -- deliberately *not* one of
/// [`assets::RESET_MAP_FLAGS`], so Mom is visible on a fresh save and
/// setting this id is an observable change.
const FLAG_HIDE_BRENDANS_HOUSE_MOM: u16 = 0x2F6;

/// The fresh-save flag store [`crate::new_game::init_save_blocks_for_new_game`]
/// builds, so these tests see exactly the object-event visibility a real new
/// game would.
///
/// **Delegates to the production constructor** rather than rebuilding the
/// flag set from [`assets::RESET_MAP_FLAGS`]. It used to do the latter,
/// which silently stopped matching its own doc comment the moment
/// `init_save_blocks` grew a second effect (the skipped truck sequence's
/// gender branch -- `crate::new_game::apply_truck_intro_flags`): the tests
/// below kept passing against a flag set no real save ever has. A helper
/// that claims to *be* production state has to come from production state.
fn fresh_save_event_data() -> engine::event_data::EventData {
    let (block1, _) = crate::new_game::init_save_blocks_for_new_game();
    block1.event_data
}

/// The issue's own "NPCs actually reach the framebuffer" guard, and the
/// mutation guard for [`super::sprites::SceneSprites::entries`]' NPC half:
/// composing 1F with a fresh save must differ, pixel for pixel, from
/// composing it with Mom's hide flag set. Both halves are load-bearing --
/// it pins that Mom *draws* (a no-op NPC OAM step would make the two
/// frames identical) and that the hide-flag gate reaches rendering (an
/// unfiltered NPC list would too).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_hiding_mom_changes_the_composed_1f_frame() {
    let mut data = fresh_save_event_data();
    let scene = super::load_room(ONE_F, super::PlayerCharacter::Brendan, &data)
        .expect("run `cargo xtask extract` first");
    let player = PlayerState::new((2, 7), 3, Direction::North);

    let mom = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .unwrap()
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MOM")
        .expect("1F's Mom object event");
    assert!(
        engine::overworld::object_event_is_visible(mom, &data),
        "this test's own premise: Mom is visible on a fresh save"
    );

    let with_mom = scene.compose(&player, &data, 0);
    data.flag_set(FLAG_HIDE_BRENDANS_HOUSE_MOM).unwrap();
    assert!(
        !engine::overworld::object_event_is_visible(mom, &data),
        "setting FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM must hide her"
    );
    let without_mom = scene.compose(&player, &data, 0);

    assert_ne!(
        with_mom.pixels(),
        without_mom.pixels(),
        "Mom must draw into the frame, and her hide flag must remove her \
         from it"
    );
    assert_eq!(
        without_mom.pixels(),
        scene.compose(&player, &data, 0).pixels(),
        "composing the same state twice must still be deterministic"
    );
}

/// The OAM half of the same guard, against the *real*
/// [`assets::MapEventsTable`] entry rather than a hand-built fixture: the
/// exact entries 1F yields on a fresh save, including each NPC's resolved
/// frame block in the scene's combined sprite tileset and its generic
/// palette bank.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_1f_oam_entries_cover_every_drawn_fresh_save_npc() {
    let data = fresh_save_event_data();
    let scene = super::load_room(ONE_F, super::PlayerCharacter::Brendan, &data)
        .expect("run `cargo xtask extract` first");
    let player = PlayerState::new((2, 7), 3, Direction::North);

    let entries = scene.sprites.entries(&player, &data);
    assert_eq!(
        entries.len(),
        2,
        "the player and Mom, and nobody else. 1F's remaining fresh-save \
         visible object events (both Vigoroths) resolve to no sprite; its \
         dad and the rival are hidden by RESET_MAP_FLAGS; and the rival's \
         *mom* and *sibling* are hidden by the male branch of the skipped \
         truck sequence (`crate::new_game::apply_truck_intro_flags`). This \
         assertion was `3` while that branch went unapplied -- a second, \
         duplicated mother standing in the player's own house, which is \
         exactly the bug it now guards"
    );

    // Entry 0 is always the player, at its fixed screen position.
    assert_eq!(
        entries[0].x(),
        i16::try_from(super::avatar::PLAYER_OBJ_X).unwrap()
    );
    assert_eq!(entries[0].y(), super::avatar::PLAYER_OBJ_Y);
    assert_eq!(entries[0].palette_bank(), 0);

    // The one NPC entry. Mom faces east (`MOVEMENT_TYPE_FACE_RIGHT`), which
    // reuses the west stand frame h-flipped (`avatar`'s frame table), and
    // sits in the combined sprite tileset at the first `FRAME_BLOCK_TILES`
    // stride after the player's own block -- her sheet is packed first, as
    // `object_events[0]`.
    let block = super::avatar::FRAME_BLOCK_TILES;
    let west_stand = super::avatar::FRAME_WEST_STAND * super::avatar::FRAME_TILES;
    let metatile_px = u16::try_from(super::METATILE_PX).unwrap();

    let mom = entries[1];
    assert_eq!(mom.palette_bank(), 4, "OBJ_EVENT_PAL_TAG_NPC_4 (mom.pal)");
    assert_eq!(mom.tile_index(), block + west_stand);
    assert!(mom.h_flip(), "MOVEMENT_TYPE_FACE_RIGHT");
    assert!(mom.enabled());
    assert_eq!(mom.dimensions(), (16, 32));
    assert_eq!(
        mom.x(),
        i16::try_from(super::avatar::PLAYER_OBJ_X).unwrap(),
        "same column as the player at (2, 7)"
    );
    assert_eq!(
        mom.y(),
        super::avatar::PLAYER_OBJ_Y - u8::try_from(metatile_px).unwrap(),
        "one metatile north of the player: Mom stands at (2, 6)"
    );

    // The rival's mother is *not* drawn: her object event at (2, 7) --
    // this fixture's own player tile, where she would have overlapped the
    // player exactly -- is hidden by the truck sequence's male branch.
    let rival_mom = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .unwrap()
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_WOMAN_4")
        .expect("1F declares the rival's mother");
    assert_eq!((rival_mom.x, rival_mom.y), (2, 7));
    assert!(
        !engine::overworld::object_event_is_visible(rival_mom, &data),
        "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_MOM is set for a \
         male player (InsideOfTruck/scripts.inc:29)"
    );

    // The bindings those tile indices address, cross-checked against the
    // packing order `npc::resolve_bindings` walks.
    let bindings = scene.sprites.bindings();
    assert_eq!(bindings.len(), 4, "mom, woman_4, norman, and the rival");
    assert!(
        bindings.contains_key("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL"),
        "the rival is bound (to the player's own sheet) even though \
         RESET_MAP_FLAGS hides him: bindings are per-`graphics_id`, not \
         per-visibility"
    );
    assert!(!bindings.contains_key("OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX"));
    assert!(!bindings.contains_key("OBJ_EVENT_GFX_NINJA_BOY"));
}

/// The opposite-gender rival regression against the *real* pack, both ways
/// round: the rival object event a player can actually meet lives in the
/// other protagonist's house and carries that protagonist's graphics id
/// (`LittlerootTown_MaysHouse_2F/map.json:19` ->
/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`, and vice versa), so a binding must
/// resolve for it whichever gender this run is. Before the fix, playing as
/// Brendan resolved a binding only for `..._RIVAL_BRENDAN_NORMAL` -- the id
/// that stays hidden in his own house -- leaving the real rival undrawn
/// once its hide flag cleared.
///
/// Real-pack, because the point is partly that both protagonists' walking
/// sheets *and* palettes are actually extracted and decodable
/// (`sprite/{brendan,may}/walking`, `sprite/palette/{brendan,may}`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_opposite_gender_rival_binds_for_either_player() {
    const BRENDANS_2F: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    const MAYS_2F: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F");

    let pack = assets::pack::AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let scene_for = |map: assets::MapId, player: super::PlayerCharacter| {
        let header = assets::MapHeaderTable::new().header(map).unwrap();
        let layout = assets::LayoutTable::new().layout(header.layout).unwrap();
        let events = assets::MapEventsTable::new().resolve(map).unwrap();
        super::OverworldScene::from_pack(
            &pack,
            header,
            layout,
            player,
            events,
            &engine::event_data::EventData::new(),
        )
        .expect("a bundled bedroom must decode against the real pack")
    };

    // Playing as Brendan, visiting May's house: her rival object must bind.
    let brendan_in_mays_house = scene_for(MAYS_2F, super::PlayerCharacter::Brendan);
    let binding = brendan_in_mays_house
        .sprites
        .bindings()
        .get("OBJ_EVENT_GFX_RIVAL_MAY_NORMAL")
        .copied()
        .expect("the rival of a Brendan player is May, and she must bind");
    assert_ne!(
        binding.palette_bank(),
        0,
        "the rival draws from its own protagonist palette, not the player's \
         bank 0 -- upstream PALSLOT_NPC_SPECIAL vs PALSLOT_PLAYER"
    );
    assert_ne!(
        binding.base_tile(),
        0,
        "and from its own frame block, not the player's at base tile 0"
    );

    // The mirror image: playing as May, visiting Brendan's house.
    let may_in_brendans_house = scene_for(BRENDANS_2F, super::PlayerCharacter::May);
    let mirrored = may_in_brendans_house
        .sprites
        .bindings()
        .get("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL")
        .copied()
        .expect("the rival of a May player is Brendan, and he must bind");
    assert_eq!(
        (mirrored.palette_bank(), mirrored.base_tile()),
        (binding.palette_bank(), binding.base_tile()),
        "the two configurations are mirror images -- same bank, same stride"
    );

    // And the *resident's own* id still binds, to the already-loaded player
    // sheet at bank 0 / base tile 0 (it is hidden in play, but the binding
    // is correct rather than absent).
    let brendan_at_home = scene_for(BRENDANS_2F, super::PlayerCharacter::Brendan);
    let own = brendan_at_home
        .sprites
        .bindings()
        .get("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL")
        .copied()
        .expect("the same-gender variant reuses the player's own sheet");
    assert_eq!((own.palette_bank(), own.base_tile()), (0, 0));
}

/// The rendering half of [`super::npc`]'s `OBJ_EVENT_GFX_VAR_0` exception
/// (issue #248): Route 103's real rival object event shares that exact
/// `graphics_id` with every bedroom decoration placeholder, so this proves
/// the *scene-decode* path -- not just [`super::npc::resolve_sprite_source`]
/// in isolation -- resolves it to a real sprite binding once
/// `VAR_OBJ_GFX_ID_0` names a real rival id, using the real
/// [`assets::MapEventsTable`] entry against the real pack.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_route_103_rival_binds_to_the_opposite_protagonists_sheet() {
    const ROUTE_103: assets::MapId = assets::MapId("MAP_ROUTE103");
    // `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`) and
    // `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`'s own numeric id
    // (`include/constants/event_objects.h:112`) -- independently
    // transcribed here too, the same convention every module citing these
    // ids uses (`crate::flow::overworld_phase::route103_rival_trigger`'s
    // own docs).
    const VAR_OBJ_GFX_ID_0: u16 = 0x4010;
    const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

    let mut event_data = engine::event_data::EventData::new();
    event_data
        .var_set(VAR_OBJ_GFX_ID_0, RIVAL_MAY_NORMAL_GFX_ID)
        .unwrap();

    let scene = super::load_room(ROUTE_103, super::PlayerCharacter::Brendan, &event_data)
        .expect("run `cargo xtask extract` first");
    let events = assets::MapEventsTable::new().resolve(ROUTE_103).unwrap();
    let rival = events
        .object_events
        .iter()
        .find(|o| o.local_id == 2)
        .expect("Route 103's own rival object event");
    assert_eq!(rival.graphics_id, "OBJ_EVENT_GFX_VAR_0");

    let binding = scene
        .sprites
        .bindings()
        .get("OBJ_EVENT_GFX_VAR_0")
        .copied()
        .expect(
            "VAR_OBJ_GFX_ID_0 naming a real rival id must bind a sprite, not stay `None` \
             like a bedroom decoration",
        );
    assert_ne!(
        binding.palette_bank(),
        0,
        "a male player's rival (May) must draw from the other-protagonist bank, not the \
         player's own bank 0"
    );
    assert_ne!(
        binding.base_tile(),
        0,
        "and from her own frame block, not the player's at base tile 0"
    );
}

// -- Real-pack Oldale Town / Route 103 background NPCs (issue #262) --------

/// One [`super::npc::SpriteBinding`]'s worth of proof that a newly-bound
/// background-NPC `graphics_id` really decodes against the real extracted
/// pack -- not just that [`super::npc::resolve_sprite_source`] names a
/// `sprite/<path>` id (already pinned pack-free by
/// `npc::tests::resolve_sprite_source_resolves_the_oldale_and_route_103_background_npcs`),
/// but that `AssetPack::sprite` actually finds that entry and
/// `avatar::pack_people_sheet_frames` accepts its real dimensions (any
/// mismatch is a real, real-pack-only failure mode --
/// [`super::OverworldSceneError::SpriteSheetWrongDimensions`]/
/// `ImagePixelCountMismatch`/`ImageNotTileAligned`). A nonzero base tile and
/// the expected `npc_<n>` palette bank rule out the two ways a binding could
/// silently be wrong instead of simply absent: reusing the player's own
/// block (base tile `0`) or a different generic palette than upstream's
/// `.paletteTag` names.
fn assert_background_npc_binds(
    bindings: &HashMap<&'static str, super::npc::SpriteBinding>,
    graphics_id: &str,
    expected_palette_bank: u8,
) {
    let binding = bindings
        .get(graphics_id)
        .copied()
        .unwrap_or_else(|| panic!("{graphics_id} must bind against the real pack"));
    assert_eq!(
        binding.palette_bank(),
        expected_palette_bank,
        "{graphics_id}'s palette bank"
    );
    assert_ne!(
        binding.base_tile(),
        0,
        "{graphics_id} must decode its own frame block, not reuse the \
         player's at base tile 0"
    );
}

/// The other half of "no longer invisible": a [`super::npc::SpriteBinding`]
/// alone still draws nothing unless [`super::npc::oam_entries`] actually
/// emits an entry for the object event using it. Mirrors
/// [`real_pack_1f_oam_entries_cover_every_drawn_fresh_save_npc`]'s
/// mechanics -- binding frame block, palette bank, 16x32 shape, and screen
/// position derived from the player's own fixed one -- for a single
/// `graphics_id`, one at a time rather than over a shared player position:
/// `engine::overworld::object_event_is_in_view`'s spawn window is only ~17
/// metatiles wide, and Route 103's trainers are spread across a 70-tile
/// route, so no one player tile holds them all. The player is parked one
/// metatile *north* of the NPC's own template tile, which puts it in that
/// window and makes the expected entry position exactly one metatile south
/// of [`super::avatar::PLAYER_OBJ_Y`] -- the same relationship the 1F test
/// asserts for Mom, with the signs swapped.
///
/// Matching is by frame block rather than by count: bindings' blocks are
/// disjoint by construction, so "exactly one entry addressing this
/// binding's block" identifies this NPC's entry without assuming anything
/// about how many *other* object events happen to share the window (Route
/// 103's berry and cuttable trees are still unbound, and its twins are
/// declared twice).
fn assert_background_npc_draws_an_oam_entry(
    scene: &super::OverworldScene,
    map: assets::MapId,
    data: &engine::event_data::EventData,
    graphics_id: &str,
) {
    // `oldale_town_npc_reposition::resolve_map_events` (issue #281), not a
    // bare `MapEventsTable::resolve`: `scene` above was itself built through
    // `load_room`, which already resolves through that same wrapper, so the
    // expected position/facing this assertion computes below must agree --
    // for Oldale Town's own footprints man and mart employee, that means
    // their post-`OldaleTown_OnTransition` tiles, not their bare map.json
    // ones. A no-op for every other map, Route 103 included.
    let event = super::oldale_town_npc_reposition::resolve_map_events(map)
        .unwrap()
        .object_events
        .iter()
        .find(|o| o.graphics_id == graphics_id)
        .unwrap_or_else(|| panic!("{} declares {graphics_id}", map.0));
    assert!(
        engine::overworld::object_event_is_visible(event, data),
        "{graphics_id}'s object event is fresh-save visible (`\"flag\": \"0\"`)"
    );

    let binding = scene.sprites.bindings()[graphics_id];
    let player = PlayerState::new(
        (i32::from(event.x), i32::from(event.y) - 1),
        event.elevation,
        Direction::South,
    );
    let entries = scene.sprites.entries(&player, data);

    let block = super::avatar::FRAME_BLOCK_TILES;
    let drawn: Vec<_> = entries
        .iter()
        .filter(|entry| {
            (binding.base_tile()..binding.base_tile() + block).contains(&entry.tile_index())
        })
        .collect();
    assert_eq!(
        drawn.len(),
        1,
        "{graphics_id} must contribute exactly one OAM entry, drawn from \
         its own binding's frame block -- a bound but never-emitted NPC is \
         still invisible, which is the bug issue #262 fixes"
    );

    let entry = drawn[0];
    assert_eq!(
        entry.palette_bank(),
        binding.palette_bank(),
        "{graphics_id}'s entry draws from its own binding's palette bank"
    );
    assert!(entry.enabled(), "{graphics_id}'s entry is enabled");
    assert_eq!(
        entry.dimensions(),
        (16, 32),
        "{graphics_id} draws at this module's uniform 16x32 shape"
    );
    assert_eq!(
        entry.x(),
        i16::try_from(super::avatar::PLAYER_OBJ_X).unwrap(),
        "{graphics_id} shares the player's column"
    );
    assert_eq!(
        entry.y(),
        super::avatar::PLAYER_OBJ_Y + u8::try_from(super::METATILE_PX).unwrap(),
        "{graphics_id} stands one metatile south of the player"
    );
}

/// Oldale Town's three fresh-save-visible background NPCs (issue #262):
/// the girl (`GIRL_3`, `data/maps/OldaleTown/map.json:33-45`), the mart
/// employee (`MART_EMPLOYEE`, `:46-59`), and the footprints man/maniac
/// (`MANIAC`, `:60-73`) -- all `"flag": "0"`, so all three are always
/// visible, unlike the town's own rival (`FLAG_HIDE_OLDALE_TOWN_RIVAL`).
/// Palette banks per each id's own `gObjectEventGraphicsInfo_*.paletteTag`
/// (`object_event_graphics_info.h`): `MART_EMPLOYEE` is `NPC_1`, `GIRL_3` is
/// `NPC_2`, `MANIAC` is `NPC_4`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_oldale_town_binds_its_background_npcs() {
    const OLDALE_TOWN: assets::MapId = assets::MapId("MAP_OLDALE_TOWN");
    let data = fresh_save_event_data();
    let scene = super::load_room(OLDALE_TOWN, super::PlayerCharacter::Brendan, &data)
        .expect("run `cargo xtask extract` first");
    let bindings = scene.sprites.bindings();

    for (graphics_id, palette_bank) in [
        ("OBJ_EVENT_GFX_MART_EMPLOYEE", 1),
        ("OBJ_EVENT_GFX_GIRL_3", 2),
        ("OBJ_EVENT_GFX_MANIAC", 4),
    ] {
        assert_background_npc_binds(bindings, graphics_id, palette_bank);
        assert_background_npc_draws_an_oam_entry(&scene, OLDALE_TOWN, &data, graphics_id);
    }
}

/// Route 103's own background NPCs/trainers (issue #262), every one of them
/// `"flag": "0"` on a fresh save (`data/maps/Route103/map.json`) -- unlike
/// its own rival (`FLAG_HIDE_ROUTE_103_RIVAL`) and Professor Birch
/// (`FLAG_HIDE_ROUTE_103_BIRCH`, already bound before this slice). Palette
/// banks per each id's own `gObjectEventGraphicsInfo_*.paletteTag`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_route_103_binds_its_background_npcs() {
    const ROUTE_103: assets::MapId = assets::MapId("MAP_ROUTE103");
    let data = fresh_save_event_data();
    let scene = super::load_room(ROUTE_103, super::PlayerCharacter::Brendan, &data)
        .expect("run `cargo xtask extract` first");
    let bindings = scene.sprites.bindings();

    for (graphics_id, palette_bank) in [
        ("OBJ_EVENT_GFX_MAN_3", 2),
        ("OBJ_EVENT_GFX_WOMAN_2", 3),
        ("OBJ_EVENT_GFX_FISHERMAN", 2),
        ("OBJ_EVENT_GFX_BOY_1", 3),
        ("OBJ_EVENT_GFX_POKEFAN_M", 2),
        ("OBJ_EVENT_GFX_BLACK_BELT", 3),
        ("OBJ_EVENT_GFX_MAN_5", 2),
        ("OBJ_EVENT_GFX_SWIMMER_F", 2),
        ("OBJ_EVENT_GFX_SWIMMER_M", 1),
    ] {
        assert_background_npc_binds(bindings, graphics_id, palette_bank);
        assert_background_npc_draws_an_oam_entry(&scene, ROUTE_103, &data, graphics_id);
    }
}

/// A female saved game must build Route 103 around May's player assets,
/// while the map's variable graphics object resolves independently to
/// Brendan as her rival. This pins the public `load_room` boundary where
/// the saved-gender selection used to be discarded in favor of Brendan.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn female_route_103_room_load_uses_may_for_player_and_brendan_for_rival() {
    const ROUTE_103: assets::MapId = assets::MapId("MAP_ROUTE103");
    const VAR_OBJ_GFX_ID_0: u16 = 0x4010;
    const RIVAL_BRENDAN_NORMAL_GFX_ID: u16 = 100;

    let mut event_data = engine::event_data::EventData::new();
    event_data
        .var_set(VAR_OBJ_GFX_ID_0, RIVAL_BRENDAN_NORMAL_GFX_ID)
        .unwrap();

    let scene = super::load_room(ROUTE_103, super::PlayerCharacter::May, &event_data)
        .expect("run `cargo xtask extract` first");
    // Stays on `load_default` (issue #412): the reference sheets below must
    // come from the same pack `load_room` itself just read, and `load_room`
    // has no pack seam to pin yet.
    let pack = assets::pack::AssetPack::load_default().unwrap();

    let may_pixels = pack.sprite("may/walking").unwrap();
    let may_bytes = super::avatar::pack_people_sheet_frames("may/walking", may_pixels).unwrap();
    let may_tiles = rendering::Tileset::decode(rendering::BitDepth::Bpp4, &may_bytes).unwrap();
    for tile_index in 0..super::avatar::FRAME_BLOCK_TILES {
        assert_eq!(
            scene.sprites.tiles().tile(tile_index),
            may_tiles.tile(tile_index),
            "player tile {tile_index} must come from May's walking sheet"
        );
    }

    let may_palette = pack.sprite_palette("may").unwrap();
    for color_index in 0..rendering::Palette::BANK_LEN {
        #[allow(clippy::cast_possible_truncation)]
        let local_index = color_index as u8;
        assert_eq!(
            scene.sprites.palette().bank_color(0, local_index).raw(),
            may_palette.color(color_index).unwrap() & 0x7FFF,
            "player palette bank 0 color {color_index} must come from May"
        );
    }

    let rival = scene
        .sprites
        .bindings()
        .get("OBJ_EVENT_GFX_VAR_0")
        .copied()
        .expect("Route 103's variable graphics object must bind Brendan");
    assert_ne!(rival.base_tile(), 0, "Brendan needs his own frame block");
    assert_ne!(
        rival.palette_bank(),
        0,
        "Brendan needs a palette bank distinct from May's player bank"
    );

    let brendan_palette = pack.sprite_palette("brendan").unwrap();
    for color_index in 0..rendering::Palette::BANK_LEN {
        #[allow(clippy::cast_possible_truncation)]
        let local_index = color_index as u8;
        assert_eq!(
            scene
                .sprites
                .palette()
                .bank_color(rival.palette_bank(), local_index)
                .raw(),
            brendan_palette.color(color_index).unwrap() & 0x7FFF,
            "rival palette color {color_index} must come from Brendan"
        );
    }
}

// -- Real-pack elevation-driven OBJ priority (S-5, issue #218) ---------------

/// The visual counterpart to
/// `flow::overworld_phase::step_tests::bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation`
/// (this crate's collision-side pin for the same issue): standing on the
/// protagonist bedroom bed's raised elevation-4 edge tile must draw the
/// player OBJ at the raised OAM priority
/// ([`super::avatar::priority_for_elevation`]), not the flat default every
/// position drew before this fix. Real-pack, because the point is that this
/// flows all the way through the production [`super::sprites::SceneSprites::entries`]
/// list built from the real bedroom's own layout, not just
/// [`super::avatar::player_entry`] in isolation (already unit-tested
/// directly in `avatar`'s own module against synthetic elevations).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_bed_side_edge_draws_the_player_obj_at_the_raised_priority() {
    const BEDROOM: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let data = fresh_save_event_data();
    let scene = super::load_room(BEDROOM, super::PlayerCharacter::Brendan, &data)
        .expect("run `cargo xtask extract` first");

    // (0, 7): ordinary floor south of the bed, elevation 3.
    let on_the_floor = PlayerState::new((0, 7), 3, Direction::North);
    let floor_entries = scene.sprites.entries(&on_the_floor, &data);
    assert_eq!(
        floor_entries[0].priority(),
        super::avatar::PLAYER_OBJ_PRIORITY,
        "entry 0 is always the player (this module's other real-pack OAM \
         tests); ordinary floor elevation (3) draws at the default priority"
    );

    // (0, 5): the bed's own raised west-edge tile, elevation 4 -- real
    // authored data, not a synthetic fixture (cross-checked against
    // `pokeemerald/data/layouts/LittlerootTown_BrendansHouse_2F/map.bin` by
    // this crate's `bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation`).
    let on_the_bed_edge = PlayerState::new((0, 5), 4, Direction::North);
    let bed_entries = scene.sprites.entries(&on_the_bed_edge, &data);
    assert_eq!(
        bed_entries[0].priority(),
        1,
        "the bed's raised elevation-4 edge must draw the player OBJ at the \
         raised priority (sElevationToPriority[4] == 1) -- the same-priority-\
         favors-the-sprite tie rule (`rendering::compositor`) then draws it \
         in front of the top BG layer instead of behind, matching a raised \
         surface visually standing above whatever would otherwise occlude \
         a player on ordinary floor"
    );

    // (0, 6): the bed's south-edge ELEVATION_TRANSITION tile, *stepped
    // onto* from the raised edge. `PlayerState::new` seeds both elevation
    // fields identically, so the drifted stance this issue is about --
    // standing on the elevation-0 wildcard while `previous_elevation()`
    // still reads the raised 4 -- is only reachable through the real
    // movement path, exactly as
    // `bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation`
    // walks it on the collision side. (The south edge rather than the
    // north one: with no on-transition script run here, the fresh event
    // data leaves the bedroom's decoration placeholder object standing on
    // the north-edge tile, and that same walk test proves the south edge
    // object-free.)
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    let header = assets::MapHeaderTable::new().header(BEDROOM).unwrap();
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();
    let runtime = scene.runtime(BEDROOM, header, events);
    let mut walker = PlayerState::new((0, 5), 4, Direction::South);
    walker.step(Some(Direction::South), &runtime, &no_connections, &data);
    while walker.in_transit() {
        walker.tick();
    }
    assert_eq!(walker.position(), (0, 6));
    assert_eq!(
        (walker.elevation(), walker.previous_elevation()),
        (0, 4),
        "fixture precondition: the wildcard tile drifts the two fields apart"
    );
    let drifted_entries = scene.sprites.entries(&walker, &data);
    assert_eq!(
        drifted_entries[0].priority(),
        1,
        "the player OBJ's priority must follow previous_elevation (the \
         raised 4), not the wildcard tile's own 0 -- \
         ObjectEventUpdateElevation leaves currentElevation's *rendering* \
         consequence on the last concrete elevation, so the sprite keeps \
         the raised priority instead of flickering to the default for the \
         frames it stands on the transition tile"
    );
}

// -- Real-pack tileset tile animation (issue #160) ---------------------------

/// Upstream `tileset_anims.c`'s General primary-tileset animated tile
/// ranges -- the same literals `crate::overworld::tileset_anims`'s own
/// module docs table pins, transcribed independently here rather than
/// imported, so this test doesn't just check that module agrees with
/// itself.
const GENERAL_ANIMATED_TILE_RANGES: [std::ops::Range<u16>; 5] = [
    508..512, // flower (4 tiles)
    432..462, // water (30 tiles)
    464..474, // sand/water edge (10 tiles)
    496..502, // waterfall (6 tiles)
    480..490, // land/water edge (10 tiles)
];

/// A player position inside `MAP_LITTLEROOT_TOWN` -- the only bundled map on
/// the animated "general" primary tileset (`tileset_anims`'s own scope
/// docs: every bundled interior uses "building", whose one animated
/// metatile, the TV, no bundled interior's own `map.bin` actually places) --
/// whose standing (not-in-transit, so zero-scroll/unpadded -- module docs'
/// camera model) viewport draws animated tiles on screen.
///
/// **Flower is the only animated region any bundled map ever puts on
/// screen.** Measured against the real pack, at this position 24 on-screen
/// BG tile cells reference the flower range and *zero* reference
/// `water`/`sand_water_edge`/`waterfall`/`land_water_edge`; sweeping every
/// cell of `MAP_LITTLEROOT_TOWN` at every standing position, flower peaks at
/// 36 cells (at `(0, 14)`) and the other four regions stay at 0 everywhere
/// -- Littleroot has no water in its own layout, and no other bundled map
/// uses the "general" tileset at all. So [`GENERAL_ANIMATED_TILE_RANGES`]'s
/// other four ranges are pinned by `tileset_anims`' own unit tests, not by
/// any rendered pixel here. Found empirically against the real pack
/// (`overworld::tests` has no typed access to `map.json`'s authored content
/// otherwise).
const LITTLEROOT_TOWN_FLOWER_VIEW: (i32, i32) = (10, 17);

/// The screen-pixel rectangles [`GENERAL_ANIMATED_TILE_RANGES`] paints for
/// `scene`/`player`'s current viewport: every 8x8 BG tile cell whose
/// bottom, middle, or top sub-layer entry names a tile index in one of
/// those ranges, as its `(x0, y0, x1, y1)` on-screen rect. Reaches into
/// [`super::OverworldScene`]'s private fields directly (this module is one
/// of its descendants) to rebuild the same tilemaps [`super::OverworldScene::compose`]
/// would, the same way [`super::viewport`]'s own tests do.
///
/// # Panics
///
/// If `player` [`PlayerState::in_transit`] -- the padded/scrolled viewport
/// that produces doesn't map tile `(col, row)` to screen pixel `(col * 8,
/// row * 8)` in general, and no test here needs it.
fn animated_tile_screen_rects(
    scene: &super::OverworldScene,
    player: &PlayerState,
) -> Vec<(usize, usize, usize, usize)> {
    assert!(
        !player.in_transit(),
        "helper assumes a zero-scroll, unpadded viewport (doc comment)"
    );
    let grid = scene
        .layout
        .grid(&scene.grid_bytes)
        .expect("grid_bytes validated in from_pack");
    let border =
        assets::BorderGrid::new(&scene.border_bytes).expect("border_bytes validated in from_pack");
    let primary_attrs = assets::MetatileAttributeTable::new(&scene.primary_attrs_bytes);
    let secondary_attrs = assets::MetatileAttributeTable::new(&scene.secondary_attrs_bytes);
    // Issue #253: rebuild the same per-connection grids `Self::frame_viewport`
    // would, so this helper stays a faithful stand-in for it even though
    // Littleroot Town's own North connection never actually resolves at
    // this helper's own on-screen-flower fixture positions.
    let connections: Vec<super::viewport::ConnectionView<'_>> = scene
        .connections
        .iter()
        .map(|connection| super::viewport::ConnectionView {
            direction: connection.direction,
            offset: connection.offset,
            grid: connection
                .layout
                .grid(&connection.grid_bytes)
                .expect("grid_bytes validated in resolve_connections"),
        })
        .collect();
    let viewport = super::viewport::build_tilemaps(
        player,
        &grid,
        &border,
        &connections,
        &scene.primary_metatiles,
        &scene.secondary_metatiles,
        &primary_attrs,
        &secondary_attrs,
        scene.blank_tile_index,
    );

    let is_animated = |entry: rendering::ScreenEntry| {
        GENERAL_ANIMATED_TILE_RANGES
            .iter()
            .any(|range| range.contains(&entry.tile_index()))
    };
    let mut rects = Vec::new();
    for row in 0..viewport.bottom.height_tiles() {
        for col in 0..viewport.bottom.width_tiles() {
            let animated = [&viewport.bottom, &viewport.middle, &viewport.top]
                .into_iter()
                .filter_map(|tilemap| tilemap.entry(col, row))
                .any(is_animated);
            if animated {
                rects.push((col * 8, row * 8, col * 8 + 8, row * 8 + 8));
            }
        }
    }
    rects
}

/// The issue #160 acceptance test's "differs" half: composing
/// `MAP_LITTLEROOT_TOWN` at two different ticks changes pixels only inside
/// the on-screen rects [`animated_tile_screen_rects`] identifies -- proving
/// [`super::OverworldScene::compose`]'s `tick` parameter actually reaches
/// rendered pixels, and only the pixels its own animated tile ranges own,
/// for a real map/pack. `tick` 60 is arbitrary but deliberate: General
/// flower -- the one region this map puts on screen at all
/// ([`LITTLEROOT_TOWN_FLOWER_VIEW`]'s docs) -- fires at ticks 16, 32, 48,
/// and 64, so at tick 60 it has latched sequence position 3 (`Frame2`), a
/// frame distinct from the base art tick 0 shows
/// (`tileset_anims::latched_frame`'s doc comment and its own unit tests).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_tick_changes_only_the_animated_tile_screen_regions() {
    let event_data = engine::event_data::EventData::new();
    let scene = super::load_room(
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        super::PlayerCharacter::Brendan,
        &event_data,
    )
    .expect("run `cargo xtask extract` first");
    let (x, y) = LITTLEROOT_TOWN_FLOWER_VIEW;
    let player = PlayerState::new((x, y), 3, Direction::South);

    let rects = animated_tile_screen_rects(&scene, &player);
    assert!(
        !rects.is_empty(),
        "this fixture position's own premise: at least one on-screen tile must \
         reference one of GENERAL_ANIMATED_TILE_RANGES"
    );
    let in_any_rect = |px: usize, py: usize| {
        rects
            .iter()
            .any(|&(x0, y0, x1, y1)| px >= x0 && px < x1 && py >= y0 && py < y1)
    };

    let base = scene.compose(&player, &event_data, 0);
    let animated = scene.compose(&player, &event_data, 60);

    let mut any_diff_inside = false;
    for py in 0..160usize {
        for px in 0..240usize {
            if base.pixel(px, py) != animated.pixel(px, py) {
                assert!(
                    in_any_rect(px, py),
                    "pixel ({px}, {py}) changed between tick 0 and tick 60 outside every \
                     animated tile's own screen rect"
                );
                any_diff_inside = true;
            }
        }
    }
    assert!(
        any_diff_inside,
        "tick 60 must actually change at least one pixel inside an animated rect -- \
         otherwise the rect-containment check above would be vacuous"
    );
}

/// The issue #160 acceptance test's "repeats" half: two ticks a full
/// cadence period apart compose pixel-identically. 128 is the LCM of every
/// region [`crate::overworld::tileset_anims`] ports (General
/// water/sand-water-edge at 128, every other region's own cycle a divisor of
/// it -- that module's own doc comment).
///
/// **The invariant is "for every tick >= 16", not "for any tick".** Below a
/// region's own first fire it shows the *base* art rather than any frame of
/// its sequence, and the base art is not in general equal to the frame the
/// same region shows 128 ticks later: on a map with water on screen,
/// `compose(0)` and `compose(128)` would genuinely differ (water latches
/// sequence position 0 at tick 128, and `water/0` is not the base tile art).
/// 16 is the largest `first_fire` over every region this slice ports (the
/// `phase == 0` regions' `interval`), so from tick 16 on every region has
/// latched and the period is exact everywhere.
///
/// Ticks 0, 1, and 8 are still exercised below, but only because *this*
/// map/position happens to make them true: General flower is the sole
/// animated region on screen here ([`LITTLEROOT_TOWN_FLOWER_VIEW`]'s docs)
/// and its `sequence[0]` frame is pixel-identical to the base art it
/// overwrites, so "pre-first-fire" and "latched frame 0" render the same.
/// They are not evidence for the general invariant -- the ticks from 16 up
/// are.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_tileset_animation_repeats_after_a_full_cadence_period() {
    const FULL_CADENCE_PERIOD: u32 = 128;

    let event_data = engine::event_data::EventData::new();
    let scene = super::load_room(
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        super::PlayerCharacter::Brendan,
        &event_data,
    )
    .expect("run `cargo xtask extract` first");
    let (x, y) = LITTLEROOT_TOWN_FLOWER_VIEW;
    let player = PlayerState::new((x, y), 3, Direction::South);

    // 0/1/8: this map/position only (doc comment). 16/60/127: the real,
    // map-independent invariant -- every region has fired by tick 16.
    for tick in [0, 1, 8, 16, 60, 127] {
        let a = scene.compose(&player, &event_data, tick);
        let b = scene.compose(&player, &event_data, tick + FULL_CADENCE_PERIOD);
        assert_eq!(
            a.pixels(),
            b.pixels(),
            "tick {tick} and tick {} (a full cadence period later) must compose \
             pixel-identically",
            tick + FULL_CADENCE_PERIOD
        );
    }
}

// -- Real-pack connected-map rendering (issue #253, I-4/V-4) ----------------

/// Littleroot Town ↔ Route 101's own real cross-boundary continuity: a
/// camera near Littleroot's north edge (`MAP_LITTLEROOT_TOWN`'s only
/// declared connection, `Direction::North`, `offset: 0`, into
/// `MAP_ROUTE101` -- `crates/assets/src/map_headers.rs`) must show Route
/// 101's own tiles, not a hard cut to the border block, and must show
/// *exactly* the tiles Route 101's own layout renders at the corresponding
/// world position -- no seam.
///
/// A deterministic pixel comparison, not a golden image: rather than
/// pinning specific upstream colors (which would break the moment either
/// map's art changes), this composes the same physical world strip two
/// ways -- once via Littleroot's own north-edge connection fallback, once
/// by loading Route 101 directly and centering the camera on the
/// corresponding position on *its* side (offset 0 means the x axes line up
/// 1:1; the constants below derive the y alignment from that same
/// [`super::PlayerCharacter`]/camera-centering math [`super::viewport`]'s
/// own module docs describe) -- and asserts the two agree, pixel for
/// pixel, everywhere in the shared strip.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_littleroot_and_route_101_render_continuously_across_their_shared_edge() {
    const LITTLEROOT: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN");
    const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

    let scene = super::load_room(
        LITTLEROOT,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    // The pre-#253 shape: the same room, its own declared connections
    // stripped, so composing it reproduces exactly what a hard cut to the
    // border block used to render -- `overworld::tests` is a descendant
    // module of `overworld`, so it can reach this private field directly
    // (the same access `animated_tile_screen_rects` above already uses).
    let mut scene_border_only = super::load_room(
        LITTLEROOT,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    scene_border_only.connections.clear();
    let route_101 = super::load_room(
        ROUTE_101,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let data = fresh_save_event_data();
    // Standing just inside Littleroot's own north edge (row 1 of its own
    // 20x20 grid -- `crates/assets/src/map_layouts.rs`), facing north: at
    // rest, the resting viewport's own anchor (`anchor_y == y -
    // PLAYER_VIEW_ROW == 1 - 5 == -4`, `super::viewport`'s module docs)
    // samples world rows `-4..=5` -- the top four rows (screen rows 0..=3,
    // world rows `-4..=-1`, pixel rows `0..64`) already fall north of
    // Littleroot's own y == 0 edge, squarely in this connection's own
    // territory. All four are inside `viewport::within_backup_map_band`'s
    // own `y >= -7` cover, so the band bound never trims this strip.
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let frame = scene.compose(&player, &data, 0);
    let frame_border_only = scene_border_only.compose(&player, &data, 0);

    // Route 101's own camera, centered so the *same* screen pixel samples
    // the *same* world position `connected_cell_at`'s North-connection
    // formula names (`target_y == connected_height + y`, offset 0 leaves x
    // untouched): matching x (10) and a y whose own `anchor_y` lines up
    // Route 101's local row 19 (its own south edge) with Littleroot's local
    // row -1 (screen row 3 in both compositions) -- `21 - 5 == 16`, and
    // `16 + 3 == 19`, against Littleroot's own `-4 + 3 == -1`.
    let route_101_player = PlayerState::new((10, 21), 3, Direction::North);
    let frame_route_101 = route_101.compose(&route_101_player, &data, 0);

    // The shared strip: screen pixel rows `0..64` -- 4 metatile rows, the
    // full width -- entirely north of Littleroot's own grid at this player
    // position, per the anchor math above. The last of them (screen row 3,
    // world row -1 on Littleroot's side, Route 101's own row 19) is the
    // row immediately adjacent to the seam, the one the comparison most
    // needs to cover (review of #253: the strip used to stop at `0..48`,
    // three rows, excluding exactly that row).
    let mut any_pixel_changed_by_the_connection = false;
    for py in 0..64usize {
        for px in 0..240usize {
            let with_connection = frame.pixel(px, py);
            let without_connection = frame_border_only.pixel(px, py);
            let neighbour_direct = frame_route_101.pixel(px, py);
            assert_eq!(
                with_connection, neighbour_direct,
                "pixel ({px}, {py}) north of Littleroot's own edge must match Route 101's \
                 own rendering at the corresponding world position exactly -- no seam"
            );
            if with_connection != without_connection {
                any_pixel_changed_by_the_connection = true;
            }
        }
    }
    assert!(
        any_pixel_changed_by_the_connection,
        "the connection must actually change at least one pixel versus the pre-#253 \
         border-only fallback, or the two comparisons above would be trivially \
         satisfied by an all-border strip"
    );
}

/// The same seam proof for the cluster's *other* live outdoor edge:
/// Route 101's only North connection (`MAP_OLDALE_TOWN`, `offset: 0` --
/// `crates/assets/src/map_headers.rs`), newly resolvable now that
/// `cargo xtask extract`'s `LAYOUTS` table bundles Oldale Town's grid
/// bytes (issue #248). Identical geometry to
/// [`real_pack_littleroot_and_route_101_render_continuously_across_their_shared_edge`]
/// -- both layouts are 20x20 and the offset is 0 -- so the same anchor
/// math covers the same four-row strip; see that test's comments for the
/// derivation.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_route_101_and_oldale_town_render_continuously_across_their_shared_edge() {
    const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");
    const OLDALE: assets::MapId = assets::MapId("MAP_OLDALE_TOWN");

    let scene = super::load_room(
        ROUTE_101,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let mut scene_border_only = super::load_room(
        ROUTE_101,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    scene_border_only.connections.clear();
    let oldale = super::load_room(
        OLDALE,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let data = fresh_save_event_data();
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let frame = scene.compose(&player, &data, 0);
    let frame_border_only = scene_border_only.compose(&player, &data, 0);

    let oldale_player = PlayerState::new((10, 21), 3, Direction::North);
    let frame_oldale = oldale.compose(&oldale_player, &data, 0);

    let mut any_pixel_changed_by_the_connection = false;
    for py in 0..64usize {
        for px in 0..240usize {
            let with_connection = frame.pixel(px, py);
            let without_connection = frame_border_only.pixel(px, py);
            let neighbour_direct = frame_oldale.pixel(px, py);
            assert_eq!(
                with_connection, neighbour_direct,
                "pixel ({px}, {py}) north of Route 101's own edge must match Oldale Town's \
                 own rendering at the corresponding world position exactly -- no seam"
            );
            if with_connection != without_connection {
                any_pixel_changed_by_the_connection = true;
            }
        }
    }
    assert!(
        any_pixel_changed_by_the_connection,
        "the connection must actually change at least one pixel versus the border-only \
         fallback, or the comparisons above would be trivially satisfied by an \
         all-border strip"
    );
}
