//! The start menu's BAG entry: the four black fades that carry the field to
//! the bag screen and back, and the live-save state the bag edits.
//!
//! Upstream runs the trip as separate callbacks and tasks. Choosing BAG
//! starts the field's fade to black on the selecting frame and waits for it
//! (`StartMenuBagCallback`, `src/start_menu.c:670-682`); the bag then blackens
//! its palettes and fades in (`GoToBagMenu`, `SetupBagMenu`,
//! `src/item_menu.c:614-641,769-781`); a B in the list switches to the task
//! that fades the bag out on the following frame (`Task_FadeAndCloseBagMenu`,
//! `src/item_menu.c:1074-1099,1244-1258`); and the field returns with its
//! start menu already drawn on the same item and fades in
//! (`CB2_ReturnToFieldWithOpenMenu`, `src/overworld.c:1670-1674`;
//! `FieldCB_ReturnToFieldOpenStartMenu`, `ReturnToFieldOpenStartMenu`,
//! `src/field_screen_effect.c:95-99,424-443`), where field input stays
//! locked until the menu's own task takes it over (`Task_ShowStartMenu`,
//! `src/start_menu.c:561-576`).
//!
//! The start menu stays on [`OverworldPhase::start_menu`] for the whole trip:
//! the port retains the windows upstream rebuilds, and the field's
//! "a menu is open" freeze keeps holding. [`BagTrip`] owns the frame instead
//! of the menu or the field from the selecting frame until the handoff.
//!
//! Every fade is a delay-zero black [`NormalPaletteFade`]: its `begin` is
//! update call 1 and each later frame is one more call, so a fade is over
//! 23 update calls (`src/palette.c:156-189,406-484,807-822`). Whoever waits
//! for a fade reads it before that frame's update, as upstream's callbacks
//! run before `UpdatePaletteFade` (`src/overworld.c:1465-1475`), so the
//! frame after call 23 is the first to see it finished.
//!
//! The frames upstream spends loading the bag's graphics and rebuilding the
//! field and its menu between two fades are not modelled: each stage begins
//! on the frame that sees the previous fade finished.

use platform::{ButtonState, Frame};
use rendering::{Framebuffer, NormalPaletteFade, PaletteFadeTarget, PaletteStage};

use super::OverworldPhase;
use crate::bag_menu::{self, BagScreen, BagScreenMode, BagScreenRequest, BagScreenTick};
use engine::bag_menu::BagPositions;
use engine::save::{ButtonMode, PlayerGender};

/// Frames after the field fade-in finishes during which the returned menu is
/// drawn but takes no input: the frame after the last update only creates
/// `Task_ShowStartMenu`, and the next installs the callback that reads
/// buttons (`src/field_screen_effect.c:424-437`, `src/start_menu.c:561-576`).
const MENU_TASK_HANDOFF_FRAMES: u8 = 2;

/// The bag state a session keeps: the positions retained between visits and
/// the trip in progress, if any.
#[derive(Debug, Default)]
pub(in crate::flow) struct FieldBag {
    /// `gBagPosition` (`src/item_menu.c`): kept for the session, not per
    /// menu.
    positions: BagPositions,
    trip: Option<BagTrip>,
}

/// Which stage of the trip owns the frame.
#[derive(Debug)]
enum BagTrip {
    /// The field and its menu fade to black.
    FieldOut(NormalPaletteFade),
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
    /// field's fade to black on the frame BAG was selected.
    pub(super) fn begin_bag_trip(&mut self) {
        self.field_bag.trip = Some(BagTrip::FieldOut(black_fade_out()));
    }

    /// Advances the trip by one frame. Returns `false` on the frame the
    /// returned menu takes input back, which the caller then gives to the
    /// menu; a trip that is not running also returns `false`.
    pub(super) fn advance_bag_trip(&mut self, buttons: ButtonState) -> bool {
        let Some(trip) = self.field_bag.trip.take() else {
            return false;
        };
        let next = match trip {
            BagTrip::FieldOut(mut fade) => {
                self.advance_locked_field_frame();
                if fade.is_done() {
                    Some(self.open_bag())
                } else {
                    fade.update();
                    Some(BagTrip::FieldOut(fade))
                }
            }
            BagTrip::BagIn {
                mut screen,
                mut fade,
            } => {
                if fade.is_done() {
                    Some(self.browse(screen, buttons))
                } else {
                    // The bag animates under the fade but hears no input.
                    screen.tick(ButtonState::default());
                    fade.update();
                    Some(BagTrip::BagIn { screen, fade })
                }
            }
            BagTrip::Browsing(screen) => Some(self.browse(screen, buttons)),
            BagTrip::Closing(screen) => Some(BagTrip::BagOut {
                screen,
                fade: black_fade_out(),
            }),
            BagTrip::BagOut { screen, mut fade } => {
                if fade.is_done() {
                    Some(Self::return_to_field())
                } else {
                    fade.update();
                    Some(BagTrip::BagOut { screen, fade })
                }
            }
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

    /// `GoToBagMenu` and `SetupBagMenu` (`src/item_menu.c:614-641,675-783`):
    /// opens the bag from the live save, drawn from black. A bag that cannot
    /// open returns to the field through the same black, as upstream's
    /// failed allocation takes its exit callback (`:614-621`).
    fn open_bag(&self) -> BagTrip {
        match bag_menu::open(&self.pack_source, &self.save1.bag, self.field_bag_request()) {
            Ok(screen) => BagTrip::BagIn {
                screen: Box::new(screen),
                fade: NormalPaletteFade::begin_in(PaletteFadeTarget::Black),
            },
            Err(err) => {
                eprintln!("{err} -- returning to the field");
                Self::return_to_field()
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

    /// `FieldCB_ReturnToFieldOpenStartMenu` (`src/field_screen_effect.c:95-99`):
    /// the field and its menu fade in from black.
    fn return_to_field() -> BagTrip {
        BagTrip::FieldIn {
            fade: NormalPaletteFade::begin_in(PaletteFadeTarget::Black),
            handoff: 0,
        }
    }

    /// One frame of the bag with input. The bag's edits reach the live save
    /// as the screen reports them, so a reorder or a toss is kept even if
    /// nothing is saved to disk afterwards.
    fn browse(&mut self, mut screen: Box<BagScreen>, buttons: ButtonState) -> BagTrip {
        match screen.tick(buttons) {
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
mod save_tests;
#[cfg(test)]
mod test_support;
