use super::*;
use crate::save::block::SaveBlock1;

const ROUTE_101: (u8, u8) = (0, 16);
const WEST: u8 = 3;
const LOOK_AROUND: u8 = 1;

fn youngster() -> SavedNpcObject {
    let tile = Coords16 { x: 16, y: 8 };
    SavedNpcObject {
        local_id: 1,
        movement_type: LOOK_AROUND,
        facing_direction: WEST,
        current_elevation: 0,
        previous_elevation: 4,
        initial: tile,
        current: tile,
        previous: tile,
    }
}

fn snapshot(objects: Vec<SavedNpcObject>) -> SavedNpcObjects {
    SavedNpcObjects::new(ROUTE_101.0, ROUTE_101.1, objects).unwrap()
}

fn patched(snapshot: &SavedNpcObjects) -> Vec<u8> {
    let mut bytes = vec![0u8; SaveBlock1::PAYLOAD_LEN];
    SavedNpcObjects::patch(Some(snapshot), &mut bytes);
    bytes
}

#[test]
fn records_sit_at_the_upstream_offsets_after_the_player_slot() {
    // `objectEvents` is at 0xA30 (`include/global.h:1018`), each `ObjectEvent` 0x24 bytes.
    assert_eq!(record(0), 0xA54);
    assert_eq!(record(NPC_OBJECT_CAPACITY - 1), 0xC4C);
    let bytes = patched(&snapshot(vec![youngster()]));
    let at = 0xA54;
    assert_eq!(bytes[at], 0x01, "active");
    assert_eq!(bytes[at + 0x06], LOOK_AROUND);
    assert_eq!(bytes[at + 0x08], 1, "localId");
    assert_eq!(bytes[at + 0x09], 16, "mapNum");
    assert_eq!(bytes[at + 0x0A], 0, "mapGroup");
    assert_eq!(
        bytes[at + 0x0B],
        0x40,
        "previousElevation:4 is the high nibble"
    );
    // Coordinates carry MAP_OFFSET (7), little-endian.
    assert_eq!(bytes[at + 0x0C..at + 0x10], [23, 0, 15, 0], "initialCoords");
    assert_eq!(bytes[at + 0x10..at + 0x14], [23, 0, 15, 0], "currentCoords");
    assert_eq!(
        bytes[at + 0x14..at + 0x18],
        [23, 0, 15, 0],
        "previousCoords"
    );
    assert_eq!(
        bytes[at + 0x18],
        0x03,
        "facingDirection:4 is the low nibble"
    );
}

#[test]
fn a_snapshot_round_trips_through_the_bytes() {
    let mut walked = youngster();
    walked.facing_direction = 4;
    walked.previous = Coords16 { x: 16, y: 7 };
    walked.current = Coords16 { x: -3, y: 9 };
    let saved = snapshot(vec![youngster(), walked]);
    let bytes = patched(&saved);
    assert_eq!(SavedNpcObjects::from_bytes(&bytes), Some(saved));
}

#[test]
fn an_empty_snapshot_is_recognized_and_keeps_its_map() {
    let saved = snapshot(Vec::new());
    let bytes = patched(&saved);
    let decoded = SavedNpcObjects::from_bytes(&bytes).expect("marked, though no object is active");
    assert!(decoded.objects().is_empty());
    assert_eq!((decoded.map_group(), decoded.map_num()), ROUTE_101);
}

#[test]
fn bytes_without_the_marker_are_not_a_snapshot() {
    assert_eq!(
        SavedNpcObjects::from_bytes(&vec![0; SaveBlock1::PAYLOAD_LEN]),
        None
    );
    // An upstream image: live-looking records, never marked.
    let mut bytes = patched(&snapshot(vec![youngster()]));
    bytes[record(0) + MARKER_OFFSET] &= !OBJECT_EVENT_MARKER_BIT;
    assert_eq!(SavedNpcObjects::from_bytes(&bytes), None);
}

#[test]
fn patching_leaves_unmodeled_bits_and_neighbours_alone() {
    let mut bytes = vec![0xFFu8; SaveBlock1::PAYLOAD_LEN];
    SavedNpcObjects::patch(Some(&snapshot(vec![youngster()])), &mut bytes);
    let at = record(0);
    assert_eq!(bytes[at + 0x18], 0xF3, "movementDirection nibble kept");
    assert_eq!(
        bytes[at + 0x03],
        0xFF,
        "marker bit already set, padding kept"
    );
    assert_eq!(
        bytes[at + 0x02],
        0xFE,
        "isPlayer cleared, sibling bits kept"
    );
    assert_eq!(bytes[at + 0x01], 0xFF);
    assert_eq!(bytes[at + 0x04], 0xFF, "spriteId");
    assert_eq!(bytes[at + 0x1A..at + 0x23], [0xFF; 9]);
    assert_eq!(
        bytes[record(1)],
        0xFE,
        "an unused slot only loses its active bit"
    );
    assert_eq!(bytes[record(1) + 0x08], 0xFF);
    assert_eq!(bytes[0xA30..record(0)], [0xFF; 0x24], "the player slot");
    assert_eq!(bytes[0xC70], 0xFF, "objectEventTemplates");
    assert_eq!(
        bytes[record(0) + 0x23],
        0xFF,
        "the stamp is the save store's"
    );
}

#[test]
fn fifteen_objects_fill_the_array_and_a_sixteenth_is_refused() {
    let full = snapshot(vec![youngster(); NPC_OBJECT_CAPACITY]);
    let bytes = patched(&full);
    assert_eq!(bytes[record(NPC_OBJECT_CAPACITY - 1)], 0x01);
    assert_eq!(bytes[0xC70..0xC70 + 0x24], [0; 0x24], "templates untouched");
    assert_eq!(SavedNpcObjects::from_bytes(&bytes), Some(full));
    assert_eq!(
        SavedNpcObjects::new(0, 16, vec![youngster(); NPC_OBJECT_CAPACITY + 1]),
        None
    );
}

#[test]
fn withdrawing_a_snapshot_clears_its_marker() {
    let mut bytes = patched(&snapshot(vec![youngster()]));
    SavedNpcObjects::patch(None, &mut bytes);
    assert_eq!(SavedNpcObjects::from_bytes(&bytes), None);

    // Only the marker bit goes: its seven sibling bits and every other byte stay as loaded.
    let mut ones = vec![0xFFu8; SaveBlock1::PAYLOAD_LEN];
    SavedNpcObjects::patch(None, &mut ones);
    assert_eq!(ones[record(0) + MARKER_OFFSET], 0x7F);
    assert!(ones
        .iter()
        .enumerate()
        .all(|(index, &byte)| byte == 0xFF || index == record(0) + MARKER_OFFSET));
}

#[test]
fn a_record_of_another_map_or_the_player_is_not_decoded() {
    let mut bytes = patched(&snapshot(vec![youngster(), youngster(), youngster()]));
    bytes[record(1) + 0x0A] = 3;
    bytes[record(2) + 0x02] |= OBJECT_EVENT_IS_PLAYER_BIT;
    let decoded = SavedNpcObjects::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.objects(), [youngster()]);
}

#[test]
fn the_stamp_is_checked_against_the_slot_counter() {
    let mut block = SaveBlock1::default();
    block.npc_objects = Some(snapshot(vec![youngster()]));
    let mut bytes = block.to_bytes(0);
    block.stamp_generation(&mut bytes, 0x0102_0304);
    assert_eq!(bytes[record(0) + 0x23], 0x04);
    assert_eq!(bytes[record(3) + 0x23], 0x01);

    let mut loaded = SaveBlock1::from_bytes(&bytes, 0).unwrap();
    loaded.require_generation(&bytes, 0x0102_0304);
    assert_eq!(loaded.npc_objects, block.npc_objects);

    // A later write by a build that knows nothing of the snapshot leaves it, unstamped.
    let mut stale = SaveBlock1::from_bytes(&bytes, 0).unwrap();
    stale.require_generation(&bytes, 0x0102_0305);
    assert_eq!(stale.npc_objects, None);
}

#[test]
fn the_block_round_trips_the_snapshot_and_a_legacy_block_has_none() {
    let mut block = SaveBlock1::default();
    assert!(SaveBlock1::from_bytes(&block.to_bytes(0), 0)
        .unwrap()
        .npc_objects
        .is_none());
    block.npc_objects = Some(snapshot(vec![youngster()]));
    let decoded = SaveBlock1::from_bytes(&block.to_bytes(0), 0).unwrap();
    assert_eq!(decoded.npc_objects, block.npc_objects);
}
