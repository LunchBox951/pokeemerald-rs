//! The BAG trip's four fades and input lock, through the real scene driver:
//! order, length, the black seams, and who owns input when.
//!
//! Frame numbers count from the frame A chose BAG (frame 0). The trip is:
//!
//! * field out: frames 0-21, update calls 2-23 (the choosing frame runs
//!   `BeginNormalPaletteFade`'s call and its own);
//! * frame 22 sees the fade finished and opens the bag at call 1 of its
//!   fade in, which ends at call 23 on frame 44;
//! * the bag takes input from frame 45; B on frame 46 only closes it;
//! * bag out: frames 47-68, update calls 2-23;
//! * frame 69 sees it finished and starts the field's fade in, call 23 on
//!   frame 91, and the returned menu takes input on frame 94.

use platform::Buttons;
use rendering::{NormalPaletteFade, PaletteFadeTarget, PaletteStage};

use super::test_support::{
    fade_in_after, fade_out_after, faded, is_black, packed, pixel, Session, FADE_CALLS,
};
use super::OverworldPhase;
use crate::bag_menu::host_fixture;
use crate::start_menu::StartMenuItem;

const FIELD_OUT_FIRST: usize = 0;
const BAG_FADE_IN_FIRST: usize = 22;
const BAG_INPUT: usize = 45;
const BAG_CLOSING: usize = 46;
const BAG_FADE_OUT_FIRST: usize = 47;
const FIELD_FADE_IN_FIRST: usize = 69;
const MENU_INPUT: usize = 94;

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
    // Frame 22 and the 22 after it are the bag's fade in; frame 45 is the
    // first the bag listens on.
    while frames.len() < BAG_INPUT {
        frames.push(s.frame(Buttons::NONE));
    }
    let open = s.frame(Buttons::NONE);
    assert!(s.browsing(), "the bag listens from frame {BAG_INPUT}");
    frames.push(open.clone());
    let bag = BagSamples::of(&open);
    // B closes the bag on frame 46 without starting the fade.
    frames.push(s.frame(Buttons::B));
    while frames.len() < MENU_INPUT {
        frames.push(s.frame(Buttons::NONE));
    }

    for (index, frame) in frames.iter().enumerate() {
        let before_bag = index < BAG_FADE_IN_FIRST;
        let in_bag_fade_in = (BAG_FADE_IN_FIRST..BAG_INPUT).contains(&index);
        let in_bag_fade_out = (BAG_FADE_OUT_FIRST..FIELD_FADE_IN_FIRST).contains(&index);
        let in_field_fade_in = index >= FIELD_FADE_IN_FIRST;
        let label = format!("frame {index}");
        if before_bag {
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
        (BAG_FADE_IN_FIRST - 1, BAG_FADE_IN_FIRST),
        (FIELD_FADE_IN_FIRST - 1, FIELD_FADE_IN_FIRST),
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

    // Frames 2-44: the field fades out and the bag fades in.
    for frame in 2..BAG_INPUT {
        inject(&mut s, frame);
        assert_eq!(s.clock, frame + 1);
    }
    // The bag listens from frame 45: nothing pressed in the fades closed it.
    s.frame(Buttons::NONE);
    assert!(s.browsing(), "nothing pressed in the fades closed the bag");
    s.frame(Buttons::B);
    assert!(!s.browsing(), "B on frame {BAG_CLOSING} closes the bag");

    // Frames 47-93: the bag fades out and the field comes back. The menu
    // is not ticked before frame 94, so its cursor never leaves BAG.
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

    // Frame 94 releases the trip: a fresh DOWN now moves the menu cursor,
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

    let mut seam = None;
    for frame in 1..=BAG_FADE_IN_FIRST {
        seam = Some((frame, s.frame(Buttons::NONE)));
    }
    let (frame, seam) = seam.expect("the loop ran");
    assert_eq!(frame, BAG_FADE_IN_FIRST);
    assert!(
        is_black(&seam),
        "the failed open starts the return on black"
    );
    let loads = crate::pack_source::pack_loads_on_this_thread();

    // Update calls 2-23 of the fade in, then the two handoff frames, keep the
    // trip; the next frame returns the menu.
    for _ in 1..FADE_CALLS {
        s.frame(Buttons::NONE);
    }
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
