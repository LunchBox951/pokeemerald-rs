//! The upstream call sites that justify each manifest song, as `(constant, [(file, line)])`.
//!
//! Every site is a line of the pinned checkout (`pokeemerald/`) that names the constant: a
//! `PlaySE`, `PlayBGM`, `PlayFanfare`, or `playse` call, a map's `"music"`, or an animation
//! script's `playsewithpan`. `MUS_NONE`, which `PlayBGM` turns into song 0
//! (`src/sound.c:563-569`), is audited as `MUS_DUMMY`. A constant may be called from more sites
//! than are listed; each entry lists the first line in each file the audit relied on.

/// Constants paired with the upstream lines that reach them.
pub(super) const AUDITED_CALL_SITES: [(&str, &[(&str, u32)]); 150] = [
    (
        "MUS_BIRCH_LAB",
        &[("data/maps/LittlerootTown_ProfessorBirchsLab/map.json", 5)],
    ),
    ("MUS_CAUGHT", &[("src/battle_message.c", 472)]),
    (
        "MUS_DUMMY",
        &[
            ("src/sound.c", 201),
            ("data/maps/LittlerootTown_BrendansHouse_1F/scripts.inc", 168),
        ],
    ),
    (
        "MUS_ENCOUNTER_BRENDAN",
        &[
            ("data/maps/Route103/scripts.inc", 47),
            ("data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc", 67),
        ],
    ),
    (
        "MUS_ENCOUNTER_INTERVIEWER",
        &[("data/scripts/players_house.inc", 151)],
    ),
    (
        "MUS_ENCOUNTER_MAY",
        &[
            ("data/maps/Route103/scripts.inc", 30),
            ("data/maps/LittlerootTown_MaysHouse_2F/scripts.inc", 66),
        ],
    ),
    ("MUS_EVOLUTION", &[("src/evolution_scene.c", 686)]),
    ("MUS_EVOLUTION_INTRO", &[("src/evolution_scene.c", 678)]),
    ("MUS_EVOLVED", &[("src/evolution_scene.c", 762)]),
    ("MUS_FOLLOW_ME", &[("data/maps/OldaleTown/scripts.inc", 42)]),
    ("MUS_HEAL", &[("src/field_effect.c", 1175)]),
    ("MUS_HELP", &[("data/maps/Route101/scripts.inc", 21)]),
    ("MUS_INTRO", &[("src/intro.c", 1216)]),
    ("MUS_INTRO_BATTLE", &[("src/intro.c", 1739)]),
    (
        "MUS_LEVEL_UP",
        &[
            ("data/battle_scripts_1.s", 3156),
            ("src/party_menu.c", 4793),
        ],
    ),
    (
        "MUS_LITTLEROOT",
        &[
            ("data/maps/LittlerootTown/map.json", 5),
            ("data/maps/LittlerootTown_BrendansHouse_1F/map.json", 5),
            ("data/maps/LittlerootTown_BrendansHouse_2F/map.json", 5),
            ("data/maps/LittlerootTown_MaysHouse_1F/map.json", 5),
            ("data/maps/LittlerootTown_MaysHouse_2F/map.json", 5),
        ],
    ),
    (
        "MUS_OBTAIN_ITEM",
        &[
            ("data/scripts/obtain_item.inc", 64),
            ("data/maps/LittlerootTown/scripts.inc", 897),
            (
                "data/maps/LittlerootTown_ProfessorBirchsLab/scripts.inc",
                112,
            ),
        ],
    ),
    (
        "MUS_OLDALE",
        &[
            ("data/maps/OldaleTown/map.json", 5),
            ("data/maps/OldaleTown_House1/map.json", 5),
            ("data/maps/OldaleTown_House2/map.json", 5),
        ],
    ),
    (
        "MUS_POKE_CENTER",
        &[
            ("data/maps/OldaleTown_PokemonCenter_1F/map.json", 5),
            ("data/maps/OldaleTown_PokemonCenter_2F/map.json", 5),
        ],
    ),
    (
        "MUS_POKE_MART",
        &[("data/maps/OldaleTown_Mart/map.json", 5)],
    ),
    ("MUS_RG_CAUGHT_INTRO", &[("src/battle_anim_throw.c", 1332)]),
    (
        "MUS_ROUTE101",
        &[
            ("data/maps/Route101/map.json", 5),
            ("data/maps/Route103/map.json", 5),
        ],
    ),
    ("MUS_ROUTE122", &[("src/main_menu.c", 1293)]),
    ("MUS_TITLE", &[("src/title_screen.c", 661)]),
    ("MUS_VICTORY_TRAINER", &[("src/battle_main.c", 4981)]),
    (
        "MUS_VICTORY_WILD",
        &[("src/battle_script_commands.c", 3363)],
    ),
    ("MUS_VS_RIVAL", &[("src/pokemon.c", 6467)]),
    ("MUS_VS_WILD", &[("src/pokemon.c", 6487)]),
    (
        "SE_BALL",
        &[
            ("src/field_effect.c", 1156),
            ("src/battle_anim_throw.c", 1119),
        ],
    ),
    (
        "SE_BALL_BOUNCE_1",
        &[("src/battle_anim_throw.c", 1050), ("src/strings.c", 407)],
    ),
    ("SE_BALL_BOUNCE_2", &[("src/battle_anim_throw.c", 1053)]),
    ("SE_BALL_BOUNCE_3", &[("src/battle_anim_throw.c", 1056)]),
    ("SE_BALL_BOUNCE_4", &[("src/battle_anim_throw.c", 1059)]),
    ("SE_BALL_OPEN", &[("src/battle_anim_throw.c", 1588)]),
    (
        "SE_BALL_THROW",
        &[
            ("data/battle_anim_scripts.s", 10728),
            ("src/battle_anim_throw.c", 824),
        ],
    ),
    ("SE_BALL_TRADE", &[("src/battle_anim_throw.c", 943)]),
    ("SE_BALL_TRAY_BALL", &[("src/battle_interface.c", 1872)]),
    ("SE_BALL_TRAY_ENTER", &[("src/battle_interface.c", 1666)]),
    ("SE_BALL_TRAY_EXIT", &[("src/battle_interface.c", 1870)]),
    ("SE_BOO", &[("src/start_menu.c", 1140)]),
    (
        "SE_CLICK",
        &[(
            "data/maps/LittlerootTown_ProfessorBirchsLab/scripts.inc",
            169,
        )],
    ),
    (
        "SE_DEX_PAGE",
        &[("src/pokedex.c", 2624), ("src/pokedex_area_screen.c", 677)],
    ),
    ("SE_DEX_SCROLL", &[("src/pokedex.c", 2605)]),
    ("SE_DEX_SEARCH", &[("src/pokedex.c", 5037)]),
    (
        "SE_DOOR",
        &[
            ("src/field_door.c", 551),
            ("data/scripts/players_house.inc", 444),
        ],
    ),
    ("SE_EFFECTIVE", &[("src/battle_script_commands.c", 2039)]),
    ("SE_ESCALATOR", &[("src/field_effect.c", 1603)]),
    (
        "SE_EXIT",
        &[
            ("src/field_screen_effect.c", 490),
            ("data/scripts/players_house.inc", 67),
        ],
    ),
    (
        "SE_EXP",
        &[
            ("src/battle_controller_player.c", 1215),
            ("src/evolution_scene.c", 736),
        ],
    ),
    ("SE_EXP_MAX", &[("data/battle_anim_scripts.s", 10705)]),
    (
        "SE_FAILURE",
        &[
            ("src/item_menu.c", 1253),
            ("src/party_menu.c", 1372),
            ("src/pokedex.c", 3399),
            ("src/pokemon_storage_system.c", 2442),
        ],
    ),
    (
        "SE_FAINT",
        &[
            ("src/battle_controller_player.c", 2422),
            ("src/battle_controller_opponent.c", 1421),
        ],
    ),
    (
        "SE_FLEE",
        &[("src/battle_message.c", 318), ("src/battle_util.c", 620)],
    ),
    ("SE_INTRO_BLAST", &[("src/intro.c", 2658)]),
    (
        "SE_LEDGE",
        &[
            ("src/field_player_avatar.c", 1034),
            ("data/maps/Route103/scripts.inc", 114),
        ],
    ),
    (
        "SE_LOW_HEALTH",
        &[
            ("src/battle_main.c", 1887),
            ("src/battle_gfx_sfx_util.c", 1100),
        ],
    ),
    (
        "SE_NOT_EFFECTIVE",
        &[("src/battle_script_commands.c", 2016)],
    ),
    (
        "SE_PC_LOGIN",
        &[
            ("data/scripts/pc.inc", 27),
            ("src/pokemon_storage_system.c", 2207),
            ("src/item_use.c", 788),
        ],
    ),
    (
        "SE_PC_OFF",
        &[
            ("data/scripts/pc.inc", 52),
            ("src/pokemon_storage_system.c", 3648),
        ],
    ),
    (
        "SE_PC_ON",
        &[
            ("data/scripts/pc.inc", 5),
            ("data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc", 246),
        ],
    ),
    (
        "SE_PIN",
        &[
            ("data/maps/LittlerootTown/scripts.inc", 207),
            ("data/scripts/pkmn_center_nurse.inc", 98),
        ],
    ),
    ("SE_REPEL", &[("src/item_use.c", 847)]),
    ("SE_RG_BALL_CLICK", &[("src/battle_anim_throw.c", 1319)]),
    ("SE_RG_CARD_FLIP", &[("src/trainer_card.c", 449)]),
    ("SE_RG_CARD_FLIPPING", &[("src/trainer_card.c", 1740)]),
    ("SE_RG_CARD_OPEN", &[("src/trainer_card.c", 430)]),
    ("SE_SAVE", &[("src/start_menu.c", 1116)]),
    (
        "SE_SELECT",
        &[
            ("src/main_menu.c", 890),
            ("src/naming_screen.c", 1515),
            ("src/text.c", 878),
            ("src/wallclock.c", 848),
        ],
    ),
    (
        "SE_SHINY",
        &[
            ("src/evolution_graphics.c", 381),
            ("src/battle_anim_throw.c", 2327),
            ("data/battle_anim_scripts.s", 1810),
        ],
    ),
    (
        "SE_SHOP",
        &[("src/shop.c", 1135), ("src/item_menu.c", 2168)],
    ),
    ("SE_SLIDING_DOOR", &[("src/field_door.c", 553)]),
    ("SE_SUCCESS", &[("src/pokedex.c", 5102)]),
    (
        "SE_SUPER_EFFECTIVE",
        &[("src/battle_script_commands.c", 2012)],
    ),
    ("SE_TRUCK_DOOR", &[("src/field_special_scene.c", 252)]),
    ("SE_TRUCK_MOVE", &[("src/field_special_scene.c", 203)]),
    ("SE_TRUCK_STOP", &[("src/field_special_scene.c", 223)]),
    ("SE_TRUCK_UNLOAD", &[("src/field_special_scene.c", 239)]),
    (
        "SE_USE_ITEM",
        &[("src/party_menu.c", 4432), ("data/battle_scripts_2.s", 120)],
    ),
    ("SE_WALL_HIT", &[("src/field_player_avatar.c", 1096)]),
    (
        "SE_WIN_OPEN",
        &[
            ("src/field_control_avatar.c", 184),
            ("src/naming_screen.c", 753),
        ],
    ),
    ("SE_M_ABSORB", &[("data/battle_anim_scripts.s", 5709)]),
    ("SE_M_ABSORB_2", &[("data/battle_anim_scripts.s", 10048)]),
    ("SE_M_ATTRACT", &[("data/battle_anim_scripts.s", 1998)]),
    ("SE_M_ATTRACT2", &[("data/battle_anim_scripts.s", 2006)]),
    ("SE_M_BELLY_DRUM", &[("data/battle_anim_scripts.s", 4592)]),
    ("SE_M_BITE", &[("data/battle_anim_scripts.s", 3026)]),
    ("SE_M_BUBBLE", &[("data/battle_anim_scripts.s", 2209)]),
    ("SE_M_BUBBLE2", &[("data/battle_anim_scripts.s", 4034)]),
    ("SE_M_BUBBLE3", &[("data/battle_anim_scripts.s", 5780)]),
    ("SE_M_BUBBLE_BEAM2", &[("src/evolution_graphics.c", 345)]),
    ("SE_M_CHARM", &[("data/battle_anim_scripts.s", 1993)]),
    ("SE_M_COMET_PUNCH", &[("data/battle_anim_scripts.s", 591)]),
    ("SE_M_CRABHAMMER", &[("data/battle_anim_scripts.s", 1551)]),
    ("SE_M_CUT", &[("data/battle_anim_scripts.s", 1205)]),
    ("SE_M_DETECT", &[("data/battle_anim_scripts.s", 1249)]),
    ("SE_M_DIG", &[("data/battle_anim_scripts.s", 2641)]),
    ("SE_M_DIZZY_PUNCH", &[("data/battle_anim_scripts.s", 2880)]),
    ("SE_M_DOUBLE_SLAP", &[("data/battle_anim_scripts.s", 430)]),
    ("SE_M_DOUBLE_TEAM", &[("data/battle_anim_scripts.s", 1184)]),
    ("SE_M_DRAGON_RAGE", &[("data/battle_anim_scripts.s", 1710)]),
    ("SE_M_EARTHQUAKE", &[("data/battle_anim_scripts.s", 2602)]),
    ("SE_M_EMBER", &[("data/battle_anim_scripts.s", 921)]),
    ("SE_M_FIRE_PUNCH", &[("data/battle_anim_scripts.s", 6977)]),
    ("SE_M_FLAMETHROWER", &[("data/battle_anim_scripts.s", 6319)]),
    ("SE_M_FLAME_WHEEL", &[("data/battle_anim_scripts.s", 665)]),
    ("SE_M_GIGA_DRAIN", &[("data/battle_anim_scripts.s", 5841)]),
    ("SE_M_GUST", &[("data/battle_anim_scripts.s", 2238)]),
    ("SE_M_GUST2", &[("data/battle_anim_scripts.s", 2242)]),
    ("SE_M_HARDEN", &[("data/battle_anim_scripts.s", 4580)]),
    ("SE_M_HEADBUTT", &[("data/battle_anim_scripts.s", 1528)]),
    ("SE_M_HORN_ATTACK", &[("data/battle_anim_scripts.s", 713)]),
    ("SE_M_HYDRO_PUMP", &[("data/battle_anim_scripts.s", 5620)]),
    ("SE_M_JUMP_KICK", &[("data/battle_anim_scripts.s", 708)]),
    ("SE_M_LEER", &[("data/battle_anim_scripts.s", 2089)]),
    (
        "SE_M_MEGA_KICK",
        &[
            ("src/evolution_graphics.c", 305),
            ("data/battle_anim_scripts.s", 946),
        ],
    ),
    ("SE_M_MEGA_KICK2", &[("data/battle_anim_scripts.s", 568)]),
    ("SE_M_METRONOME", &[("data/battle_anim_scripts.s", 2817)]),
    ("SE_M_MIST", &[("data/battle_anim_scripts.s", 5134)]),
    ("SE_M_MOONLIGHT", &[("data/battle_anim_scripts.s", 3374)]),
    (
        "SE_M_MORNING_SUN",
        &[
            ("data/battle_anim_scripts.s", 3904),
            ("src/battle_anim_effects_3.c", 2413),
        ],
    ),
    ("SE_M_PETAL_DANCE", &[("src/evolution_graphics.c", 429)]),
    ("SE_M_POISON_POWDER", &[("data/battle_anim_scripts.s", 460)]),
    ("SE_M_PSYBEAM", &[("data/battle_anim_scripts.s", 1349)]),
    ("SE_M_PSYBEAM2", &[("data/battle_anim_scripts.s", 1344)]),
    ("SE_M_RAZOR_WIND", &[("data/battle_anim_scripts.s", 1424)]),
    ("SE_M_RAZOR_WIND2", &[("data/battle_anim_scripts.s", 823)]),
    ("SE_M_REFLECT", &[("data/battle_anim_scripts.s", 1233)]),
    ("SE_M_SACRED_FIRE2", &[("data/battle_anim_scripts.s", 1397)]),
    ("SE_M_SAND_ATTACK", &[("data/battle_anim_scripts.s", 2272)]),
    ("SE_M_SCRATCH", &[("data/battle_anim_scripts.s", 1518)]),
    ("SE_M_SCREECH", &[("data/battle_anim_scripts.s", 655)]),
    ("SE_M_SKETCH", &[("data/battle_anim_scripts.s", 3164)]),
    ("SE_M_SKY_UPPERCUT", &[("data/battle_anim_scripts.s", 2970)]),
    ("SE_M_SNORE", &[("data/battle_anim_scripts.s", 4706)]),
    ("SE_M_SPIT_UP", &[("data/battle_anim_scripts.s", 3779)]),
    (
        "SE_M_STAT_DECREASE",
        &[("src/battle_anim_utility_funcs.c", 564)],
    ),
    (
        "SE_M_STAT_INCREASE",
        &[("src/battle_anim_utility_funcs.c", 562)],
    ),
    ("SE_M_STRING_SHOT", &[("data/battle_anim_scripts.s", 2046)]),
    ("SE_M_STRING_SHOT2", &[("data/battle_anim_scripts.s", 2061)]),
    ("SE_M_SUPERSONIC", &[("data/battle_anim_scripts.s", 640)]),
    ("SE_M_SWAGGER", &[("data/battle_anim_scripts.s", 789)]),
    ("SE_M_SWAGGER2", &[("data/battle_anim_scripts.s", 1715)]),
    ("SE_M_TAIL_WHIP", &[("data/battle_anim_scripts.s", 915)]),
    ("SE_M_TAKE_DOWN", &[("data/battle_anim_scripts.s", 561)]),
    ("SE_M_THUNDERBOLT2", &[("data/battle_anim_scripts.s", 1116)]),
    ("SE_M_TOXIC", &[("data/battle_anim_scripts.s", 5143)]),
    ("SE_M_VITAL_THROW", &[("data/battle_anim_scripts.s", 2179)]),
    ("SE_M_VITAL_THROW2", &[("data/battle_anim_scripts.s", 955)]),
    ("SE_M_WHIRLPOOL", &[("data/battle_anim_scripts.s", 6372)]),
    ("SE_M_WING_ATTACK", &[("data/battle_anim_scripts.s", 6176)]),
];
