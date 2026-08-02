//! Unit tests for [`super::OverworldScene`] and its private helpers.
//!
//! [`overworld_scene_from_pack_composes_a_non_blank_deterministic_frame`]
//! builds a small **synthetic** pack in memory (mirroring
//! `assets::pack::tests`' fixture style -- CI has no `pokeemerald/`
//! checkout and no real pack) and exercises the full
//! `OverworldScene::from_pack` + `compose` pipeline against it. The one
//! exception, [`real_pack_composes_non_blank_deterministic_overworld_frames`],
//! is `#[ignore]`d and needs a real local pack.

use super::{
    layout_pack_name, pack_4bpp_region, resolve_tileset_pack_name, OverworldSceneError,
    DEFAULT_ROOM_MAP_ID,
};
use assets::{AssetPack, ImageRef, LayoutId, MapLayout};
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
    let err = super::load_default_room().unwrap_err();
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
    let err = super::load_room(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F")).unwrap_err();
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

/// Distinguishes concurrently-running tests' scratch packs (same process
/// id, so the pid alone is not enough).
static NEXT_SYNTHETIC_PACK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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

/// [`synthetic_scene`]'s fallible core, parameterized on the pack bytes and
/// the layout's tileset symbol: writes the scratch pack, runs
/// [`super::OverworldScene::from_pack`], and returns its result -- so
/// tests can assert on rejection errors and on non-`general` fixtures too.
fn synthetic_scene_result(
    pack_bytes: Vec<u8>,
    tileset_symbol: &'static str,
    width: u16,
    height: u16,
) -> Result<super::OverworldScene, OverworldSceneError> {
    let serial = NEXT_SYNTHETIC_PACK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-overworld-test-{}-{serial}.pack",
        std::process::id()
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
    let scene = super::OverworldScene::from_pack(
        &pack,
        &layout,
        super::PlayerCharacter::Brendan,
        &NO_OBJECT_EVENTS,
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

/// Loads the *real* local pack (`cargo xtask extract`'s output) and
/// exercises the full `load_default_room` + `compose` pipeline against
/// the protagonist's bedroom (Brendan's house 2F) -- proof this module's
/// decoding agrees with the real extracted overworld assets, not just the
/// synthetic fixture above. Needs a local pack: run `cargo xtask extract`
/// first, then `cargo test -p pokeemerald-rs -- --ignored`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_overworld_frames() {
    let scene = super::load_default_room().expect("run `cargo xtask extract` first");
    // The production fresh-save flag store, so the bedroom renders exactly
    // as it would for a real new game -- same reasoning as
    // `fresh_save_event_data` below: a fixture that claims to *be*
    // production state has to come from production state.
    let event_data = fresh_save_event_data();

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
    let scene = super::load_room(ONE_F).expect("run `cargo xtask extract` first");
    let player = PlayerState::new((2, 7), 3, Direction::North);

    let mut data = fresh_save_event_data();
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
    let scene = super::load_room(ONE_F).expect("run `cargo xtask extract` first");
    let data = fresh_save_event_data();
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

    let pack = assets::pack::AssetPack::load_default().expect("run `cargo xtask extract` first");
    let scene_for = |map: assets::MapId, player: super::PlayerCharacter| {
        let header = assets::MapHeaderTable::new().header(map).unwrap();
        let layout = assets::LayoutTable::new().layout(header.layout).unwrap();
        let events = assets::MapEventsTable::new().resolve(map).unwrap();
        super::OverworldScene::from_pack(&pack, layout, player, events)
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
    let viewport = super::viewport::build_tilemaps(
        player,
        &grid,
        &border,
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
    let scene = super::load_room(assets::MapId("MAP_LITTLEROOT_TOWN"))
        .expect("run `cargo xtask extract` first");
    let (x, y) = LITTLEROOT_TOWN_FLOWER_VIEW;
    let player = PlayerState::new((x, y), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();

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

    let scene = super::load_room(assets::MapId("MAP_LITTLEROOT_TOWN"))
        .expect("run `cargo xtask extract` first");
    let (x, y) = LITTLEROOT_TOWN_FLOWER_VIEW;
    let player = PlayerState::new((x, y), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();

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
