//! The BAG trip's four fades and input lock, through the real scene driver:
//! order, length, the black seams, and who owns input when.
//!
//! Frame numbers count from the frame A chose BAG (frame 0). The trip is:
//!
//! * field out: frames 0-21, update calls 2-23 (the choosing frame runs
//!   `BeginNormalPaletteFade`'s call and its own);
//! * frame 22 is the field's, seeing the fade finished, and frame 23 the
//!   bag's opening callback: both black;
//! * frame 24 sets the bag up at call 1 of its fade in, which ends at call
//!   23 on frame 46;
//! * the bag takes input from frame 47; B on frame 48 only closes it;
//! * bag out: frames 49-70, update calls 2-23;
//! * frame 71 sees it finished, and frames 72-82 reload the field and
//!   rebuild its four-action menu: all black;
//! * frame 83 shows the field's fade in at call 1, call 23 is on frame 105,
//!   and the returned menu takes input on frame 108.
//!
//! A fade to black is wholly black from call 18 and a fade from black until
//! call 2, so the screen is black on frames 16-25 and 65-84.

use platform::Buttons;
use rendering::{NormalPaletteFade, PaletteFadeTarget, PaletteStage};

use super::test_support::{
    fade_in_after, fade_out_after, faded, is_black, packed, pixel, Session, FADE_CALLS,
};
use super::OverworldPhase;
use crate::bag_menu::host_fixture;
use crate::start_menu::StartMenuItem;

const FIELD_OUT_FIRST: usize = 0;
/// The field frame that sees its fade out finished.
const FIELD_OUT_SEEN: usize = 22;
/// Black frames between the field's fade out and the bag's fade in: the
/// field's last and `CB2_BagMenuFromStartMenu`'s.
const BAG_OPEN_BLACK_FRAMES: usize = 2;
const BAG_FADE_IN_FIRST: usize = FIELD_OUT_SEEN + BAG_OPEN_BLACK_FRAMES;
const BAG_INPUT: usize = 47;
const BAG_CLOSING: usize = 48;
const BAG_FADE_OUT_FIRST: usize = 49;
/// The bag frame that sees its fade out finished.
const BAG_OUT_SEEN: usize = 71;
/// Black frames between the bag's fade out and the field's fade in: the
/// bag's last, the exit callback's, two of field reload, the field
/// callback's swap, and six of a four-action menu's rebuild, on the last of
/// which the fade begins unseen.
const FIELD_RETURN_BLACK_FRAMES: usize = 12;
const FIELD_FADE_IN_FIRST: usize = BAG_OUT_SEEN + FIELD_RETURN_BLACK_FRAMES;
const MENU_INPUT: usize = 108;
/// Calls of a fade to black still showing colour, and of a fade from black
/// still wholly black.
const FADE_OUT_COLOUR_CALLS: usize = 17;
const FADE_IN_BLACK_CALLS: usize = 2;

/// A BG pixel of the menu's white window fill, away from its text.
const MENU_FILL_AT: (usize, usize) = (228, 84);
const GROUND_AT: (usize, usize) = (0, 0);

/// The unfaded colours the field's frames are sampled against.
struct FieldSamples {
    ground: u32,
    avatar: (usize, usize),
    avatar_color: u32,
    menu_fill: u32,
}

impl FieldSamples {
    fn of(phase: &OverworldPhase) -> Self {
        let frame = phase.compose_frame();
        let ground = pixel(&frame, GROUND_AT);
        // A sprite pixel is one the OBJ bank alone changes.
        let objects_black = NormalPaletteFade::begin_in(PaletteFadeTarget::Black).obj_blend();
        let without_sprites = phase.compose_frame_staged(PaletteStage {
            bg: None,
            obj: Some(&objects_black),
        });
        let avatar = (0..160)
            .flat_map(|y| (0..176).map(move |x| (x, y)))
            .find(|&at| pixel(&frame, at) != pixel(&without_sprites, at))
            .expect("the avatar is a sprite");
        let frame = &frame;
        let samples = Self {
            ground,
            avatar,
            avatar_color: pixel(frame, avatar),
            menu_fill: pixel(frame, MENU_FILL_AT),
        };
        assert_eq!(samples.menu_fill, 0x00FF_FFFF, "the window fill is white");
        assert!(samples.ground != 0 && samples.avatar_color != 0);
        samples
    }

    /// Whether `frame` shows the field and its menu with the BG bank at
    /// `bg` and the OBJ bank at `obj` sixteenths of the way to black.
    fn shows(&self, frame: &platform::Frame, (bg, obj): (u8, u8)) -> bool {
        pixel(frame, GROUND_AT) == faded(self.ground, bg)
            && pixel(frame, MENU_FILL_AT) == faded(self.menu_fill, bg)
            && pixel(frame, self.avatar) == faded(self.avatar_color, obj)
    }
}

/// The open bag's colours: a background pixel and the bag picture sprite.
struct BagSamples {
    background: (usize, usize),
    background_color: u32,
}

impl BagSamples {
    fn of(frame: &platform::Frame) -> Self {
        let background_color = packed(host_fixture::BACKGROUND);
        let background = (0..160)
            .flat_map(|y| (0..240).map(move |x| (x, y)))
            .find(|&at| pixel(frame, at) == background_color)
            .expect("the bag's background shows");
        assert_eq!(
            pixel(frame, host_fixture::BAG_PICTURE_AT),
            packed(host_fixture::bag_picture_color()),
            "the Items pocket's bag picture shows"
        );
        Self {
            background,
            background_color,
        }
    }

    fn shows(&self, frame: &platform::Frame, (bg, obj): (u8, u8)) -> bool {
        pixel(frame, self.background) == faded(self.background_color, bg)
            && pixel(frame, host_fixture::BAG_PICTURE_AT)
                == faded(packed(host_fixture::bag_picture_color()), obj)
    }
}

fn session(label: &str) -> Session {
    Session::new(label, &host_fixture::pack_bytes())
}

/// The field with its menu, each of the four fades in order and to length,
/// and a black frame at every seam between them.
#[test]
fn choosing_bag_fades_the_field_out_the_bag_in_and_out_and_the_field_back_in() {
    let mut s = session("four-fades");
    s.open_start_menu();
    s.idle(2);
    s.frame(Buttons::NONE);
    let field = FieldSamples::of(s.phase());

    // The choosing frame is frame 0 of the field's fade out.
    let mut frames = vec![s.choose_bag()];
    while frames.len() < BAG_FADE_IN_FIRST {
        frames.push(s.frame(Buttons::NONE));
    }
    // Frames 22 and 23 are black; frame 24 and the 22 after it are the
    // bag's fade in; frame 47 is the first the bag listens on.
    while frames.len() < BAG_INPUT {
        frames.push(s.frame(Buttons::NONE));
    }
    let open = s.frame(Buttons::NONE);
    assert!(s.browsing(), "the bag listens from frame {BAG_INPUT}");
    frames.push(open.clone());
    let bag = BagSamples::of(&open);
    // B closes the bag on frame 48 without starting the fade.
    frames.push(s.frame(Buttons::B));
    while frames.len() < MENU_INPUT {
        frames.push(s.frame(Buttons::NONE));
    }

    for (index, frame) in frames.iter().enumerate() {
        let before_bag = index < FIELD_OUT_SEEN;
        let between_fades = (FIELD_OUT_SEEN..BAG_FADE_IN_FIRST).contains(&index)
            || (BAG_OUT_SEEN..FIELD_FADE_IN_FIRST).contains(&index);
        let in_bag_fade_in = (BAG_FADE_IN_FIRST..BAG_INPUT).contains(&index);
        let in_bag_fade_out = (BAG_FADE_OUT_FIRST..BAG_OUT_SEEN).contains(&index);
        let in_field_fade_in = index >= FIELD_FADE_IN_FIRST;
        let label = format!("frame {index}");
        if between_fades {
            assert!(is_black(frame), "{label}: black between two fades");
        } else if before_bag {
            let n = index - FIELD_OUT_FIRST + 2;
            assert!(
                field.shows(frame, fade_out_after(n)),
                "{label}: field out, call {n}"
            );
        } else if in_bag_fade_in {
            let n = index - BAG_FADE_IN_FIRST + 1;
            assert!(
                bag.shows(frame, fade_in_after(n)),
                "{label}: bag in, call {n}"
            );
        } else if in_bag_fade_out {
            let n = index - BAG_FADE_OUT_FIRST + 2;
            assert!(
                bag.shows(frame, fade_out_after(n)),
                "{label}: bag out, call {n}"
            );
        } else if in_field_fade_in {
            let n = (index - FIELD_FADE_IN_FIRST + 1).min(FADE_CALLS);
            assert!(
                field.shows(frame, fade_in_after(n)),
                "{label}: field in, call {n}"
            );
        } else {
            assert!(bag.shows(frame, (0, 0)), "{label}: the open bag");
        }
    }

    // Each fade ends on update call 23, black, and the next stage starts on
    // black: the frame before and after every seam is wholly black.
    let seams = [
        (FIELD_OUT_SEEN - 1, BAG_FADE_IN_FIRST),
        (BAG_OUT_SEEN - 1, FIELD_FADE_IN_FIRST),
    ];
    for (last, first) in seams {
        assert!(is_black(&frames[last]), "frame {last} ends a fade on black");
        assert!(
            is_black(&frames[first]),
            "frame {first} starts a fade on black"
        );
    }
    // The fades themselves are not black: the stages around the seams show
    // content, so an always-black implementation cannot pass.
    assert!(!is_black(&frames[0]) && !is_black(&frames[BAG_INPUT]));
    assert!(!is_black(&frames[BAG_FADE_OUT_FIRST - 1]));
    assert!(field.shows(&frames[MENU_INPUT - 1], (0, 0)));
}

/// No press is read while a fade owns the frame, and the returned menu does
/// not take input until its task has: A, B, START and the D-pad pressed
/// afresh on every frame of the trip change nothing.
#[test]
fn input_is_locked_until_the_menu_owns_it_again() {
    let mut s = session("input-lock");
    s.open_start_menu();
    s.idle(2);
    let position = s.phase().player.position();
    let save_bytes = s.phase().save1.bag.clone();
    s.choose_bag();
    s.frame(Buttons::NONE);

    let presses = [
        Buttons::B,
        Buttons::DOWN,
        Buttons::A,
        Buttons::START,
        Buttons::UP,
    ];
    let inject = |s: &mut Session, frame: usize| {
        let button = if frame.is_multiple_of(2) {
            presses[(frame / 2) % presses.len()]
        } else {
            Buttons::NONE
        };
        s.frame(button);
    };

    // Frames 2-46: the field fades out, the bag is set up and fades in.
    for frame in 2..BAG_INPUT {
        inject(&mut s, frame);
        assert_eq!(s.clock, frame + 1);
    }
    // The bag listens from frame 47: nothing pressed in the fades closed it.
    s.frame(Buttons::NONE);
    assert!(s.browsing(), "nothing pressed in the fades closed the bag");
    s.frame(Buttons::B);
    assert!(!s.browsing(), "B on frame {BAG_CLOSING} closes the bag");

    // Frames 49-107: the bag fades out, the field reloads in black and
    // comes back. The menu is not ticked before frame 108, so its cursor
    // never leaves BAG.
    for frame in BAG_FADE_OUT_FIRST..MENU_INPUT {
        inject(&mut s, frame);
        assert!(
            s.phase().bag_trip_active(),
            "frame {frame} is still the trip"
        );
        assert_eq!(
            s.phase()
                .start_menu()
                .map(crate::start_menu::StartMenu::selected),
            Some(StartMenuItem::Bag),
            "frame {frame}"
        );
    }
    assert_eq!(s.phase().player.position(), position);
    assert_eq!(s.phase().save1.bag, save_bytes);

    // Frame 108 releases the trip: a fresh DOWN now moves the menu cursor,
    // and the player still does not walk.
    s.frame(Buttons::NONE);
    assert!(!s.phase().bag_trip_active());
    s.frame(Buttons::DOWN);
    assert_eq!(
        s.phase()
            .start_menu()
            .map(crate::start_menu::StartMenu::selected),
        Some(StartMenuItem::Save)
    );
    assert_eq!(s.phase().player.position(), position);
}

/// A bag that cannot open takes the field back through black to the menu on
/// BAG, with the save untouched and no reload attempt on later frames.
#[test]
fn a_bag_that_cannot_open_returns_to_the_menu_through_black() {
    let mut s = Session::new(
        "bag-pack-missing",
        &host_fixture::pack_bytes_without_the_bag_screen(),
    );
    s.open_start_menu();
    s.idle(2);
    let bag = s.phase().save1.bag.clone();
    s.choose_bag();

    // The open fails on the bag's opening callback, the frame after the
    // field's last (`GoToBagMenu`, `src/item_menu.c:614-621`).
    let failed_open = FIELD_OUT_SEEN + 1;
    let mut seam = None;
    for frame in 1..=failed_open {
        seam = Some((frame, s.frame(Buttons::NONE)));
    }
    let (frame, seam) = seam.expect("the loop ran");
    assert_eq!(frame, failed_open);
    assert!(
        is_black(&seam),
        "the failed open starts the return on black"
    );
    let loads = crate::pack_source::pack_loads_on_this_thread();

    // That frame installed the exit callback, as the bag's last does on a
    // close, so the same black frames follow it, then the fade in's call 1.
    for _ in 0..FIELD_RETURN_BLACK_FRAMES {
        assert!(is_black(&s.frame(Buttons::NONE)));
    }
    // Update calls 2-23 of the fade in, then the two handoff frames, keep the
    // trip; the next frame returns the menu.
    let mut shown = false;
    for _ in 1..FADE_CALLS {
        shown |= !is_black(&s.frame(Buttons::NONE));
    }
    assert!(shown, "the field fades in");
    for _ in 0..2 {
        s.frame(Buttons::NONE);
        assert!(s.phase().bag_trip_active());
    }
    s.frame(Buttons::NONE);
    assert!(!s.phase().bag_trip_active());
    assert_eq!(
        crate::pack_source::pack_loads_on_this_thread(),
        loads,
        "the open is not retried on later frames"
    );
    assert_eq!(
        s.phase()
            .start_menu()
            .map(crate::start_menu::StartMenu::selected),
        Some(StartMenuItem::Bag)
    );
    assert_eq!(s.phase().save1.bag, bag);
}

/// The trip's frames from the choosing A (frame 0) through `last`, with B
/// on the frame after the bag first listens.
fn trip_frames(s: &mut Session, last: usize) -> Vec<Box<platform::Frame>> {
    s.open_start_menu();
    s.idle(2);
    s.frame(Buttons::NONE);
    let mut frames = vec![s.choose_bag()];
    while frames.len() <= last {
        let held = if frames.len() == BAG_CLOSING {
            Buttons::B
        } else {
            Buttons::NONE
        };
        frames.push(s.frame(held));
    }
    frames
}

/// The indices of the wholly black frames in `frames[from..]`.
fn black_frames(frames: &[Box<platform::Frame>], from: usize) -> Vec<usize> {
    (from..frames.len())
        .filter(|&index| is_black(&frames[index]))
        .collect()
}

/// Upstream does not begin the bag's fade in on the frame the field's fade
/// out is seen finished: that frame only installs `CB2_BagMenuFromStartMenu`
/// (`src/start_menu.c:670-682`), whose own frame only installs `CB2_Bag`
/// (`src/item_menu.c:560-563,614-641`), and `CB2_Bag`'s frame sets the bag
/// up and begins the fade (`:669-673,769-781`). Two more frames are black
/// than the two fades alone give.
#[test]
fn opening_the_bag_holds_two_black_frames_before_its_fade_in() {
    let mut s = session("open-black");
    let frames = trip_frames(&mut s, BAG_INPUT);

    let first = FADE_OUT_COLOUR_CALLS - 1;
    let end = BAG_FADE_IN_FIRST + FADE_IN_BLACK_CALLS;
    assert_eq!(
        black_frames(&frames, 0),
        (first..end).collect::<Vec<_>>(),
        "black from the fade out's call 18 to the fade in's call 2"
    );
    assert_eq!(
        end - first,
        (FADE_CALLS - FADE_OUT_COLOUR_CALLS) + BAG_OPEN_BLACK_FRAMES + FADE_IN_BLACK_CALLS
    );
    assert_eq!(end - first, 10);
}

/// Upstream spends frames between the bag's fade out finishing and the
/// field's fade in starting: `Task_CloseBagMenu` installs the exit callback
/// (`src/item_menu.c:1080-1099`); `CB2_ReturnToFieldWithOpenMenu` installs
/// `CB2_ReturnToFieldLocal` (`src/overworld.c:1625-1636,1670-1675`), whose
/// states 0 and 1 take a frame each and whose state 2 first swaps the field
/// callback, then steps `InitStartMenuStep` once a frame, states 0-3, two of
/// state 4 for four actions, and state 5, where `ReturnToFieldOpenStartMenu`
/// begins the fade (`:1961-1984`, `src/start_menu.c:449-552`). Twelve more
/// frames are black than the two fades alone give, and the menu is not back
/// in the player's hands any sooner.
#[test]
fn closing_the_bag_stays_black_through_the_field_reload_and_menu_rebuild() {
    let mut s = session("reload-black");
    let frames = trip_frames(&mut s, MENU_INPUT - 1);

    let first = BAG_FADE_OUT_FIRST + FADE_OUT_COLOUR_CALLS - 1;
    let end = FIELD_FADE_IN_FIRST + FADE_IN_BLACK_CALLS;
    assert_eq!(
        black_frames(&frames, BAG_INPUT),
        (first..end).collect::<Vec<_>>(),
        "black from the fade out's call 18 to the fade in's call 2"
    );
    assert_eq!(
        end - first,
        (FADE_CALLS - FADE_OUT_COLOUR_CALLS) + FIELD_RETURN_BLACK_FRAMES + FADE_IN_BLACK_CALLS
    );
    assert_eq!(end - first, 20);

    // The trip still owns the last frame before the menu's, and gives the
    // next one to the menu.
    assert!(s.phase().bag_trip_active());
    s.frame(Buttons::NONE);
    assert!(!s.phase().bag_trip_active());
}

/// The rebuild prints two actions a frame (`PrintStartMenuActions`,
/// `src/start_menu.c:449-478,506-509`), so the black lasts a frame longer
/// for every two actions the menu lists.
#[test]
fn the_return_black_grows_with_the_menu_it_rebuilds() {
    let frames = super::field_return_black_frames;
    assert_eq!(usize::from(frames(4)) + 1, FIELD_RETURN_BLACK_FRAMES);
    assert_eq!(frames(3), frames(4));
    assert_eq!(frames(5), frames(4) + 1);
    assert_eq!(frames(8), frames(4) + 2);
}
