//! The pure decisions `advance_scene` acts on at the main menu and the boot
//! hand-off: `menu_action`, `window_frame_for`, `new_game_options_for`, and
//! what a corrupt or intact save leaves for the menu.

use super::{menu_action, new_game_options_for, window_frame_for, MainMenuAction};
use crate::game_save::SavedGame;
use crate::main_menu::MainMenuItem;
use crate::new_game::{self, NewGameOptions};

/// Issue #216 regression, the half the scene-level tests in `main_menu_confirm_tests` cannot
/// see: with no pack, a failed `intro::load_default`, a failed
/// `OverworldPhase::continue_saved_game`, and an Options screen that fails to load
/// all land back on the main menu, so the item -> action mapping itself
/// is pinned here on the pure decision function instead (`menu_action`'s
/// own doc comment).
#[test]
fn menu_action_maps_each_item_to_its_upstream_action() {
    assert_eq!(
        menu_action(MainMenuItem::NewGame),
        MainMenuAction::NewGame,
        "A on NEW GAME is upstream's ACTION_NEW_GAME (main_menu.c:1055-1063)"
    );
    assert_eq!(
        menu_action(MainMenuItem::Continue),
        MainMenuAction::Continue,
        "A on CONTINUE is upstream's ACTION_CONTINUE (main_menu.c:1064-1069)"
    );
    assert_eq!(
        menu_action(MainMenuItem::Option),
        MainMenuAction::Options,
        "A on OPTION is upstream's ACTION_OPTION (main_menu.c:1070-1073)"
    );
}

/// Issue #795: `advance_scene`'s `Title` -> `MainMenu` transition threads
/// `window_frame_for(&saved)` into main-menu construction, so which border
/// a given boot verdict produces is pinned here on the pure decision
/// function too -- the same pack-less treatment [`menu_type_for`]'s own doc
/// comment already gives the menu-type half of that same call.
#[test]
fn window_frame_for_reads_the_saved_blocks_own_option() {
    use crate::game_save::SaveFileStatus;
    use engine::save::{SaveBlock1, SaveBlock2};

    let fresh = SavedGame {
        status: SaveFileStatus::Empty,
        block1: SaveBlock1::default(),
        block2: SaveBlock2::default(),
    };
    assert_eq!(
        window_frame_for(&fresh),
        0,
        "a zeroed, fresh save block's own optionsWindowFrameType is 0 \
         (WINDOW_FRAME_TYPE_0)"
    );

    let saved = SavedGame {
        status: SaveFileStatus::Ok,
        block1: SaveBlock1::default(),
        block2: SaveBlock2 {
            options_window_frame_type: 12,
            ..SaveBlock2::default()
        },
    };
    assert_eq!(
        window_frame_for(&saved),
        12,
        "a continued save's own recovered optionsWindowFrameType must not \
         be discarded for a hardcoded default"
    );
}

/// Pins [`new_game_options_for`]'s status split (issue #1125) -- see that
/// function's own doc comment for the contract this pins, not a copy of it
/// here.
#[test]
fn new_game_options_for_keeps_every_non_defaulted_saves_own_options() {
    use crate::game_save::SaveFileStatus;
    use engine::save::{BattleStyle, ButtonMode, SaveBlock1, SaveBlock2, SoundMode};

    let recovered = |status: SaveFileStatus| SavedGame {
        status,
        block1: SaveBlock1::default(),
        block2: SaveBlock2 {
            options_text_speed: 2,
            options_window_frame_type: 12,
            options_button_mode: ButtonMode::LEqualsA,
            options_sound: SoundMode::Stereo,
            options_battle_style: BattleStyle::Set,
            options_battle_scene_off: true,
            ..SaveBlock2::default()
        },
    };

    for status in [
        SaveFileStatus::Ok,
        SaveFileStatus::Error,
        SaveFileStatus::NoFlash,
    ] {
        let saved = recovered(status);
        assert_eq!(
            new_game_options_for(&saved),
            NewGameOptions {
                text_speed: 2,
                window_frame_type: 12,
                button_mode: ButtonMode::LEqualsA,
                sound: SoundMode::Stereo,
                battle_style: BattleStyle::Set,
                battle_scene_off: true,
            },
            "a {status:?} save's recovered options must survive into NEW GAME"
        );
    }

    for status in [SaveFileStatus::Empty, SaveFileStatus::Corrupt] {
        let mut saved = recovered(status);
        super::apply_boot_defaults(&mut saved);
        assert_eq!(
            new_game_options_for(&saved),
            NewGameOptions::DEFAULT,
            "a {status:?} boot verdict must still default NEW GAME's options"
        );
    }
}

/// Upstream re-clears the save blocks after a corrupt verdict:
/// `CB2_InitCopyrightScreenAfterBootup` calls `Sav2_ClearSetDefault()` when
/// `gSaveFileStatus` is `SAVE_STATUS_EMPTY`/`SAVE_STATUS_CORRUPT`
/// (`intro.c:1154-1156`), and `SetDefaultOptions` puts
/// `optionsWindowFrameType` back to 0 (`new_game.c:91-98`). A corrupt boot
/// still recovers a checksum-valid `SaveBlock2` here, so the no-save main
/// menu must not wear that save's border. `SAVE_STATUS_ERROR` is not cleared
/// upstream -- one slot loaded intact -- so its recovered border survives.
#[test]
fn a_corrupt_boot_falls_back_to_the_default_window_frame() {
    use crate::game_save::SaveFileStatus;
    use engine::save::{SaveBlock1, SaveBlock2};

    let recovered = SaveBlock2 {
        options_window_frame_type: 5,
        ..SaveBlock2::default()
    };

    let mut corrupt = SavedGame {
        status: SaveFileStatus::Corrupt,
        block1: SaveBlock1::default(),
        block2: recovered.clone(),
    };
    super::apply_boot_defaults(&mut corrupt);
    assert_eq!(
        window_frame_for(&corrupt),
        0,
        "a corrupt boot clears SaveBlock2 and re-defaults the option \
         (intro.c:1154-1156 -> new_game.c:91-98)"
    );

    let one_bad_slot = SavedGame {
        status: SaveFileStatus::Error,
        block1: SaveBlock1::default(),
        block2: recovered,
    };
    assert_eq!(
        window_frame_for(&one_bad_slot),
        5,
        "SAVE_STATUS_ERROR loaded an intact slot and is never cleared"
    );
}

/// `NewGameInitData` never touches `optionsButtonMode`, `optionsSound`,
/// `optionsBattleStyle`, or `optionsBattleSceneOff` (`pokeemerald/src/new_game.c:149-207`);
/// only `SetDefaultOptions` via `Sav2_ClearSetDefault` for Empty/Corrupt does.
#[test]
fn new_game_over_an_intact_save_keeps_its_menu_owned_options() {
    use crate::game_save::SaveFileStatus;
    use engine::save::{BattleStyle, ButtonMode, SaveBlock1, SaveBlock2, SoundMode};

    for status in [SaveFileStatus::Ok, SaveFileStatus::Error] {
        let saved = SavedGame {
            status,
            block1: SaveBlock1::default(),
            block2: SaveBlock2 {
                options_button_mode: ButtonMode::Lr,
                options_sound: SoundMode::Stereo,
                options_battle_style: BattleStyle::Set,
                options_battle_scene_off: true,
                ..SaveBlock2::default()
            },
        };
        let mut rng = engine::rng::Rng::new(0);
        let (_, block2) =
            new_game::init_save_blocks_with_options(&mut rng, new_game_options_for(&saved));
        assert_eq!(block2.options_button_mode, ButtonMode::Lr, "{status:?}");
        assert_eq!(block2.options_sound, SoundMode::Stereo, "{status:?}");
        assert_eq!(block2.options_battle_style, BattleStyle::Set, "{status:?}");
        assert!(block2.options_battle_scene_off, "{status:?}");
    }
}
