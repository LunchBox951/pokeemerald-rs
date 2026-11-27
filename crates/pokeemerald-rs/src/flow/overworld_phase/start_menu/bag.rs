//! The start menu's BAG entry: the four black fades that carry the field to
//! the bag screen and back, and the live-save state the bag edits.
//!
//! Upstream entry and exit points: `StartMenuBagCallback`
//! (`src/start_menu.c:670-682`), `GoToBagMenu` and `SetupBagMenu`
//! (`src/item_menu.c:614-641,769-781`), `Task_FadeAndCloseBagMenu`
//! (`src/item_menu.c:1074-1099,1244-1258`), and
//! `CB2_ReturnToFieldWithOpenMenu` (`src/overworld.c:1670-1674`) with
//! `ReturnToFieldOpenStartMenu` (`src/field_screen_effect.c:95-99,424-443`).
//!
//! Every fade is a delay-zero black [`NormalPaletteFade`]: its `begin` is
//! update call 1 and each later frame is one more call, so a fade is over
//! 23 update calls (`src/palette.c:156-189,406-484,807-822`). Whoever waits
//! for a fade reads it before that frame's update, as upstream's callbacks
//! run before `UpdatePaletteFade` (`src/overworld.c:1465-1475`), so the
//! frame after call 23 is the first to see it finished.
//!
//! Between two fades upstream swaps main callbacks, and each swap or setup
//! state is a frame of its own on a screen that stays black. Counting from
//! the field frame N that sees the fade out finished, frame N+1 runs
//! `CB2_BagMenuFromStartMenu` and frame N+2 runs `CB2_Bag`, whose loop does
//! the whole of `SetupBagMenu` and begins the fade in
//! (`src/start_menu.c:670-682`, `src/item_menu.c:560-563,614-641,669-673,
//! 769-781`). Counting from the bag frame M that sees its fade out finished,
//! the field's fade in begins [`field_return_black_frames`] frames later
//! (`src/item_menu.c:1074-1099`, `src/overworld.c:1625-1645,1961-1984`,
//! `src/start_menu.c:480-552`). The link waits those paths also take are
//! not modelled, nor is the time the console spends inside one callback:
//! `CB2_Bag` cannot leave its loop until a vertical-blank interrupt has run the
//! tile copy it queued (`src/item_menu.c:806-817`, `src/menu.c:1760-1778`,
//! `src/main.c:340-358`), which the port counts as the one frame the
//! callback is.
//!
//! `ReadKeys` runs at the top of every one of those frames, whichever
//! callback owns it (`src/main.c:133,251-275`), so the held-button repeat
//! the bag's list reads (`JOY_REPEAT`, `src/list_menu.c:406-413`) counts
//! through the fades and the black frames while `Task_BagMenu_HandleInput`
//! ignores the keys (`!gPaletteFade.active`, `src/item_menu.c:1217`). A
//! direction pressed on frame F of the trip and held repeats on frame
//! F + 40 and every 5 frames after; the fresh press and the repeats that
//! fall on frames the bag does not read are lost, and the first that falls
//! on one it reads moves the list. [`FieldBag::repeat`] is that counter for
//! the trip. The start menu reads only fresh presses
//! (`src/start_menu.c:595-629`), so nothing carries the count past the trip.

use platform::{ButtonState, Buttons, Frame};
use rendering::{Framebuffer, NormalPaletteFade, PaletteFadeTarget, PaletteStage};

use super::OverworldPhase;
use crate::bag_menu::{self, BagScreen, BagScreenMode, BagScreenRequest, BagScreenTick};
use crate::key_repeat::{self, KeyRepeat};
use engine::bag_menu::BagPositions;
use engine::save::{ButtonMode, PlayerGender};

/// Frames after the field fade-in finishes during which the returned menu is
/// drawn but takes no input: the frame after the last update only creates
/// `Task_ShowStartMenu`, and the next installs the callback that reads
/// buttons (`src/field_screen_effect.c:424-437`, `src/start_menu.c:561-576`).
const MENU_TASK_HANDOFF_FRAMES: u8 = 2;

/// The frame `CB2_ReturnToFieldWithOpenMenu` runs after `Task_CloseBagMenu`
/// installed it: it only sets the field callback and installs
/// `CB2_ReturnToFieldLocal` (`src/item_menu.c:1089-1092`,
/// `src/overworld.c:1625-1636,1670-1675`, `src/main.c:197-201`).
const EXIT_CALLBACK_FRAMES: u8 = 1;

/// `ReturnToFieldLocal` states 0 and 1, a frame each: the map resumes and
/// its graphics load (`src/overworld.c:1638-1645,1965-1977`).
const FIELD_RELOAD_FRAMES: u8 = 2;

/// The first frame of `ReturnToFieldLocal` state 2:
/// `FieldCB_ReturnToFieldOpenStartMenu` only swaps itself for the callback
/// that builds the menu and reports it is not done
/// (`src/overworld.c:1505-1512,1978-1981`,
/// `src/field_screen_effect.c:440-444`, `src/start_menu.c:554-559`).
const FIELD_CALLBACK_SWAP_FRAMES: u8 = 1;

/// `InitStartMenuStep` states 0 to 3, a frame each, before any action is
/// printed (`src/start_menu.c:486-505,543-552`).
const MENU_SETUP_FRAMES: u8 = 4;

/// Actions `InitStartMenuStep` state 4 prints in one frame; the state lasts
/// until the last is printed (`src/start_menu.c:449-478,506-509`).
const MENU_ACTIONS_PER_FRAME: usize = 2;

/// `InitStartMenuStep` state 5: the cursor is drawn and, on the same frame,
/// `ReturnToFieldOpenStartMenu` begins the fade in. Nothing copies the
/// palettes to the screen on that frame, and the next, `ReturnToFieldLocal`
/// state 3, makes no update call, so it is the first to show the fade, at
/// call 1 (`src/start_menu.c:510-513,543-552`,
/// `src/field_screen_effect.c:95-99,433-438`,
/// `src/overworld.c:1638-1645,1756-1777,1982-1983`).
const MENU_CURSOR_FRAMES: u8 = 1;

/// Black frames between the bag frame that sees its fade out finished and
/// the frame the field's fade in is first shown, for a start menu of
/// `menu_items` actions: eleven for the port's four.
fn field_return_black_frames(menu_items: usize) -> u8 {
    let printing = u8::try_from(menu_items.div_ceil(MENU_ACTIONS_PER_FRAME)).unwrap_or(u8::MAX);
    (EXIT_CALLBACK_FRAMES
        + FIELD_RELOAD_FRAMES
        + FIELD_CALLBACK_SWAP_FRAMES
        + MENU_SETUP_FRAMES
        + MENU_CURSOR_FRAMES)
        .saturating_add(printing)
}

/// The bag state a session keeps: the positions retained between visits and
/// the trip in progress, if any.
#[derive(Debug)]
pub(in crate::flow) struct FieldBag {
    /// `gBagPosition` (`src/item_menu.c`): kept for the session, not per
    /// menu.
    positions: BagPositions,
    trip: Option<BagTrip>,
    /// `gMain.keyRepeatCounter` for the trip in progress, advanced with the
    /// real buttons on every frame of it, the choosing frame included.
    repeat: KeyRepeat,
}

impl Default for FieldBag {
    fn default() -> Self {
        Self {
            positions: BagPositions::default(),
            trip: None,
            repeat: KeyRepeat::new(key_repeat::START_DELAY),
        }
    }
}

/// Which stage of the trip owns the frame.
#[derive(Debug)]
enum BagTrip {
    /// The field and its menu fade to black.
    FieldOut(NormalPaletteFade),
    /// The field saw its fade finished; the next frame is
    /// `CB2_BagMenuFromStartMenu`'s. Black.
    BagCallback,
    /// `GoToBagMenu` has the bag; the next frame is `CB2_Bag`'s, which sets
    /// the screen up and begins the fade in. Black.
    BagSetup(Box<BagScreen>),
    /// The bag, drawn from black, fades in.
    BagIn {
        screen: Box<BagScreen>,
        fade: NormalPaletteFade,
    },
    /// The bag has the player's input.
    Browsing(Box<BagScreen>),
    /// B closed the bag; the fade out starts on the next frame.
    Closing(Box<BagScreen>),
    /// The closed bag fades to black.
    BagOut {
        screen: Box<BagScreen>,
        fade: NormalPaletteFade,
    },
    /// The field reloads and its menu is rebuilt, for this many more frames.
    /// Black.
    FieldReload(u8),
    /// The field and its menu fade in. `handoff` counts the frames since the
    /// fade finished.
    FieldIn {
        fade: NormalPaletteFade,
        handoff: u8,
    },
}

fn black_fade_out() -> NormalPaletteFade {
    let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::Black);
    // `BeginNormalPaletteFade` ran once; the frame's own `UpdatePaletteFade`
    // is the second call.
    fade.update();
    fade
}

impl OverworldPhase {
    /// Whether the bag trip owns the frame, so neither the menu nor the field
    /// may read input.
    pub(in crate::flow) const fn bag_trip_active(&self) -> bool {
        self.field_bag.trip.is_some()
    }

    /// `StartMenuBagCallback` (`src/start_menu.c:670-682`): starts the
    /// field's fade to black on the frame BAG was selected. `buttons` are
    /// that frame's, which open the trip's repeat count.
    ///
    /// The count opens at rest behind the held set of the frame before. The
    /// choice is a fresh A, so this frame's raw input differs from the
    /// frame before's and upstream's counter restarts here -- unless L=A
    /// had remapped a held `L` to the `A` now pressed, when the raw input
    /// equals the remembered set and the counter steps instead
    /// (`src/main.c:257-261,279-288`). It stepped from rest: on the frame
    /// before, a raw `L` without `A` could not equal a remapped set.
    pub(super) fn begin_bag_trip(&mut self, buttons: ButtonState) {
        let mut held_before = buttons.raw_held() & !buttons.raw_newly_pressed();
        if self.save2.options_button_mode == ButtonMode::LEqualsA
            && held_before.intersects(Buttons::L)
        {
            held_before |= Buttons::A;
        }
        self.field_bag.repeat = KeyRepeat::resting_after(key_repeat::START_DELAY, held_before);
        self.field_bag.repeat.advance(buttons);
        self.field_bag.trip = Some(BagTrip::FieldOut(black_fade_out()));
    }

    /// Advances the trip by one frame. Returns `false` on the frame the
    /// returned menu takes input back, which the caller then gives to the
    /// menu; a trip that is not running also returns `false`.
    pub(super) fn advance_bag_trip(&mut self, buttons: ButtonState) -> bool {
        let Some(trip) = self.field_bag.trip.take() else {
            return false;
        };
        // `ReadKeys` counts before any callback runs, on every frame.
        let repeated = self.field_bag.repeat.advance(buttons);
        let next = match trip {
            BagTrip::FieldOut(mut fade) => {
                self.advance_locked_field_frame();
                if fade.is_done() {
                    Some(BagTrip::BagCallback)
                } else {
                    fade.update();
                    Some(BagTrip::FieldOut(fade))
                }
            }
            BagTrip::BagCallback => Some(self.open_bag()),
            BagTrip::BagSetup(screen) => Some(BagTrip::BagIn {
                screen,
                fade: NormalPaletteFade::begin_in(PaletteFadeTarget::Black),
            }),
            BagTrip::BagIn {
                mut screen,
                mut fade,
            } => {
                if fade.is_done() {
                    Some(self.browse(screen, buttons, repeated))
                } else {
                    // The bag animates under the fade but hears no input:
                    // neither the fresh presses nor a repeat that falls due.
                    screen.tick_repeated(ButtonState::default(), Buttons::NONE);
                    fade.update();
                    Some(BagTrip::BagIn { screen, fade })
                }
            }
            BagTrip::Browsing(screen) => Some(self.browse(screen, buttons, repeated)),
            // The closed bag keeps animating under its fade out, as
            // `CB2_BagMenuRun` still runs `AnimateSprites` and the arrows'
            // tasks until `Task_CloseBagMenu` frees them
            // (`src/item_menu.c:643-649,1074-1099`); it hears no input.
            BagTrip::Closing(mut screen) => {
                screen.tick_repeated(ButtonState::default(), Buttons::NONE);
                Some(BagTrip::BagOut {
                    screen,
                    fade: black_fade_out(),
                })
            }
            BagTrip::BagOut {
                mut screen,
                mut fade,
            } => {
                if fade.is_done() {
                    Some(self.reload_field())
                } else {
                    screen.tick_repeated(ButtonState::default(), Buttons::NONE);
                    fade.update();
                    Some(BagTrip::BagOut { screen, fade })
                }
            }
            BagTrip::FieldReload(0) => Some(BagTrip::FieldIn {
                fade: NormalPaletteFade::begin_in(PaletteFadeTarget::Black),
                handoff: 0,
            }),
            BagTrip::FieldReload(left) => Some(BagTrip::FieldReload(left - 1)),
            BagTrip::FieldIn { mut fade, handoff } => {
                if fade.is_done() && handoff >= MENU_TASK_HANDOFF_FRAMES {
                    None
                } else {
                    self.advance_locked_field_frame();
                    if fade.is_done() {
                        Some(BagTrip::FieldIn {
                            fade,
                            handoff: handoff + 1,
                        })
                    } else {
                        fade.update();
                        Some(BagTrip::FieldIn { fade, handoff })
                    }
                }
            }
        };
        let owned = next.is_some();
        self.field_bag.trip = next;
        owned
    }

    /// `GoToBagMenu` (`src/item_menu.c:614-641`): takes the bag from the
    /// live save on `CB2_BagMenuFromStartMenu`'s frame, for `CB2_Bag` to set
    /// up on the next. A bag that cannot open returns to the field through
    /// the same black, as upstream's failed allocation installs its exit
    /// callback on this frame (`:614-621`).
    ///
    /// Setting the bag up packs the save's own pockets
    /// (`UpdatePocketItemLists`, `src/item_menu.c:725-726,1131-1136`), which
    /// a bag that failed to open never reaches. Nothing reads the save
    /// between this frame and the next, so the screen opens over the pockets
    /// already packed.
    fn open_bag(&mut self) -> BagTrip {
        let mut bag = self.save1.bag.clone();
        bag_menu::pack_pockets(&mut bag);
        match bag_menu::open(&self.pack_source, &bag, self.field_bag_request()) {
            Ok(screen) => {
                self.save1.bag = bag;
                BagTrip::BagSetup(Box::new(screen))
            }
            Err(err) => {
                eprintln!("{err} -- returning to the field");
                self.reload_field()
            }
        }
    }

    /// The bag's request from the live save (`CB2_BagMenuFromStartMenu`,
    /// `src/item_menu.c:560-563`): the registered item and the options come
    /// from the save blocks, the positions from the session.
    fn field_bag_request(&self) -> BagScreenRequest {
        BagScreenRequest {
            positions: self.field_bag.positions,
            registered_item: Some(self.save1.registered_item)
                .filter(|item| *item != assets::items::ItemId::NONE),
            window_frame: self.save2.options_window_frame_type,
            female: self.save2.player_gender == PlayerGender::Female,
            lr_buttons: self.save2.options_button_mode == ButtonMode::Lr,
            ..BagScreenRequest::new(BagScreenMode::Field)
        }
    }

    /// The frame that installed the exit callback: the black frames of the
    /// field's reload and the menu's rebuild follow, then the fade in
    /// (`FieldCB_ReturnToFieldOpenStartMenu`,
    /// `src/field_screen_effect.c:95-99,433-444`).
    ///
    /// The reload destroys the field's sprites in `ReturnToFieldLocal` state
    /// 0 and zeroes the tileset animation counters in state 1
    /// (`src/overworld.c:1965-1977,2131-2134`). Nothing counts or draws
    /// either until `CB2_Overworld` runs again on the frame after the fade
    /// in begins (`:1465-1475,1638-1645`), so both are restarted here.
    fn reload_field(&mut self) -> BagTrip {
        self.reinit_field_tileset_animations();
        let menu_items = self
            .start_menu
            .as_ref()
            .map_or(0, crate::start_menu::StartMenu::item_count);
        BagTrip::FieldReload(field_return_black_frames(menu_items))
    }

    /// One frame of the bag with input. The bag's edits reach the live save
    /// as the screen reports them, so a reorder or a toss is kept even if
    /// nothing is saved to disk afterwards. `repeated` is the trip's
    /// `gMain.newAndRepeatedKeys` for the frame.
    fn browse(
        &mut self,
        mut screen: Box<BagScreen>,
        buttons: ButtonState,
        repeated: Buttons,
    ) -> BagTrip {
        match screen.tick_repeated(buttons, repeated) {
            // Item actions (USE, GIVE, registration) are not routed yet; the
            // screen stays open and nothing changes.
            BagScreenTick::Open | BagScreenTick::Action { .. } => BagTrip::Browsing(screen),
            BagScreenTick::Reordered { bag } | BagScreenTick::Tossed { bag } => {
                self.save1.bag = *bag;
                BagTrip::Browsing(screen)
            }
            BagScreenTick::Closed(_) => {
                self.field_bag.positions = screen.positions();
                BagTrip::Closing(screen)
            }
        }
    }

    /// The composed frame for the trip stage that owns it, or `None` with no
    /// trip running.
    pub(in crate::flow::overworld_phase) fn compose_bag_trip_frame(&self) -> Option<Box<Frame>> {
        let frame = match self.field_bag.trip.as_ref()? {
            BagTrip::FieldOut(fade) | BagTrip::FieldIn { fade, .. } => {
                self.compose_frame_staged(fade.palette_stage())
            }
            // Nothing copies a palette to the screen between two fades, so
            // it keeps the black the last one ended on.
            BagTrip::BagCallback | BagTrip::BagSetup(_) | BagTrip::FieldReload(_) => {
                crate::frame::to_platform_frame(&Framebuffer::new())
            }
            BagTrip::BagIn { screen, fade } | BagTrip::BagOut { screen, fade } => {
                bag_frame(screen, fade.palette_stage())
            }
            BagTrip::Browsing(screen) | BagTrip::Closing(screen) => {
                bag_frame(screen, PaletteStage::default())
            }
        };
        Some(frame)
    }
}

/// The bag over black: its transparent pixels show the screen the bag
/// covers, which is black until the bag's own palettes fade in.
fn bag_frame(screen: &BagScreen, palette: PaletteStage<'_>) -> Box<Frame> {
    crate::frame::to_platform_frame(&screen.compose_over(Framebuffer::new(), palette))
}

#[cfg(test)]
mod fade_tests;
#[cfg(test)]
mod field_reinit_tests;
#[cfg(test)]
mod save_tests;
#[cfg(test)]
mod test_support;
