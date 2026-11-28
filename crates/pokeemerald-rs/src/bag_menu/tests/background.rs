//! The background tilemap leaves transparent tile pixels to the supplied frame
//! and replaces the frame only where a tile pixel is opaque.

use super::super::pack_fixture::{entries, load, BACKGROUND};
use super::super::{open_from_pack, BagScreenMode, BagScreenRequest};
use super::{at, frame_of, sample_bag, SHOWN_BASE};

/// The tilemap entry for tile 1, which the fixture's screen sheet leaves all
/// palette index 0.
const TRANSPARENT_TILE_ENTRY: u16 = 1;

#[test]
fn a_transparent_background_tile_keeps_the_supplied_frame() {
    let mut all = entries();
    let tilemap = all
        .iter_mut()
        .find(|entry| entry.id == "bag/raw/menu")
        .expect("the fixture has a tilemap");
    // Cell (0, 0) selects the transparent tile; cell (1, 0) keeps tile 0,
    // whose pixels are all opaque index 3.
    tilemap.payload[0..2].copy_from_slice(&TRANSPARENT_TILE_ENTRY.to_le_bytes());
    let pack = load("transparent-tile", all);
    let screen = open_from_pack(
        &pack,
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Battle),
    )
    .expect("the synthetic pack opens");
    let fb = frame_of(&screen);

    assert_eq!(
        at(&fb, 1, 1),
        SHOWN_BASE,
        "the transparent tile keeps the supplied frame"
    );
    assert_eq!(
        at(&fb, 9, 1),
        BACKGROUND,
        "the opaque neighbour still replaces the supplied frame"
    );
}
