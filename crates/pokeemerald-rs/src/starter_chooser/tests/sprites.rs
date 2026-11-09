use platform::Buttons;
use rendering::{AffineMode, ObjMode};

use super::support::Driver;
use crate::starter_chooser::sprites::{
    matrices, selected_frame_offset, BallAnimation, HandBob, Preview, Sprites,
};
use crate::starter_chooser::StarterChoice;

#[test]
fn the_selected_ball_cycles_through_its_128_frame_loop() {
    let expected = [
        (0, 16),
        (3, 16),
        (4, 0),
        (7, 0),
        (8, 32),
        (11, 32),
        (12, 0),
        (16, 16),
        (28, 0),
        (31, 0),
        (32, 0),
        (63, 0),
        (64, 16),
        (71, 16),
        (72, 0),
        (79, 0),
        (80, 32),
        (87, 32),
        (96, 16),
        (104, 0),
        (112, 32),
        (120, 0),
        (127, 0),
        (128, 16),
    ];
    for (age, offset) in expected {
        assert_eq!(selected_frame_offset(age), offset, "age {age}");
    }
}

#[test]
fn a_ball_restarts_only_when_its_selection_changes() {
    let mut ball = BallAnimation::default();
    ball.step(true);
    assert_eq!(ball.tile_offset(), 16);
    for _ in 0..4 {
        ball.step(true);
    }
    assert_eq!(ball.tile_offset(), 0);
    ball.step(false);
    assert_eq!(ball.tile_offset(), 0);
    ball.step(true);
    assert_eq!(ball.tile_offset(), 16);
}

#[test]
fn the_hand_bob_follows_upstreams_fixed_point_sine_and_never_resets() {
    let mut hand = HandBob::default();
    let mut offsets = Vec::new();
    for _ in 0..128 {
        hand.step();
        offsets.push(hand.offset());
    }
    for index in [0usize, 2, 8, 16, 24, 32, 33, 40, 48, 56, 63] {
        let expected = [0i8, 1, 5, 8, 5, 0, -1, -6, -8, -6, -1];
        let position = [0usize, 2, 8, 16, 24, 32, 33, 40, 48, 56, 63]
            .iter()
            .position(|&i| i == index)
            .unwrap();
        assert_eq!(offsets[index], expected[position], "frame {index}");
        assert_eq!(offsets[index + 64], offsets[index], "period at {index}");
    }
}

#[test]
fn the_hand_keeps_bobbing_through_a_preview_and_cancel() {
    let mut driver = Driver::ready();
    driver.open_confirmation();
    driver.tap(Buttons::B);
    driver.idle(50);
    let mut reference = HandBob::default();
    for _ in 0..driver.frames {
        reference.step();
    }
    assert_eq!(driver.chooser.sprites.hand.offset(), reference.offset());
    assert!(driver.frames > 64, "the script must outlast one bob period");
}

#[test]
fn ball_hand_and_preview_oam_use_footprint_origins() {
    let mut sprites = Sprites::default();
    sprites.step(StarterChoice::Torchic);
    let entries = sprites.oam_entries(StarterChoice::Torchic);
    // Hand, then balls left to right.
    let hand = entries[0];
    assert_eq!((hand.x(), hand.y()), (104, 40));
    assert_eq!(
        (hand.tile_index(), hand.palette_bank(), hand.priority()),
        (48, 0, 1)
    );
    assert_eq!(hand.dimensions(), (32, 32));
    let balls: Vec<_> = entries[1..].iter().map(|e| (e.x(), e.y())).collect();
    assert_eq!(balls, vec![(44, 48), (104, 72), (164, 48)]);
    assert_eq!(entries[1].tile_index(), 0);
    assert_eq!(entries[2].tile_index(), 16);

    sprites.preview = Some(Preview::new(StarterChoice::Torchic));
    sprites.step(StarterChoice::Torchic);
    let entries = sprites.oam_entries(StarterChoice::Torchic);
    let (mon, circle) = (entries[0], entries[1]);
    assert_eq!((mon.x(), mon.y()), (88, 54));
    assert_eq!((circle.x(), circle.y()), (56, 22));
    assert_eq!(mon.dimensions(), (64, 64));
    assert_eq!(circle.dimensions(), (64, 64));
    assert_eq!(mon.affine(), AffineMode::Affine { matrix_num: 0 });
    assert_eq!(
        circle.affine(),
        AffineMode::AffineDoubleSize { matrix_num: 1 }
    );
    assert_eq!(
        (mon.palette_bank(), mon.priority(), mon.tile_index()),
        (14, 0, 128)
    );
    assert_eq!(
        (
            circle.palette_bank(),
            circle.priority(),
            circle.tile_index()
        ),
        (1, 1, 64)
    );
    assert_eq!(mon.mode(), ObjMode::Normal);
}

#[test]
fn the_settled_preview_sits_at_88_32_and_the_circle_box_at_56_0() {
    let mut sprites = Sprites {
        preview: Some(Preview::new(StarterChoice::Mudkip)),
        ..Sprites::default()
    };
    for _ in 0..20 {
        sprites.step(StarterChoice::Mudkip);
    }
    let entries = sprites.oam_entries(StarterChoice::Mudkip);
    assert_eq!((entries[0].x(), entries[0].y()), (88, 32));
    assert_eq!((entries[1].x(), entries[1].y()), (56, 0));
}

#[test]
fn the_affine_scale_ramps_and_the_matrix_is_its_reciprocal() {
    let mut preview = Preview::new(StarterChoice::Treecko);
    let mut scales = Vec::new();
    for _ in 0..18 {
        preview.step();
        scales.push((
            preview.mon_scale(),
            preview.circle_scale(),
            preview.affine_ended(),
        ));
    }
    assert_eq!(scales[0], (16, 20, false));
    assert_eq!(scales[1], (32, 40, false));
    assert_eq!(scales[15], (256, 320, false));
    assert_eq!(scales[16], (256, 320, true));

    let [mon, circle] = matrices(Some(preview));
    assert_eq!((mon.pa(), mon.pd()), (256, 256));
    assert_eq!((circle.pa(), circle.pd()), (204, 204));
    let [first_mon, first_circle] = matrices(Some({
        let mut p = Preview::new(StarterChoice::Treecko);
        p.step();
        p
    }));
    assert_eq!(first_mon.pa(), 4096);
    assert_eq!(first_circle.pa(), 3276);
}
