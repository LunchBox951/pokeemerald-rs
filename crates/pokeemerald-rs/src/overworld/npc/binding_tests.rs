use super::test_support::{bundled_map_graphics_ids, NON_RIVAL_GFX_ID};
use super::*;
use engine::overworld::Direction;

#[test]
fn both_rival_variants_resolve_for_either_player_gender() {
    let no_flags = EventData::new();
    assert_eq!(
        resolve_sprite_source(
            "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
            PlayerCharacter::Brendan,
            &no_flags
        ),
        Some(NpcSpriteSource::People16x32 {
            sprite_path: "may/walking",
            palette_bank: OTHER_PROTAGONIST_BANK,
        }),
        "the rival of a Brendan player is May, drawn from May's sheet"
    );
    assert_eq!(
        resolve_sprite_source(
            "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
            PlayerCharacter::May,
            &no_flags
        ),
        Some(NpcSpriteSource::People16x32 {
            sprite_path: "brendan/walking",
            palette_bank: OTHER_PROTAGONIST_BANK,
        }),
        "the rival of a May player is Brendan, drawn from Brendan's sheet"
    );
    assert_eq!(
        resolve_sprite_source(
            "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
            PlayerCharacter::Brendan,
            &no_flags
        ),
        Some(NpcSpriteSource::PlayerCharacter)
    );
    assert_eq!(
        resolve_sprite_source(
            "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
            PlayerCharacter::May,
            &no_flags
        ),
        Some(NpcSpriteSource::PlayerCharacter)
    );
}

#[test]
fn var_0_resolves_to_a_rival_only_when_the_var_holds_a_real_rival_gfx_id() {
    let mut event_data = EventData::new();

    assert_eq!(
        resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &event_data),
        None
    );

    event_data
        .var_set(VAR_OBJ_GFX_ID_0, RIVAL_MAY_NORMAL_GFX_ID)
        .unwrap();
    assert_eq!(
        resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &event_data),
        Some(NpcSpriteSource::People16x32 {
            sprite_path: "may/walking",
            palette_bank: OTHER_PROTAGONIST_BANK,
        }),
        "a male player's Route 103 rival is May"
    );

    event_data
        .var_set(VAR_OBJ_GFX_ID_0, RIVAL_BRENDAN_NORMAL_GFX_ID)
        .unwrap();
    assert_eq!(
        resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::May, &event_data),
        Some(NpcSpriteSource::People16x32 {
            sprite_path: "brendan/walking",
            palette_bank: OTHER_PROTAGONIST_BANK,
        }),
        "a female player's Route 103 rival is Brendan"
    );

    event_data
        .var_set(VAR_OBJ_GFX_ID_0, NON_RIVAL_GFX_ID)
        .unwrap();
    assert_eq!(
        resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &event_data),
        None
    );
}

#[test]
fn the_other_protagonist_palette_bank_collides_with_nothing() {
    assert_ne!(OTHER_PROTAGONIST_BANK, PLAYER_PALETTE_BANK);
    for tag in [
        NpcPaletteTag::Npc1,
        NpcPaletteTag::Npc2,
        NpcPaletteTag::Npc3,
        NpcPaletteTag::Npc4,
    ] {
        assert_ne!(OTHER_PROTAGONIST_BANK, tag.bank(), "{tag:?} bank clash");
    }
}

#[test]
fn resolve_sprite_source_resolves_mom_to_the_npc4_palette() {
    let source = resolve_sprite_source(
        "OBJ_EVENT_GFX_MOM",
        PlayerCharacter::Brendan,
        &EventData::new(),
    )
    .unwrap();
    assert_eq!(
        source,
        NpcSpriteSource::People16x32 {
            sprite_path: "mom",
            palette_bank: NpcPaletteTag::Npc4.bank(),
        }
    );
}

#[test]
fn resolve_sprite_source_resolves_the_oldale_and_route_103_background_npcs() {
    use NpcPaletteTag::{Npc1, Npc2, Npc3, Npc4};

    let no_flags = EventData::new();
    let cases = [
        ("OBJ_EVENT_GFX_MART_EMPLOYEE", "mart_employee", Npc1),
        ("OBJ_EVENT_GFX_GIRL_3", "girl_3", Npc2),
        ("OBJ_EVENT_GFX_MANIAC", "maniac", Npc4),
        ("OBJ_EVENT_GFX_MAN_3", "man_3", Npc2),
        ("OBJ_EVENT_GFX_WOMAN_2", "woman_2", Npc3),
        ("OBJ_EVENT_GFX_BOY_1", "boy_1", Npc3),
        ("OBJ_EVENT_GFX_POKEFAN_M", "pokefan_m", Npc2),
        ("OBJ_EVENT_GFX_BLACK_BELT", "black_belt", Npc3),
        ("OBJ_EVENT_GFX_MAN_5", "man_5", Npc2),
        ("OBJ_EVENT_GFX_SWIMMER_F", "swimmer_f", Npc2),
        ("OBJ_EVENT_GFX_SWIMMER_M", "swimmer_m", Npc1),
        ("OBJ_EVENT_GFX_FISHERMAN", "fisherman", Npc2),
    ];
    for (id, sprite_path, tag) in cases {
        assert_eq!(
            resolve_sprite_source(id, PlayerCharacter::Brendan, &no_flags),
            Some(NpcSpriteSource::People16x32 {
                sprite_path,
                palette_bank: tag.bank(),
            }),
            "{id}"
        );
    }
}

/// The bundled maps' graphics ids `resolve_sprite_source` draws.
const DRAWN_GRAPHICS_IDS: [&str; 41] = [
    "OBJ_EVENT_GFX_BIRCHS_BAG",
    "OBJ_EVENT_GFX_BLACK_BELT",
    "OBJ_EVENT_GFX_BOY_1",
    "OBJ_EVENT_GFX_BOY_2",
    "OBJ_EVENT_GFX_CUTTABLE_TREE",
    "OBJ_EVENT_GFX_FAT_MAN",
    "OBJ_EVENT_GFX_FISHERMAN",
    "OBJ_EVENT_GFX_GENTLEMAN",
    "OBJ_EVENT_GFX_GIRL_3",
    "OBJ_EVENT_GFX_ITEM_BALL",
    "OBJ_EVENT_GFX_MANIAC",
    "OBJ_EVENT_GFX_MAN_3",
    "OBJ_EVENT_GFX_MAN_5",
    "OBJ_EVENT_GFX_MART_EMPLOYEE",
    "OBJ_EVENT_GFX_MOM",
    "OBJ_EVENT_GFX_MOVING_BOX",
    "OBJ_EVENT_GFX_MYSTERY_GIFT_MAN",
    "OBJ_EVENT_GFX_NINJA_BOY",
    "OBJ_EVENT_GFX_NORMAN",
    "OBJ_EVENT_GFX_NURSE",
    "OBJ_EVENT_GFX_PICHU_DOLL",
    "OBJ_EVENT_GFX_POKEFAN_F",
    "OBJ_EVENT_GFX_POKEFAN_M",
    "OBJ_EVENT_GFX_PROF_BIRCH",
    "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
    "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
    "OBJ_EVENT_GFX_SCHOOL_KID_M",
    "OBJ_EVENT_GFX_SCIENTIST_1",
    "OBJ_EVENT_GFX_SWABLU_DOLL",
    "OBJ_EVENT_GFX_SWIMMER_F",
    "OBJ_EVENT_GFX_SWIMMER_M",
    "OBJ_EVENT_GFX_TEALA",
    "OBJ_EVENT_GFX_TRUCK",
    "OBJ_EVENT_GFX_TWIN",
    "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
    "OBJ_EVENT_GFX_VIGOROTH_FACING_AWAY",
    "OBJ_EVENT_GFX_WOMAN_2",
    "OBJ_EVENT_GFX_WOMAN_4",
    "OBJ_EVENT_GFX_WOMAN_5",
    "OBJ_EVENT_GFX_YOUNGSTER",
    "OBJ_EVENT_GFX_ZIGZAGOON_1",
];

/// The bundled maps' graphics ids `resolve_sprite_source` leaves undrawn.
const UNDRAWN_GRAPHICS_IDS: [&str; 35] = [
    "OBJ_EVENT_GFX_BERRY_TREE",
    "OBJ_EVENT_GFX_BOY_3",
    "OBJ_EVENT_GFX_BUG_CATCHER",
    "OBJ_EVENT_GFX_EXPERT_F",
    "OBJ_EVENT_GFX_EXPERT_M",
    "OBJ_EVENT_GFX_GAMEBOY_KID",
    "OBJ_EVENT_GFX_GIRL_1",
    "OBJ_EVENT_GFX_GIRL_2",
    "OBJ_EVENT_GFX_KECLEON",
    "OBJ_EVENT_GFX_LASS",
    "OBJ_EVENT_GFX_LITTLE_BOY",
    "OBJ_EVENT_GFX_MAN_1",
    "OBJ_EVENT_GFX_MAN_4",
    "OBJ_EVENT_GFX_OLD_MAN",
    "OBJ_EVENT_GFX_OLD_WOMAN",
    "OBJ_EVENT_GFX_PICNICKER",
    "OBJ_EVENT_GFX_PIKACHU",
    "OBJ_EVENT_GFX_SAILOR",
    "OBJ_EVENT_GFX_SCOTT",
    "OBJ_EVENT_GFX_SKITTY",
    "OBJ_EVENT_GFX_VAR_0",
    "OBJ_EVENT_GFX_VAR_1",
    "OBJ_EVENT_GFX_VAR_2",
    "OBJ_EVENT_GFX_VAR_3",
    "OBJ_EVENT_GFX_VAR_4",
    "OBJ_EVENT_GFX_VAR_5",
    "OBJ_EVENT_GFX_VAR_6",
    "OBJ_EVENT_GFX_VAR_7",
    "OBJ_EVENT_GFX_VAR_8",
    "OBJ_EVENT_GFX_VAR_9",
    "OBJ_EVENT_GFX_VAR_A",
    "OBJ_EVENT_GFX_VAR_B",
    "OBJ_EVENT_GFX_WINGULL",
    "OBJ_EVENT_GFX_WOMAN_1",
    "OBJ_EVENT_GFX_WOMAN_3",
];

#[test]
fn resolve_sprite_source_partitions_every_bundled_map_graphics_id() {
    const GRAPHICS_ID_COUNT: usize = 76;

    let reachable = bundled_map_graphics_ids();
    assert_eq!(
        reachable.len(),
        GRAPHICS_ID_COUNT,
        "the bundled map set changed"
    );

    let no_flags = EventData::new();
    let (drawn, not_drawn): (Vec<&'static str>, Vec<&'static str>) = reachable
        .iter()
        .partition(|id| resolve_sprite_source(id, PlayerCharacter::Brendan, &no_flags).is_some());

    assert_eq!(
        drawn, DRAWN_GRAPHICS_IDS,
        "the supported graphics-id set changed"
    );
    assert_eq!(
        not_drawn, UNDRAWN_GRAPHICS_IDS,
        "the unsupported graphics-id set changed"
    );

    let drawn_as_may: Vec<_> = reachable
        .iter()
        .filter(|id| resolve_sprite_source(id, PlayerCharacter::May, &no_flags).is_some())
        .copied()
        .collect();
    assert_eq!(
        drawn_as_may, DRAWN_GRAPHICS_IDS,
        "graphics-id support must not depend on the player character"
    );
}

#[test]
fn resolve_sprite_source_returns_none_for_decorations_and_per_event_berries() {
    let no_flags = EventData::new();
    for id in [
        "OBJ_EVENT_GFX_VAR_0",
        "OBJ_EVENT_GFX_VAR_1",
        "OBJ_EVENT_GFX_BERRY_TREE",
    ] {
        assert!(
            resolve_sprite_source(id, PlayerCharacter::Brendan, &no_flags).is_none(),
            "{id}"
        );
    }
}

/// Geometry, sheet, palette and pose of each non-people-sheet graphic, from
/// `object_event_graphics_info.h` and the pic and anim tables.
#[test]
fn object_sheets_carry_upstream_geometry_palette_and_pose() {
    use super::sheets::{object_sheet, Facing};
    // (id, w, h, cells, bank, facing cell for South, North, East flip)
    let rows = [
        ("OBJ_EVENT_GFX_TRUCK", 48, 48, 1, 8, 0, 0, false),
        ("OBJ_EVENT_GFX_MOVING_BOX", 16, 16, 1, 7, 0, 0, false),
        ("OBJ_EVENT_GFX_ITEM_BALL", 16, 16, 1, 3, 0, 0, false),
        ("OBJ_EVENT_GFX_PICHU_DOLL", 16, 16, 1, 2, 0, 0, false),
        ("OBJ_EVENT_GFX_SWABLU_DOLL", 16, 16, 1, 1, 0, 0, false),
        ("OBJ_EVENT_GFX_BIRCHS_BAG", 16, 16, 1, 2, 0, 0, false),
        ("OBJ_EVENT_GFX_CUTTABLE_TREE", 16, 16, 4, 3, 0, 0, false),
        ("OBJ_EVENT_GFX_NINJA_BOY", 16, 16, 9, 1, 0, 1, true),
        ("OBJ_EVENT_GFX_NURSE", 16, 32, 4, 1, 0, 1, true),
        ("OBJ_EVENT_GFX_MYSTERY_GIFT_MAN", 16, 32, 3, 1, 0, 1, true),
        ("OBJ_EVENT_GFX_ZIGZAGOON_1", 32, 32, 9, 10, 0, 1, true),
        (
            "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
            32,
            32,
            5,
            9,
            0,
            0,
            true,
        ),
        (
            "OBJ_EVENT_GFX_VIGOROTH_FACING_AWAY",
            32,
            32,
            5,
            9,
            3,
            3,
            true,
        ),
    ];
    for (id, width, height, cells, bank, south, north, east_flips) in rows {
        let sheet = object_sheet(id).unwrap_or_else(|| panic!("{id} has no row"));
        assert_eq!(
            (sheet.width, sheet.height, sheet.cells, sheet.palette_bank),
            (width, height, cells, bank),
            "{id}"
        );
        assert_eq!(
            sheet.facing.cell_for(Direction::South),
            (south, false),
            "{id}"
        );
        assert_eq!(
            sheet.facing.cell_for(Direction::North),
            (north, false),
            "{id}"
        );
        let (_, flipped) = sheet.facing.cell_for(Direction::East);
        assert_eq!(
            flipped, east_flips,
            "{id}: east reuses west flipped only for Standard"
        );
        assert!(sheet.cells > 0 && sheet.width.is_multiple_of(8) && sheet.height.is_multiple_of(8));
    }
    assert_eq!(
        Facing::PEOPLE.cell_for(Direction::West),
        (2, false),
        "west stand frame"
    );
}

#[test]
fn berry_trees_resolve_to_the_fresh_save_berries_stage_per_tree() {
    use super::sheets::initial_berry_sheet;
    for (tree, path, bank) in [
        ("BERRY_TREE_ROUTE_103_CHERI_1", "berry_trees/cheri", 3),
        ("BERRY_TREE_ROUTE_103_CHERI_2", "berry_trees/cheri", 3),
        ("BERRY_TREE_ROUTE_103_LEPPA", "berry_trees/leppa", 2),
    ] {
        let sheet = initial_berry_sheet(tree).unwrap();
        assert_eq!(
            (sheet.sprite_path, sheet.palette_bank),
            (path, bank),
            "{tree}"
        );
        assert_eq!((sheet.width, sheet.height), (16, 32));
        // sAnim_BerryTreeStage4 starts at pic 7 = berry-sheet cell 4.
        assert_eq!(sheet.facing.cell_for(Direction::South), (4, false));
    }
    assert!(initial_berry_sheet("BERRY_TREE_ROUTE_104_PECHA_1").is_none());
}

#[test]
fn npc_palette_tags_use_distinct_banks_starting_at_one() {
    assert_eq!(NpcPaletteTag::Npc1.bank(), 1);
    assert_eq!(NpcPaletteTag::Npc2.bank(), 2);
    assert_eq!(NpcPaletteTag::Npc3.bank(), 3);
    assert_eq!(NpcPaletteTag::Npc4.bank(), 4);
}
