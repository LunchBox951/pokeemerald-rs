use pokeemerald_rs::main_menu::MainMenuItem;
use pokeemerald_rs::{AppButtons, AppState};

use super::*;

const SAVE_FLOW_FRAME_BUDGET: usize = 4_000;

fn press(app: &mut App, buttons: AppButtons) {
    app.set_headless_buttons(buttons).unwrap();
    assert!(app.step().unwrap());
    app.set_headless_buttons(AppButtons::NONE).unwrap();
    assert!(app.step().unwrap());
}

#[test]
#[ignore = "needs a local pack produced by `cargo xtask extract`"]
fn real_pack_restart_continues_the_game_the_first_boot_saved() {
    let _pack = crate::extract::REAL_PACK_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let session = Session::new(&PackSelection::checkout()).expect("owned session");

    let mut first = session.start().expect("first boot");
    // Drive the production new-game path to the first overworld frame.
    for frame in super::super::boot_to_first_fight::frames() {
        first.set_headless_buttons(frame.buttons).unwrap();
        assert!(first.step().unwrap());
        if first.state() == AppState::Overworld {
            break;
        }
    }
    assert_eq!(first.state(), AppState::Overworld);
    for _ in 0..60 {
        first.set_headless_buttons(AppButtons::NONE).unwrap();
        assert!(first.step().unwrap());
    }
    // START -> SAVE (BAG, player, SAVE) -> YES, then confirm messages.
    press(&mut first, AppButtons::START);
    press(&mut first, AppButtons::DOWN);
    press(&mut first, AppButtons::DOWN);
    let mut frames = 0;
    while !session.save.exists() {
        assert!(frames < SAVE_FLOW_FRAME_BUDGET, "save never written");
        press(&mut first, AppButtons::A);
        frames += 2;
    }
    drop(first);

    let mut restarted = session.start().expect("restart onto the same save");
    assert_eq!(restarted.state(), AppState::Title);
    // The title ignores input through its setup frames and white fade-in.
    for _ in 0..super::super::TITLE_INIT_FRAMES {
        restarted.set_headless_buttons(AppButtons::NONE).unwrap();
        assert!(restarted.step().unwrap());
    }
    restarted.set_headless_buttons(AppButtons::START).unwrap();
    assert!(restarted.step().unwrap());
    for _ in 0..=super::super::FADE_WAIT_FRAMES {
        restarted.set_headless_buttons(AppButtons::NONE).unwrap();
        assert!(restarted.step().unwrap());
    }
    assert_eq!(
        restarted.state(),
        AppState::MainMenu(MainMenuItem::Continue),
        "the restarted session must offer CONTINUE from the first session's save"
    );
}
