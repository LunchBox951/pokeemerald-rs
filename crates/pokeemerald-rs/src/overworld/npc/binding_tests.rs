use super::test_support::{extracted_map_graphics_ids, NON_RIVAL_GFX_ID};
use super::*;

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

#[test]
fn resolve_sprite_source_partitions_every_extracted_map_graphics_id() {
    const GRAPHICS_ID_COUNT: usize = 46;

    let reachable = extracted_map_graphics_ids();
    assert_eq!(
        reachable.len(),
        GRAPHICS_ID_COUNT,
        "the extracted map set changed"
    );

    let no_flags = EventData::new();
    let (drawn, not_drawn): (Vec<&'static str>, Vec<&'static str>) = reachable
        .iter()
        .partition(|id| resolve_sprite_source(id, PlayerCharacter::Brendan, &no_flags).is_some());

    assert_eq!(
        drawn,
        [
            "OBJ_EVENT_GFX_BLACK_BELT",
            "OBJ_EVENT_GFX_BOY_1",
            "OBJ_EVENT_GFX_BOY_2",
            "OBJ_EVENT_GFX_FAT_MAN",
            "OBJ_EVENT_GFX_FISHERMAN",
            "OBJ_EVENT_GFX_GIRL_3",
            "OBJ_EVENT_GFX_MANIAC",
            "OBJ_EVENT_GFX_MAN_3",
            "OBJ_EVENT_GFX_MAN_5",
            "OBJ_EVENT_GFX_MART_EMPLOYEE",
            "OBJ_EVENT_GFX_MOM",
            "OBJ_EVENT_GFX_NORMAN",
            "OBJ_EVENT_GFX_POKEFAN_M",
            "OBJ_EVENT_GFX_PROF_BIRCH",
            "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL",
            "OBJ_EVENT_GFX_RIVAL_MAY_NORMAL",
            "OBJ_EVENT_GFX_SCIENTIST_1",
            "OBJ_EVENT_GFX_SWIMMER_F",
            "OBJ_EVENT_GFX_SWIMMER_M",
            "OBJ_EVENT_GFX_TWIN",
            "OBJ_EVENT_GFX_WOMAN_2",
            "OBJ_EVENT_GFX_WOMAN_4",
        ],
        "the supported graphics-id set changed"
    );
    assert_eq!(
        not_drawn,
        [
            "OBJ_EVENT_GFX_BERRY_TREE",
            "OBJ_EVENT_GFX_BIRCHS_BAG",
            "OBJ_EVENT_GFX_CUTTABLE_TREE",
            "OBJ_EVENT_GFX_ITEM_BALL",
            "OBJ_EVENT_GFX_NINJA_BOY",
            "OBJ_EVENT_GFX_PICHU_DOLL",
            "OBJ_EVENT_GFX_SWABLU_DOLL",
            "OBJ_EVENT_GFX_TRUCK",
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
            "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
            "OBJ_EVENT_GFX_VIGOROTH_FACING_AWAY",
            "OBJ_EVENT_GFX_YOUNGSTER",
            "OBJ_EVENT_GFX_ZIGZAGOON_1",
        ],
        "the unsupported graphics-id set changed"
    );

    let drawn_as_may: Vec<_> = reachable
        .iter()
        .filter(|id| resolve_sprite_source(id, PlayerCharacter::May, &no_flags).is_some())
        .copied()
        .collect();
    assert_eq!(
        drawn_as_may, drawn,
        "graphics-id support must not depend on the player character"
    );
}

#[test]
fn resolve_sprite_source_returns_none_for_decorations_and_props() {
    let no_flags = EventData::new();
    assert!(
        resolve_sprite_source("OBJ_EVENT_GFX_VAR_0", PlayerCharacter::Brendan, &no_flags).is_none()
    );
    assert!(
        resolve_sprite_source("OBJ_EVENT_GFX_TRUCK", PlayerCharacter::Brendan, &no_flags).is_none()
    );
    assert!(resolve_sprite_source(
        "OBJ_EVENT_GFX_ITEM_BALL",
        PlayerCharacter::Brendan,
        &no_flags
    )
    .is_none());
    assert!(resolve_sprite_source(
        "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
        PlayerCharacter::Brendan,
        &no_flags
    )
    .is_none());
}

#[test]
fn npc_palette_tags_use_distinct_banks_starting_at_one() {
    assert_eq!(NpcPaletteTag::Npc1.bank(), 1);
    assert_eq!(NpcPaletteTag::Npc2.bank(), 2);
    assert_eq!(NpcPaletteTag::Npc3.bank(), 3);
    assert_eq!(NpcPaletteTag::Npc4.bank(), 4);
}
