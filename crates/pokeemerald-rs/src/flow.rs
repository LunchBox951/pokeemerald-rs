//! The real (windowed) game-flow state machine (I-3, issue #149): title ->
//! main menu -> intro -> overworld.
//!
//! Split out of [`crate::app`] to keep that module focused on the
//! platform/window shell (`one module = one concept` `(oop-boundaries)`):
//! [`AppScene`] is the state, [`advance_scene`] is the one-frame transition
//! function [`crate::app::App::step`] delegates to. See [`crate::app`]'s
//! module docs for the transition diagram (`Title` -> `MainMenu` -> `Intro`
//! -> `Overworld`) and the "log-or-ignore is fine" failure policy for a
//! transition's own pack load.
//!
//! [`AnimatedTitle`] is the pre-I-3 per-frame title-animation state,
//! unchanged in shape and behaviour from before this issue (see
//! [`advance_scene`]'s `Title` arm and `crate::app`'s "Animating the real
//! title screen" docs) -- only its ownership moved here, into one variant of
//! [`AppScene`] instead of its own dedicated `App` field.

use assets::{MapEventsTable, MapHeaderTable};
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons, Frame};

use crate::frame::to_platform_frame;
use crate::intro::{self, IntroScene, IntroStatus};
use crate::main_menu::{self, MainMenuScene};
use crate::new_game;
use crate::overworld::{self, OverworldScene, OverworldSceneError};
use crate::title::TitleScene;

/// [`crate::app::App`]'s per-frame animation state for the real title screen
/// (`crate::app`'s "Animating the real title screen" docs): the loaded
/// scene, the tick most recently composed, and whether that frame has been
/// presented yet (so [`advance_scene`] advances the tick *before*
/// presenting every frame after the first -- keeping
/// [`crate::app::App::frame`]'s "most recently presented" contract true at
/// all times).
pub(crate) struct AnimatedTitle {
    pub(crate) scene: TitleScene,
    pub(crate) tick: u32,
    pub(crate) presented: bool,
}

/// [`crate::app::App`]'s real (windowed) game-flow state (module docs):
/// which of the four scenes is currently active. Every variant is boxed --
/// their sizes vary wildly (an [`AnimatedTitle`] embeds a whole
/// [`TitleScene`]'s tile/palette data; an [`OverworldPhase`] embeds a whole
/// [`OverworldScene`]'s) -- so the enum itself stays cheap to move around
/// (`clippy::large_enum_variant`).
pub(crate) enum AppScene {
    /// The idle/animating title screen, waiting for Start.
    Title(Box<AnimatedTitle>),
    /// The no-save-present main menu, waiting for A to confirm `NEW GAME`.
    MainMenu(Box<MainMenuScene>),
    /// Birch's speech, paging through [`crate::intro::speech`]'s text.
    Intro(Box<IntroScene<'static>>),
    /// The overworld loop: the player, movable, in
    /// [`crate::new_game::SPAWN_MAP_ID`].
    Overworld(Box<OverworldPhase>),
}

/// The overworld-loop state (module docs): an [`OverworldScene`] to render
/// plus the [`PlayerState`] it renders, together with the map identity
/// needed to re-look-up that map's header and event lists (from the
/// `'static` [`MapHeaderTable`]/[`MapEventsTable`]) every frame -- see
/// [`OverworldPhase::step`].
pub(crate) struct OverworldPhase {
    scene: OverworldScene,
    player: PlayerState,
    map_id: assets::MapId,
}

impl OverworldPhase {
    /// Load [`crate::overworld::load_default_room`] and place the player at
    /// [`new_game::SPAWN_POSITION`] (module docs on why this, not upstream's
    /// truck sequence, is the intro's handoff target).
    fn load_default() -> Result<Self, OverworldSceneError> {
        let scene = overworld::load_default_room()?;
        let player = PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        );
        Ok(Self {
            scene,
            player,
            map_id: new_game::SPAWN_MAP_ID,
        })
    }

    /// Advance the player by one frame: a held D-pad direction (module
    /// docs' [`held_direction`]) attempts a step/turn against a
    /// [`engine::overworld::MapRuntime`] rebuilt fresh this call (mirroring
    /// [`OverworldScene::compose`]'s own "no persisted borrow" pattern --
    /// see the module docs), then the walk-animation timer always ticks.
    /// Silently does nothing if this map's header/events can't be found in
    /// the `'static` tables (unreachable for [`new_game::SPAWN_MAP_ID`]
    /// against a real extraction).
    fn step(&mut self, buttons: ButtonState) {
        let direction = held_direction(buttons);
        if let (Ok(header), Ok(events)) = (
            MapHeaderTable::new().header(self.map_id),
            MapEventsTable::new().resolve(self.map_id),
        ) {
            let runtime = self.scene.runtime(self.map_id, header, events);
            let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
            let _ = self.player.step(direction, &runtime, &no_connections);
        }
        self.player.tick();
    }

    /// [`OverworldScene::compose_frame`] against this phase's current
    /// player state.
    fn compose_frame(&self) -> Box<Frame> {
        self.scene.compose_frame(&self.player)
    }
}

/// The held D-pad direction to feed [`PlayerState::step`] this frame, or
/// `None` if no direction is held. Priority order (first held wins)
/// transcribes upstream `RunFieldInput`'s own `dpadDirection` resolution
/// exactly: `if (heldKeys & DPAD_UP) ... else if (DPAD_DOWN) ... else if
/// (DPAD_LEFT) ... else if (DPAD_RIGHT)`
/// (`pokeemerald/src/field_control_avatar.c:123-129`) -- up, then down,
/// then left, then right, with only one cardinal direction ever selected
/// per call regardless of which other D-pad bits also happen to be held
/// `(behavioral-fidelity)`.
fn held_direction(buttons: ButtonState) -> Option<Direction> {
    let held = buttons.held();
    if held.intersects(Buttons::UP) {
        Some(Direction::North)
    } else if held.intersects(Buttons::DOWN) {
        Some(Direction::South)
    } else if held.intersects(Buttons::LEFT) {
        Some(Direction::West)
    } else if held.intersects(Buttons::RIGHT) {
        Some(Direction::East)
    } else {
        None
    }
}

/// Advance `scene` by exactly one frame given this frame's `buttons`,
/// returning the (possibly transitioned) next scene and the frame it
/// composed -- the pure state-transition core of
/// [`crate::app::App::step`]'s real (windowed) path (module docs), factored
/// out as a free function over an owned [`AppScene`] so it needs no
/// `&mut App` self-borrow and is directly unit-testable.
///
/// Every `Title`/`MainMenu`/`Intro` -> next-scene transition loads its own
/// fresh [`assets::AssetPack`] (mirroring how [`TitleScene`]/
/// [`OverworldScene`] already each load their own pack independently) and
/// composes that scene's first frame immediately, so the returned frame is
/// always the *new* scene's -- never a stale one from the scene being left.
/// If a transition's pack load fails, this logs and returns the *original*
/// scene unchanged instead (module docs).
pub(crate) fn advance_scene(scene: AppScene, buttons: ButtonState) -> (AppScene, Box<Frame>) {
    match scene {
        AppScene::Title(mut title) => {
            if title.presented {
                title.tick = title.tick.wrapping_add(1);
            }
            title.presented = true;

            if buttons.is_newly_pressed(Buttons::START) {
                match main_menu::load_default() {
                    Ok(menu) => {
                        let frame = menu.compose_frame();
                        return (AppScene::MainMenu(Box::new(menu)), frame);
                    }
                    Err(err) => eprintln!("main menu: {err} -- staying on the title screen"),
                }
            }

            let frame = to_platform_frame(&title.scene.compose(title.tick));
            (AppScene::Title(title), frame)
        }
        AppScene::MainMenu(menu) => {
            if buttons.is_newly_pressed(Buttons::A) {
                match intro::load_default() {
                    Ok(intro_scene) => {
                        let frame = intro_scene.compose_frame();
                        return (AppScene::Intro(Box::new(intro_scene)), frame);
                    }
                    Err(err) => eprintln!("intro: {err} -- staying on the main menu"),
                }
            }
            let frame = menu.compose_frame();
            (AppScene::MainMenu(menu), frame)
        }
        AppScene::Intro(mut intro_scene) => {
            let confirm_pressed = buttons.is_newly_pressed(Buttons::A);
            let skip_pressed = buttons.is_newly_pressed(Buttons::B);
            let status = intro_scene.tick(confirm_pressed, skip_pressed);

            if status == IntroStatus::Finished {
                match OverworldPhase::load_default() {
                    Ok(phase) => {
                        let frame = phase.compose_frame();
                        return (AppScene::Overworld(Box::new(phase)), frame);
                    }
                    Err(err) => eprintln!("overworld: {err} -- staying on the intro"),
                }
            }

            let frame = intro_scene.compose_frame();
            (AppScene::Intro(intro_scene), frame)
        }
        AppScene::Overworld(mut phase) => {
            phase.step(buttons);
            let frame = phase.compose_frame();
            (AppScene::Overworld(phase), frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{advance_scene, held_direction, AnimatedTitle, AppScene, OverworldPhase};
    use crate::intro::IntroStatus;
    use crate::new_game;
    use engine::overworld::Direction;
    use platform::{ButtonState, Buttons};

    fn pressed(button: Buttons) -> ButtonState {
        let mut state = ButtonState::new();
        state.update(button);
        state
    }

    fn held(button: Buttons) -> ButtonState {
        // Two updates: the first makes it newly-pressed, the second makes
        // it merely held (matching a real multi-frame hold).
        let mut state = ButtonState::new();
        state.update(button);
        state.update(button);
        state
    }

    #[test]
    fn held_direction_prioritizes_up_over_every_other_direction() {
        // field_control_avatar.c's own if/else-if chain order (see
        // `held_direction`'s doc comment): up beats every simultaneous
        // combination.
        assert_eq!(
            held_direction(held(
                Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT
            )),
            Some(Direction::North)
        );
        assert_eq!(
            held_direction(held(Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT)),
            Some(Direction::South)
        );
        assert_eq!(
            held_direction(held(Buttons::LEFT | Buttons::RIGHT)),
            Some(Direction::West)
        );
        assert_eq!(held_direction(held(Buttons::RIGHT)), Some(Direction::East));
        assert_eq!(held_direction(ButtonState::new()), None);
    }

    /// I-3 scene-flow test: title screen, Start newly pressed -> main menu.
    /// Needs the real pack (both `TitleScene` and
    /// `main_menu::load_default` read from it).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn title_start_button_transitions_to_main_menu() {
        let title_scene = crate::title::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Title(Box::new(AnimatedTitle {
            scene: title_scene,
            tick: 0,
            presented: false,
        }));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::START));

        assert!(
            matches!(next, AppScene::MainMenu(_)),
            "Start on the title screen must transition to the main menu"
        );
    }

    /// I-3 scene-flow test: title screen, no Start press -> stays on title
    /// and keeps animating (the pre-I-3 animated-title behaviour must
    /// survive the state-machine refactor unchanged).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn title_without_start_stays_on_title_and_keeps_animating() {
        let title_scene = crate::title::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Title(Box::new(AnimatedTitle {
            scene: title_scene,
            tick: 0,
            presented: true, // as if this were the second frame onward.
        }));

        let (next, _frame) = advance_scene(scene, ButtonState::new());

        let AppScene::Title(title) = next else {
            panic!("expected to stay on the title screen");
        };
        assert_eq!(title.tick, 1, "the tick must still advance every frame");
    }

    /// I-3 scene-flow test: main menu, A newly pressed -> intro (the menu's
    /// only item, `NEW GAME`, per `crate::main_menu`'s module docs).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn main_menu_confirm_transitions_to_intro() {
        let menu = crate::main_menu::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::MainMenu(Box::new(menu));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::A));

        assert!(
            matches!(next, AppScene::Intro(_)),
            "A on the main menu must transition to the intro"
        );
    }

    /// I-3 scene-flow test: intro finishing (here, via the skip path --
    /// `crate::intro`'s module docs on why B skips the whole intro) hands
    /// off to the overworld with the player placed at the upstream spawn
    /// tile (`crate::new_game::SPAWN_POSITION`), not left at `(0, 0)` or
    /// wherever the intro's own defaults would otherwise leave it.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn intro_skip_transitions_to_overworld_with_the_player_at_the_spawn_tile() {
        let intro_scene = crate::intro::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Intro(Box::new(intro_scene));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::B));

        let AppScene::Overworld(phase) = next else {
            panic!("expected the skipped intro to hand off to the overworld");
        };
        assert_eq!(phase.player.position(), new_game::SPAWN_POSITION);
        assert_eq!(phase.player.elevation(), new_game::SPAWN_ELEVATION);
        assert_eq!(phase.player.facing(), new_game::SPAWN_FACING);
        assert_eq!(phase.map_id, new_game::SPAWN_MAP_ID);
    }

    /// I-3 scene-flow test: the intro's own paged advance-on-confirm (not
    /// just the skip shortcut) also reaches the overworld once every page
    /// is read. Confirms every tick; `IntroScene`'s own headless tests
    /// (`crate::intro::tests`) already cover the finer per-page timing.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn intro_finishing_every_page_also_transitions_to_the_overworld() {
        let mut intro_scene =
            crate::intro::load_default().expect("run `cargo xtask extract` first");
        let mut status = IntroStatus::Continue;
        for _ in 0..20_000 {
            status = intro_scene.tick(true, false);
            if status == IntroStatus::Finished {
                break;
            }
        }
        assert_eq!(status, IntroStatus::Finished, "the intro must terminate");

        let scene = AppScene::Intro(Box::new(intro_scene));
        let (next, _frame) = advance_scene(scene, ButtonState::new());
        assert!(matches!(next, AppScene::Overworld(_)));
    }

    /// I-3 scene-flow test: once in the overworld, a held direction is fed
    /// to the player every frame -- "the player movable" (issue #149's own
    /// scope item 4). A turn always succeeds regardless of the room's
    /// collision layout (only a *step* can be blocked), so this is a safe
    /// assertion without depending on the real map's exact geometry.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn overworld_movement_input_turns_the_player() {
        let scene = crate::overworld::load_default_room().expect("run `cargo xtask extract` first");
        let player = engine::overworld::PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        );
        assert_eq!(player.facing(), Direction::South, "starts facing south");
        let mut phase = OverworldPhase {
            scene,
            player,
            map_id: new_game::SPAWN_MAP_ID,
        };

        phase.step(held(Buttons::UP));

        assert_eq!(
            phase.player.facing(),
            Direction::North,
            "a fresh directional input first turns the player to face it"
        );
    }
}
