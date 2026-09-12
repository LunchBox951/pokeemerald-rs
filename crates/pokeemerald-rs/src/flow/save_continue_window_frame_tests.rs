//! Issue #795: the main-menu window-frame chrome a continuable save renders
//! with, split from [`super::save_continue_tests`]'s round trip (`one
//! module = one concept` `(oop-boundaries)`).

use super::save_continue_tests::{new_game_phase, save_from_the_start_menu};
use super::tests::TempSave;
use super::{menu_type_for, window_frame_for};
use crate::main_menu::{MainMenuScene, MainMenuType};

/// One directory entry for [`synthetic_two_frame_pack_bytes`] -- a small
/// independent copy of `main_menu::tests`' own fixture-building style, not a
/// shared one (that module's helper is private to `main_menu::tests`, and
/// this test lives in a different module tree, module docs above).
struct PackEntry {
    id: &'static str,
    kind_tag: u8,
    meta: Vec<u8>,
    payload: Vec<u8>,
}

fn image_meta(width: u32, height: u32, bit_depth: u8) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(&width.to_le_bytes());
    m.extend_from_slice(&height.to_le_bytes());
    m.push(bit_depth);
    m
}

fn palette_meta(color_count: u16) -> Vec<u8> {
    color_count.to_le_bytes().to_vec()
}

fn write_synthetic_pack(mut entries: Vec<PackEntry>) -> Vec<u8> {
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

/// A minimal pack covering what [`MainMenuScene::from_pack_with_window_frame`]
/// needs, with two distinguishable selectable window frames -- frame 0
/// (green, source file `1.png`) and frame 5 (red, source file `6.png`,
/// `WINDOW_FRAME_TYPE_5`) -- mirroring `main_menu::tests`'
/// `synthetic_main_menu_pack_bytes` so which of the two a scene drew is
/// readable from one border pixel.
fn synthetic_two_frame_pack_bytes() -> Vec<u8> {
    let frame0_pixels = vec![1u8; 24 * 24];
    let mut frame0_palette = vec![0u8; 32];
    let green = rendering::Bgr555::from_channels(0, 31, 0).raw();
    frame0_palette[2..4].copy_from_slice(&green.to_le_bytes());

    let frame5_pixels = vec![1u8; 24 * 24];
    let mut frame5_palette = vec![0u8; 32];
    let red = rendering::Bgr555::from_channels(31, 0, 0).raw();
    frame5_palette[2..4].copy_from_slice(&red.to_le_bytes());

    let font_pixels =
        vec![0u8; (assets::fonts::SHEET_WIDTH * assets::fonts::SHEET_HEIGHT) as usize];

    let mut bg_palette = vec![0u8; 32];
    let dark_blue = rendering::Bgr555::from_channels(4, 4, 16).raw();
    bg_palette[0..2].copy_from_slice(&dark_blue.to_le_bytes());

    write_synthetic_pack(vec![
        PackEntry {
            id: "text-window/image/1",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame0_pixels,
        },
        PackEntry {
            id: "text-window/palette/1",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: frame0_palette,
        },
        PackEntry {
            id: "text-window/image/6",
            kind_tag: 0,
            meta: image_meta(24, 24, 4),
            payload: frame5_pixels,
        },
        PackEntry {
            id: "text-window/palette/6",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: frame5_palette,
        },
        PackEntry {
            id: "font/normal/glyphs",
            kind_tag: 0,
            meta: image_meta(assets::fonts::SHEET_WIDTH, assets::fonts::SHEET_HEIGHT, 2),
            payload: font_pixels,
        },
        PackEntry {
            id: "interface/palette/main_menu_bg",
            kind_tag: 1,
            meta: palette_meta(16),
            payload: bg_palette,
        },
    ])
}

/// A [`synthetic_two_frame_pack_bytes`] file on a scratch path, removed on
/// drop (including on unwind) -- mirrors [`TempSave`].
struct TempPack {
    path: std::path::PathBuf,
}

impl TempPack {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-flow-window-frame-pack-{label}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, synthetic_two_frame_pack_bytes()).unwrap();
        Self { path }
    }
}

impl Drop for TempPack {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

/// Issue #795: the I-6 round trip above proves the *save*, but a
/// continuable save with a chosen window-frame option must also *render*
/// with it -- `MainMenu_FormatSavegameText`'s own box, and every other
/// main-menu box, borders with `GetWindowFrameTilesPal(gSaveBlock2Ptr->
/// optionsWindowFrameType)` (`main_menu.c:2191-2193`), not always
/// `WINDOW_FRAME_TYPE_0`. Drives the real write/reload
/// [`a_saved_game_reloads_into_an_overworld_phase_that_matches_it`] does,
/// then feeds the recovered save through [`menu_type_for`]/
/// [`window_frame_for`] into [`MainMenuScene::from_pack_with_window_frame`]
/// -- the exact same two pure decisions, called the exact same way, as
/// [`crate::flow::advance_scene`]'s own `Title` -> `MainMenu` transition,
/// against a synthetic pack (no local pack needed) instead of
/// `assets::pack::AssetPack::load_default`.
#[test]
fn a_saved_games_own_window_frame_choice_borders_its_main_menu() {
    let temp = TempSave::new("window-frame-round-trip");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    // A mid-game options change: `optionsWindowFrameType` after the player
    // picked frame 5 in the options menu -- not the zeroed fresh-save
    // default `new_game::init_save_blocks` starts every session with.
    phase.save2.options_window_frame_type = 5;
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "a save just written must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(menu_type_for(&saved), MainMenuType::SavedGame);
    assert_eq!(
        window_frame_for(&saved),
        5,
        "the real save round trip must recover the chosen window-frame option"
    );

    let temp_pack = TempPack::new("saved-window-frame");
    let pack = assets::pack::AssetPack::load(&temp_pack.path).unwrap();
    let scene = MainMenuScene::from_pack_with_window_frame(
        &pack,
        menu_type_for(&saved),
        window_frame_for(&saved),
    )
    .unwrap();
    let fb = scene.compose();

    // CONTINUE's top border row (module docs on the geometry) -- undarkened,
    // since CONTINUE is selected by default.
    let frame5_red = rendering::Bgr555::from_channels(31, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(18, 2),
        Some(frame5_red),
        "a save whose optionsWindowFrameType is 5 must draw frame 5's border, \
         not FRAME_ID's frame 0"
    );
}
