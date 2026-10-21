//! Unit tests for [`super::OverworldScene`] and its private helpers.

use std::collections::HashMap;

use super::{OverworldSceneError, DEFAULT_ROOM_MAP_ID};
use engine::overworld::{Direction, PlayerState};

use super::test_support::{
    image_meta, synthetic_overworld_pack_entries_for, synthetic_scene, synthetic_scene_result,
    synthetic_scene_result_with_connections, write_synthetic_pack, Entry,
};

/// `DEFAULT_ROOM_MAP_ID` is a literal rather than an import of
/// `crate::new_game::SPAWN_MAP_ID`, to avoid a cyclic dependency on
/// `new_game`; this guards the two values against drifting apart.
#[test]
fn default_room_map_id_matches_new_games_spawn_map_id() {
    assert_eq!(DEFAULT_ROOM_MAP_ID, crate::new_game::SPAWN_MAP_ID);
}

/// A real, registered `MapId`/`LayoutId`
/// (`LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F`, 11x9,
/// `crates/assets/src/map_layouts.rs`): `resolve_connections` walks the
/// real generated `MapHeaderTable`/`LayoutTable`, so a synthetic connection
/// test needs a real map id -- there is no synthetic table stand-in.
const CONNECTION_TARGET: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F");
const CONNECTION_TARGET_WIDTH: u16 = 11;
const CONNECTION_TARGET_HEIGHT: u16 = 9;

/// [`overworld_scene_from_pack_composes_a_non_blank_deterministic_frame`]'s
/// own synthetic fixture, extended with one declared South connection whose
/// target is [`CONNECTION_TARGET`]: a second tile and metatile (green,
/// palette index 6) so a pixel sampling the connected map is visibly
/// distinct from one sampling the active grid or border block (both still
/// the base fixture's red metatile 0).
fn connected_overworld_pack_entries(width: u16, height: u16) -> Vec<Entry> {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);

    let tiles = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/tiles")
        .expect("the general fixture always fabricates its own tiles entry");
    tiles.meta = image_meta(8, 16, 4);
    tiles.payload.extend(std::iter::repeat_n(6u8, 8 * 8));

    // Metatile 1 is `Covered`, like metatile 0, so the player OBJ is never
    // hidden behind it.
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

    let bank0 = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/00")
        .expect("the general fixture always fabricates its own bank-0 palette entry");
    bank0.meta = 7u16.to_le_bytes().to_vec();
    bank0.payload.extend_from_slice(&0x03E0u16.to_le_bytes());

    // Every cell is metatile 1 (marker); collision/elevation are irrelevant
    // here (rendering only).
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
        id: "layout/littleroot_town_mays_house_1f/map".into(),
        kind_tag: 2,
        meta: vec![],
        payload: target_grid,
    });

    entries
}

/// A declared South connection reaches composed pixels, and a position
/// past a different, undeclared edge still falls back to the border block
/// -- the two acceptance-test halves (I-4, V-4) at the synthetic-fixture
/// level, ahead of the real-pack test below.
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

    // Screen row 144 samples world y == 4 (module docs' anchor math:
    // `anchor_y == -5`, viewport row 9): the resting `RESTING_SCROLL_Y == 8`
    // baseline keeps row 9 at screen row 144, `(144 + 8) / 16 == 9`.
    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let frame = scene.compose(&player, &event_data, 0);

    // Screen (112, 144) is world (0, 4): south of the grid, within the
    // connected map's width.
    assert_eq!(
        frame.pixel(112, 144),
        Some(rendering::Bgr555::from_raw(0x03E0).to_rgb888()),
        "a cell covered by the declared South connection must render the \
         connected map's own tile"
    );

    // Screen (96, 144) is world (-1, 4): south of the grid and west of it
    // (x < 0), but no West connection is declared, and the South
    // connection's own bounds check (`connected_cell_at`) does not cover
    // negative x.
    assert_eq!(
        frame.pixel(96, 144),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888()),
        "a position past an edge with no declared connection must still \
         use the border block"
    );
}

/// A connection whose target can't be resolved against the pack (no real
/// generated `MapId` matches) must not surface as an error, and must
/// render exactly as if it were not declared at all --
/// [`super::ConnectedLayout`]'s own doc comment on why an unresolvable
/// connection is simply omitted.
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

/// The *other* silent-omission path: [`CONNECTION_TARGET`] resolves
/// against the real generated tables, but the pack carries no
/// `layout/<name>/map` entry for it. Still not an error, and still
/// observably identical to no connection, like the unknown-`MapId` case
/// above ([`super::ConnectedLayout`]'s docs on the two omission modes).
#[test]
fn a_connection_whose_target_has_no_pack_entry_is_omitted_not_an_error() {
    let connections: &'static [assets::MapConnection] = &[assets::MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: CONNECTION_TARGET,
    }];
    // Deliberately the base fixture, not `connected_overworld_pack_entries`:
    // it fabricates no target map entry, so the pack lookup itself is what
    // fails.
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

/// A connected target's map entry is optional only when it is absent. A
/// present entry of another pack kind is corrupt and must retain the exact
/// pack lookup error rather than disappearing as though the neighbour had
/// not been bundled.
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
    // A well-formed palette of the wrong kind: `pack_format::parse_directory`
    // rejects a `color_count` that doesn't address its payload, and this
    // fixture is about the *kind* reaching the caller, not a corrupt pack
    // failing to load.
    let colors = u16::try_from(target_grid.payload.len() / 2).expect("a u16 grid of colours");
    target_grid.meta = colors.to_le_bytes().to_vec();

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

/// A connection target that *is* in the pack but whose `map.bin` bytes are
/// too short for its declared dimensions is a corrupt pack, not a missing
/// neighbour -- it must surface as an `OverworldSceneError`, not a silent
/// border fallback. Mirrors the *active* map's own `grid_bytes` validation
/// ([`super::ConnectedLayout`]'s docs on the asymmetry this closes).
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

/// `OverworldScene` loaded from the checkout pack for the protagonist's
/// bedroom (Brendan's house 2F) composes deterministic, non-blank frames,
/// and a different frame when the player's position and facing change
/// together. Needs a local pack: run `cargo xtask extract` first, then
/// `cargo test -p pokeemerald-rs -- --ignored`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_overworld_frames() {
    // Fresh-save flags, so the bedroom's object events resolve as in a new game.
    let event_data = fresh_save_event_data();
    let scene =
        super::load_repo_default_room(&event_data).expect("run `cargo xtask extract` first");

    // Two on-foot player states inside the bedroom's 9x8 interior
    // (`LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`).
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

// -- Real-pack NPC rendering -------------------------------------------------

/// Brendan's House 1F: the bundled map whose fresh-save object events
/// include a drawn NPC (Mom at `(2, 6)`); the rival's mom at `(2, 7)` is
/// hidden on a fresh male save. The bedroom's two object events resolve to
/// nothing renderable on a fresh save, so the NPC tests use 1F.
const ONE_F: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

/// `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM`
/// (`include/constants/flags.h:809`). It is not in
/// [`assets::RESET_MAP_FLAGS`], so Mom is visible on a fresh save and
/// setting the flag hides her.
const FLAG_HIDE_BRENDANS_HOUSE_MOM: u16 = 0x2F6;

/// The `EventData` that [`crate::new_game::init_save_blocks_for_new_game`]
/// builds, so object-event visibility matches a real new game. It calls the
/// production constructor instead of rebuilding the flag set, so every flag
/// the new-game path sets (reset-map flags, the wall-clock Vigoroth flags,
/// the truck-intro gender flags) is present.
fn fresh_save_event_data() -> engine::event_data::EventData {
    let (block1, _) = crate::new_game::init_save_blocks_for_new_game();
    block1.event_data
}

/// Composing 1F on a fresh save differs pixel for pixel from composing it
/// with Mom's hide flag set. That pins both that Mom is drawn by
/// [`super::sprites::SceneSprites::entries`] (without NPC entries the two
/// frames would match) and that the hide flag reaches rendering (an
/// unfiltered NPC list would match too).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_hiding_mom_changes_the_composed_1f_frame() {
    let mut data = fresh_save_event_data();
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ONE_F,
        super::PlayerCharacter::Brendan,
        &data,
    )
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

/// [`super::sprites::SceneSprites::entries`] for 1F on a fresh save yields
/// exactly the player and Mom, with Mom's frame block in the combined sprite
/// tileset, generic palette bank, facing, and screen position taken from the
/// real [`assets::MapEventsTable`] entry.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_1f_oam_entries_cover_every_drawn_fresh_save_npc() {
    let data = fresh_save_event_data();
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ONE_F,
        super::PlayerCharacter::Brendan,
        &data,
    )
    .expect("run `cargo xtask extract` first");
    let player = PlayerState::new((2, 7), 3, Direction::North);

    let entries = scene.sprites.entries(&player, &data);
    assert_eq!(
        entries.len(),
        2,
        "the player and Mom, and nobody else: 1F's Vigoroths are hidden by \
         `crate::new_game::apply_wall_clock_vigoroth_flags`; its dad and \
         the rival are hidden by RESET_MAP_FLAGS; the rival's mom and \
         sibling are hidden by the male branch of \
         `crate::new_game::apply_truck_intro_flags`"
    );

    // Entry 0 is always the player, at its fixed screen position.
    assert_eq!(
        entries[0].x(),
        i16::try_from(super::avatar::PLAYER_OBJ_X).unwrap()
    );
    assert_eq!(entries[0].y(), super::avatar::PLAYER_OBJ_Y);
    assert_eq!(entries[0].palette_bank(), 0);

    // Mom faces east (`MOVEMENT_TYPE_FACE_RIGHT`), which reuses the west
    // stand frame h-flipped (`avatar::stand_frame_for`). Her sheet is packed
    // first (`object_events[0]`), one `FRAME_BLOCK_TILES` stride after the
    // player's block.
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

    // The rival's mother is not drawn: her object event at (2, 7), the
    // player's own tile, is hidden by the truck sequence's male branch.
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

    // Bindings follow the packing order `npc::resolve_bindings` walks.
    let bindings = scene.sprites.bindings();
    assert_eq!(bindings.len(), 4, "mom, woman_4, norman, and the rival");
    assert!(
        bindings.contains_key("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL"),
        "the rival is bound to the player's own sheet even though \
         RESET_MAP_FLAGS hides him: bindings are per `graphics_id`, not per \
         visibility"
    );
    assert!(!bindings.contains_key("OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX"));
    assert!(!bindings.contains_key("OBJ_EVENT_GFX_NINJA_BOY"));
}

/// `SceneSprites::bindings` binds the opposite-gender rival for either
/// player. The rival a player can meet lives in the other protagonist's
/// house and carries that protagonist's graphics id
/// (`LittlerootTown_MaysHouse_2F/map.json:19` ->
/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`, and vice versa), so a Brendan player
/// must bind `..._RIVAL_MAY_NORMAL` and a May player `..._RIVAL_BRENDAN_NORMAL`.
///
/// Uses the real pack because both protagonists' walking sheets and palettes
/// must be extracted and decodable (`sprite/{brendan,may}/walking`,
/// `sprite/palette/{brendan,may}`).
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

    // The resident's own id binds to the player's sheet at bank 0 / base
    // tile 0, even though that object event is hidden in play.
    let brendan_at_home = scene_for(BRENDANS_2F, super::PlayerCharacter::Brendan);
    let own = brendan_at_home
        .sprites
        .bindings()
        .get("OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL")
        .copied()
        .expect("the same-gender variant reuses the player's own sheet");
    assert_eq!((own.palette_bank(), own.base_tile()), (0, 0));
}

/// With `VAR_OBJ_GFX_ID_0` naming a rival graphics id, `SceneSprites` binds
/// Route 103's `OBJ_EVENT_GFX_VAR_0` rival object event to the opposite
/// protagonist's sheet. This exercises scene decode (not just
/// [`super::npc::resolve_sprite_source`]) against the real
/// [`assets::MapEventsTable`] entry and pack.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_route_103_rival_binds_to_the_opposite_protagonists_sheet() {
    const ROUTE_103: assets::MapId = assets::MapId("MAP_ROUTE103");
    // `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`) and
    // `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL` (`include/constants/event_objects.h:112`).
    const VAR_OBJ_GFX_ID_0: u16 = 0x4010;
    const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

    let mut event_data = engine::event_data::EventData::new();
    event_data
        .var_set(VAR_OBJ_GFX_ID_0, RIVAL_MAY_NORMAL_GFX_ID)
        .unwrap();

    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_103,
        super::PlayerCharacter::Brendan,
        &event_data,
    )
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
        .expect("VAR_OBJ_GFX_ID_0 naming a rival id must bind a sprite for OBJ_EVENT_GFX_VAR_0");
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

// -- Real-pack Oldale Town / Route 103 background NPCs -----------------------

/// Asserts that `graphics_id` has a [`super::npc::SpriteBinding`] in
/// `bindings` with `expected_palette_bank` and its own frame block. The
/// binding exists only if `AssetPack::sprite` finds the entry and
/// `avatar::pack_people_sheet_frames` accepts its dimensions
/// ([`super::OverworldSceneError::SpriteSheetWrongDimensions`],
/// `ImagePixelCountMismatch`, `ImageNotTileAligned`). A nonzero base tile
/// rules out reusing the player's block (base tile `0`), and the palette
/// bank must match the upstream `.paletteTag` of the graphics info.
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

/// Asserts that [`super::sprites::SceneSprites::entries`] emits exactly one
/// OAM entry for the fresh-save-visible object event using `graphics_id`,
/// with the binding's frame block and palette bank, a 16x32 shape, and the
/// player's column.
///
/// Each `graphics_id` is checked alone because
/// `engine::overworld::object_event_is_in_view` covers only ~17 metatiles
/// and Route 103's trainers span a 70-tile route. The player stands one
/// metatile north of the NPC, so the entry sits one metatile south of
/// [`super::avatar::PLAYER_OBJ_Y`].
///
/// The entry is found by frame block rather than by count: binding blocks
/// are disjoint, and other object events share the window (Route 103's
/// berry and cuttable trees are unbound, and its twins are declared twice).
fn assert_background_npc_draws_an_oam_entry(
    scene: &super::OverworldScene,
    map: assets::MapId,
    data: &engine::event_data::EventData,
    graphics_id: &str,
) {
    // `load_room` resolves events through `resolve_map_events`, so the
    // expected position must too: Oldale Town's footprints man and mart
    // employee stand at their post-`OldaleTown_OnTransition` tiles, not the
    // map.json ones. Other maps, Route 103 included, are unchanged.
    let event = super::oldale_town_npc_reposition::resolve_map_events(map, data)
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
        "{graphics_id} must contribute exactly one OAM entry drawn from its \
         own binding's frame block; a bound but never-emitted NPC is invisible"
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

/// Oldale Town's three fresh-save-visible background NPCs bind and draw:
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
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        OLDALE_TOWN,
        super::PlayerCharacter::Brendan,
        &data,
    )
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

/// Route 103's background NPCs and trainers bind and draw. Each is
/// `"flag": "0"` on a fresh save (`data/maps/Route103/map.json`), unlike its
/// rival (`FLAG_HIDE_ROUTE_103_RIVAL`) and Professor Birch
/// (`FLAG_HIDE_ROUTE_103_BIRCH`). Palette banks follow each id's
/// `gObjectEventGraphicsInfo_*.paletteTag`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_route_103_binds_its_background_npcs() {
    const ROUTE_103: assets::MapId = assets::MapId("MAP_ROUTE103");
    let data = fresh_save_event_data();
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_103,
        super::PlayerCharacter::Brendan,
        &data,
    )
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

/// Loading Route 103 for a `PlayerCharacter::May` save uses May's sheet and
/// palette bank 0 for the player's `SceneSprites` tiles, while the map's
/// `OBJ_EVENT_GFX_VAR_0` object resolves independently to Brendan as the
/// rival.
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

    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_103,
        super::PlayerCharacter::May,
        &event_data,
    )
    .expect("run `cargo xtask extract` first");
    // Reference sheets come from the same pack the scene read.
    let pack = assets::pack::AssetPack::load_repo().unwrap();

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

// -- Real-pack elevation-driven OBJ priority --------------------------------

/// The player OBJ's priority follows elevation on the real bedroom map.
/// Standing on the bed's raised elevation-4 edge tile draws the player at the
/// raised OAM priority ([`super::avatar::priority_for_elevation`]), not the
/// default priority of ordinary floor. Real-pack so the priority is read from
/// the production [`super::sprites::SceneSprites::entries`] list built from the
/// bedroom's own layout, not just [`super::avatar::player_entry`] on synthetic
/// elevations.
///
/// The collision-side counterpart for the same tiles is
/// `flow::overworld_phase::bedroom_collision_tests::bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_bed_side_edge_draws_the_player_obj_at_the_raised_priority() {
    const BEDROOM: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let data = fresh_save_event_data();
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        BEDROOM,
        super::PlayerCharacter::Brendan,
        &data,
    )
    .expect("run `cargo xtask extract` first");

    // (0, 7): ordinary floor south of the bed, elevation 3.
    let on_the_floor = PlayerState::new((0, 7), 3, Direction::North);
    let floor_entries = scene.sprites.entries(&on_the_floor, &data);
    assert_eq!(
        floor_entries[0].priority(),
        super::avatar::PLAYER_OBJ_PRIORITY,
        "entry 0 is the player; ordinary floor (elevation 3) draws at the default priority"
    );

    // (0, 5): the bed's raised west-edge tile, elevation 4. This is authored
    // data from `pokeemerald/data/layouts/LittlerootTown_BrendansHouse_2F/map.bin`,
    // the same tile the collision-side test above checks.
    let on_the_bed_edge = PlayerState::new((0, 5), 4, Direction::North);
    let bed_entries = scene.sprites.entries(&on_the_bed_edge, &data);
    assert_eq!(
        bed_entries[0].priority(),
        1,
        "the bed's raised elevation-4 edge must draw the player OBJ at priority 1 \
         (sElevationToPriority[4]); the compositor's same-priority tie rule \
         (`rendering::compositor`) then puts the sprite in front of the top BG layer"
    );

    // (0, 6): the bed's south-edge ELEVATION_TRANSITION tile, stepped onto
    // from the raised edge. `PlayerState::new` seeds both elevation fields
    // identically, so the drifted stance (elevation 0 on the wildcard tile
    // while `previous_elevation()` still reads 4) is reachable only through
    // `PlayerState::step`, as in the collision-side walk test. The south edge
    // is used because the fresh event data leaves a decoration placeholder
    // object on the north-edge tile.
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
        "precondition: the wildcard tile drifts the two elevation fields apart"
    );
    let drifted_entries = scene.sprites.entries(&walker, &data);
    assert_eq!(
        drifted_entries[0].priority(),
        1,
        "the player OBJ priority must follow previous_elevation (4), not the \
         wildcard tile's own 0 (ObjectEventUpdateElevation keeps the last \
         concrete elevation), so the sprite stays at the raised priority while \
         it stands on the transition tile"
    );
}

// -- Real-pack tileset tile animation ---------------------------------------

/// The General primary tileset's animated tile ranges from upstream
/// `tileset_anims.c`. They match `GENERAL_REGIONS` in
/// `crate::overworld::tileset_anims` but are transcribed here instead of
/// imported, so the tests do not compare that module with itself.
const GENERAL_ANIMATED_TILE_RANGES: [std::ops::Range<u16>; 5] = [
    508..512, // flower (4 tiles)
    432..462, // water (30 tiles)
    464..474, // sand/water edge (10 tiles)
    496..502, // waterfall (6 tiles)
    480..490, // land/water edge (10 tiles)
];

/// A standing (not in transit) player position in `MAP_LITTLEROOT_TOWN` whose
/// viewport draws animated tiles. Littleroot is on the animated "general"
/// primary tileset; the bundled interiors use "building", whose one animated
/// metatile (the TV) no interior `map.bin` places.
///
/// Flower is the only animated region Littleroot puts on screen. Measured on
/// the real pack, 24 on-screen BG cells at this position reference the flower
/// range and none reference water, sand/water edge, waterfall, or land/water
/// edge. Over every standing position in Littleroot, flower peaks at 36 cells
/// (at `(0, 14)`) and the other four regions stay at 0. So the other four
/// ranges of [`GENERAL_ANIMATED_TILE_RANGES`] are pinned by
/// `tileset_anims`' unit tests, not by a rendered pixel here. This position is a
/// Littleroot view where the flower region is on screen.
const LITTLEROOT_TOWN_FLOWER_VIEW: (i32, i32) = (10, 17);

/// The screen-pixel rectangles that [`GENERAL_ANIMATED_TILE_RANGES`] paints for
/// `scene` and `player`'s viewport: the `(x0, y0, x1, y1)` rect of every 8x8 BG
/// tile cell whose bottom, middle, or top entry names a tile index in one of
/// those ranges. It rebuilds the tilemaps [`super::OverworldScene::compose`]
/// uses from [`super::OverworldScene`]'s private fields (this module is a
/// descendant), as [`super::viewport`]'s own tests do.
///
/// A resting viewport carries `RESTING_SCROLL_Y`'s 8px vertical scroll, so a
/// tile's raw pixel row is shifted by `viewport.scroll_y` (horizontal scroll is
/// zero at rest). Rects pushed fully off-screen are dropped, since no test
/// samples outside `0..160`.
///
/// # Panics
///
/// If `player` [`PlayerState::in_transit`]: the padded, scrolled viewport that
/// produces does not map tile `(col, row)` to pixel `(col * 8, row * 8 -
/// scroll_y)` in general.
fn animated_tile_screen_rects(
    scene: &super::OverworldScene,
    player: &PlayerState,
) -> Vec<(usize, usize, usize, usize)> {
    assert!(
        !player.in_transit(),
        "helper assumes an unpadded viewport with only the resting scroll"
    );
    let grid = scene
        .layout
        .grid(&scene.grid_bytes)
        .expect("grid_bytes validated in from_pack");
    let border =
        assets::BorderGrid::new(&scene.border_bytes).expect("border_bytes validated in from_pack");
    let primary_attrs = assets::MetatileAttributeTable::new(&scene.primary_attrs_bytes);
    let secondary_attrs = assets::MetatileAttributeTable::new(&scene.secondary_attrs_bytes);
    // Rebuild the per-connection grids `OverworldScene::frame_viewport` builds,
    // so this helper matches it even though Littleroot's North connection
    // never resolves at the flower fixture position.
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
            if !animated {
                continue;
            }
            let y0 = i32::try_from(row * 8).unwrap() - i32::from(viewport.scroll_y);
            let y1 = y0 + 8;
            if y1 <= 0 || y0 >= 160 {
                continue;
            }
            let x0 = col * 8;
            rects.push((
                x0,
                usize::try_from(y0.max(0)).unwrap(),
                x0 + 8,
                usize::try_from(y1.min(160)).unwrap(),
            ));
        }
    }
    rects
}

/// Composing `MAP_LITTLEROOT_TOWN` at two ticks changes pixels only inside the
/// on-screen rects [`animated_tile_screen_rects`] returns. So
/// [`super::OverworldScene::compose`]'s `tick` reaches rendered pixels, and
/// only the pixels the animated tile ranges own. Tick 60 is chosen because
/// General flower, the one region this map puts on screen
/// ([`LITTLEROOT_TOWN_FLOWER_VIEW`]), fires at ticks 16, 32, 48, and 64, so at
/// tick 60 it has latched sequence position 3 (`Frame2`), a frame distinct from
/// the base art at tick 0 (see `tileset_anims::latched_frame`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_tick_changes_only_the_animated_tile_screen_regions() {
    let event_data = engine::event_data::EventData::new();
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
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
        "at least one on-screen tile must reference one of GENERAL_ANIMATED_TILE_RANGES"
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

/// Two ticks a full cadence period apart compose pixel-identically. 128 is the
/// least common multiple of the cycles of every region
/// [`crate::overworld::tileset_anims`] ports: General water and sand/water edge
/// run at 128, and every other region's cycle divides it.
///
/// The invariant holds for every tick >= 16, not for any tick. Before a
/// region's first fire it shows the base art, which in general differs from the
/// frame it shows 128 ticks later: on a map with water on screen, `compose(0)`
/// and `compose(128)` would differ, because water latches sequence position 0
/// at tick 128 and `water/0` is not the base tile art. 16 is the largest first
/// fire of any ported region (the `phase == 0` regions' `interval`), so from
/// tick 16 every region has latched and the period is exact.
///
/// Ticks 0, 1, and 8 are exercised only because this map and position make
/// them hold: General flower is the sole animated region on screen
/// ([`LITTLEROOT_TOWN_FLOWER_VIEW`]) and its `sequence[0]` frame is
/// pixel-identical to the base art, so before-first-fire and latched frame 0
/// render the same. Only the ticks from 16 up are evidence for the general
/// invariant.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_tileset_animation_repeats_after_a_full_cadence_period() {
    const FULL_CADENCE_PERIOD: u32 = 128;

    let event_data = engine::event_data::EventData::new();
    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        super::PlayerCharacter::Brendan,
        &event_data,
    )
    .expect("run `cargo xtask extract` first");
    let (x, y) = LITTLEROOT_TOWN_FLOWER_VIEW;
    let player = PlayerState::new((x, y), 3, Direction::South);

    // 0/1/8 hold for this map and position only (see the doc comment).
    // 16/60/127 cover the map-independent invariant: every region has fired
    // by tick 16.
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

// -- Real-pack connected-map rendering --------------------------------------

/// A camera near Littleroot's north edge renders Route 101's tiles with no
/// seam. `MAP_LITTLEROOT_TOWN`'s only declared connection is `Direction::North`
/// with `offset: 0` into `MAP_ROUTE101` (`crates/assets/src/map_headers.rs`).
/// The strip north of the edge must show the tiles Route 101's own layout
/// renders at the corresponding world position, not a hard cut to the border
/// block.
///
/// It is a pixel comparison, not a golden image, so it survives art changes.
/// The same world strip is composed two ways: through Littleroot's north-edge
/// connection, and by loading Route 101 directly with the camera on the
/// corresponding position of its side (offset 0 lines the x axes up 1:1; the
/// constants below derive the y alignment from the camera-centering math in
/// [`super::viewport`]'s module docs). The two compositions must agree pixel
/// for pixel everywhere in the shared strip.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_littleroot_and_route_101_render_continuously_across_their_shared_edge() {
    const LITTLEROOT: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN");
    const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        LITTLEROOT,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    // The same room with its declared connections stripped, so composing it
    // renders a hard cut to the border block. The private `connections` field
    // is reachable because this module is a descendant of `overworld`.
    let mut scene_border_only = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        LITTLEROOT,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    scene_border_only.connections.clear();
    let route_101 = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_101,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let data = fresh_save_event_data();
    // Standing at row 1 of Littleroot's 20x20 grid
    // (`crates/assets/src/map_layouts.rs`), facing north. The resting viewport
    // anchor is `anchor_y == y - PLAYER_VIEW_ROW == 1 - 5 == -4`
    // (`super::viewport`'s module docs), so it samples world rows `-4..=6` (the last is the resting-scroll row).
    // Screen rows 0..=3 (world rows `-4..=-1`) lie north of Littleroot's
    // y == 0 edge, in the connection's territory, and all are inside
    // `viewport::within_backup_map_band`'s `y >= -7` cover. `RESTING_SCROLL_Y`'s
    // 8px scroll shifts them to screen pixel rows `0..56`, not `0..64`:
    // `(56 + 8) / 16 == 4` is the first screen row that samples world row 0,
    // Littleroot's own interior.
    let player = PlayerState::new((10, 1), 3, Direction::North);
    let frame = scene.compose(&player, &data, 0);
    let frame_border_only = scene_border_only.compose(&player, &data, 0);

    // Route 101's camera is placed so the same screen pixel samples the same
    // world position as `connected_cell_at`'s North-connection formula
    // (`target_y == connected_height + y`; offset 0 leaves x untouched): same
    // x (10), and a y whose `anchor_y` puts Route 101's row 19 (its south edge)
    // on Littleroot's row -1, screen row 3 in both compositions. That is
    // `21 - 5 == 16` and `16 + 3 == 19`, against Littleroot's `-4 + 3 == -1`.
    let route_101_player = PlayerState::new((10, 21), 3, Direction::North);
    let frame_route_101 = route_101.compose(&route_101_player, &data, 0);

    // The shared strip is screen pixel rows `0..56`: 4 metatile rows at full
    // width, all north of Littleroot's grid per the anchor math above. Its last
    // row (pixel rows 40..56, Littleroot world row -1, Route 101 row 19) is
    // adjacent to the seam, so a seam error shows there first.
    let mut any_pixel_changed_by_the_connection = false;
    for py in 0..56usize {
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
        "the connection must change at least one pixel versus the border-only \
         fallback, or the comparisons above would be satisfied by an all-border strip"
    );
}

/// The same seam check for Route 101's only North connection, `MAP_OLDALE_TOWN`
/// with `offset: 0` (`crates/assets/src/map_headers.rs`), which resolves
/// because the `LAYOUTS` table bundles Oldale Town's grid bytes. The geometry
/// is identical to
/// [`real_pack_littleroot_and_route_101_render_continuously_across_their_shared_edge`]
/// (both layouts are 20x20, offset 0), so the same anchor math covers the same
/// four-row strip; see that test's comments for the derivation.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_route_101_and_oldale_town_render_continuously_across_their_shared_edge() {
    const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");
    const OLDALE: assets::MapId = assets::MapId("MAP_OLDALE_TOWN");

    let scene = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_101,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let mut scene_border_only = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_101,
        super::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    scene_border_only.connections.clear();
    let oldale = super::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
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
    for py in 0..56usize {
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
        "the connection must change at least one pixel versus the border-only \
         fallback, or the comparisons above would be satisfied by an all-border strip"
    );
}

// -- Default-CI tileset animation through compose ----------------------------

/// [`synthetic_scene`]'s fixture pack with its metatile drawing the General
/// flower tile (508) and flower frames 0/1/2 painted in distinct opaque
/// colors, so a composed pixel names the frame patched into that tile.
fn synthetic_animated_flower_pack_bytes() -> Vec<u8> {
    const FLOWER_START_TILE: u16 = 508;
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);

    let metatiles = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/metatiles")
        .expect("the general fixture always fabricates its own metatiles entry");
    metatiles.payload = std::iter::repeat_n(FLOWER_START_TILE.to_le_bytes(), 8)
        .flatten()
        .collect();

    let bank0 = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/00")
        .expect("the general fixture always fabricates its own bank-0 palette");
    bank0.meta = 8u16.to_le_bytes().to_vec();
    bank0.payload.extend_from_slice(&0x03E0u16.to_le_bytes()); // index 6: green.
    bank0.payload.extend_from_slice(&0x7C00u16.to_le_bytes()); // index 7: blue.

    for (asset, palette_index) in [(0u8, 5u8), (1, 6), (2, 7)] {
        let id = format!("tileset/general/anim/flower/{asset}");
        let frame = entries
            .iter_mut()
            .find(|e| e.id == id)
            .expect("push_general_anim_frames fabricates flower assets 0..3");
        frame.payload = vec![palette_index; 8 * 8 * 4];
    }

    write_synthetic_pack(entries)
}

/// Tick-to-pixel coverage that needs no local pack: flower frames latch as
/// `[0, 1, 0, 2]` every 16 ticks (`super::tileset_anims::GENERAL_REGIONS`).
#[test]
fn compose_paints_each_latched_flower_frame_into_the_animated_tile_range() {
    let color = |raw: u16| rendering::Bgr555::from_raw(raw).to_rgb888();
    let scene = synthetic_scene_result(
        synthetic_animated_flower_pack_bytes(),
        "gTileset_General",
        4,
        4,
    )
    .expect("the animated-flower synthetic pack should decode cleanly");

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let sample = |tick: u32| {
        scene
            .compose(&player, &event_data, tick)
            .pixel(4, 4)
            .expect("(4, 4) is on screen and away from the player sprite")
    };

    let at_first_fire = sample(16);
    assert_ne!(
        sample(0),
        at_first_fire,
        "tick must reach composed pixels: tick 0 shows base art, tick 16 the first \
         latched flower frame"
    );
    assert_eq!(
        at_first_fire,
        color(0x03E0),
        "tick 16 latches sequence position 1 -- flower asset 1 (green)"
    );
    assert_eq!(
        sample(32),
        color(0x001F),
        "tick 32 latches sequence position 2 -- flower asset 0 (red)"
    );
    assert_eq!(
        sample(48),
        color(0x7C00),
        "tick 48 latches sequence position 3 -- flower asset 2 (blue)"
    );
}
