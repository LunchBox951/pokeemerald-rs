use super::{read_header, split_tracks, MidiReader};
use crate::extract::midi::error::MidiError;

fn mthd(format: u16, track_count: u16, division: u16) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(format.to_be_bytes());
    out.extend(track_count.to_be_bytes());
    out.extend(division.to_be_bytes());
    out
}

fn mtrk(body: &[u8]) -> Vec<u8> {
    let mut out = b"MTrk".to_vec();
    let len = u32::try_from(body.len()).expect("test track bodies are tiny");
    out.extend(len.to_be_bytes());
    out.extend_from_slice(body);
    out
}

#[test]
fn vlq_round_trips_multi_byte_values() {
    // 0x80 0x00 encodes 128 (7-bit groups, top bit = continuation).
    let mut r = MidiReader::new(&[0x81, 0x00]);
    assert_eq!(r.vlq().unwrap(), 128);
}

#[test]
fn vlq_reads_a_single_byte_value() {
    let mut r = MidiReader::new(&[0x40]);
    assert_eq!(r.vlq().unwrap(), 0x40);
}

/// The largest quantity a standard MIDI file can legally encode: four
/// bytes, 28 bits, all ones.
#[test]
fn vlq_reads_the_largest_legal_four_byte_value() {
    let mut r = MidiReader::new(&[0xFF, 0xFF, 0xFF, 0x7F]);
    assert_eq!(r.vlq().unwrap(), 0x0FFF_FFFF);
}

/// Pins `MidiReader::vlq`'s documented decision to reproduce
/// `tools/mid2agb/midi.cpp:124`'s `val <<= 7` rather than reject an
/// over-long quantity. Five 7-bit groups is 35 bits; the top three fall off
/// a `u32` exactly as they do in upstream's C++, leaving `0x7F` shifted up
/// by 28 and truncated, i.e. `0xF000_0000`. Neither side errors, and
/// neither side panics — a debug-build `<<` traps only on an out-of-range
/// *shift amount*, never on discarded bits.
#[test]
fn over_long_vlq_wraps_like_upstreams_shift() {
    let mut r = MidiReader::new(&[0xFF, 0x80, 0x80, 0x80, 0x00]);
    assert_eq!(r.vlq().unwrap(), 0xF000_0000);
}

/// A quantity whose continuation bit never clears before the chunk ends is
/// the reader's *only* VLQ failure mode (`MidiReader::vlq`'s docs).
#[test]
fn unterminated_vlq_runs_out_of_bytes() {
    let mut r = MidiReader::new(&[0x80, 0x80, 0x80]);
    assert_eq!(r.vlq().unwrap_err(), MidiError::Truncated);
}

#[test]
fn header_reports_format_track_count_and_division() {
    let bytes = mthd(1, 11, 24);
    let header = read_header(&bytes).unwrap();
    assert_eq!(header.track_count, 11);
    assert_eq!(header.division, 24);
}

#[test]
fn bad_magic_is_rejected() {
    let mut bytes = mthd(0, 1, 24);
    bytes[0] = b'X';
    assert_eq!(read_header(&bytes).unwrap_err(), MidiError::BadHeaderMagic);
}

#[test]
fn format_2_is_rejected() {
    let bytes = mthd(2, 1, 24);
    assert_eq!(
        read_header(&bytes).unwrap_err(),
        MidiError::UnsupportedFormat(2)
    );
}

#[test]
fn negative_division_is_rejected() {
    let bytes = mthd(0, 1, 0x8000);
    assert_eq!(
        read_header(&bytes).unwrap_err(),
        MidiError::NegativeDivision(-32768)
    );
}

#[test]
fn split_tracks_returns_each_chunk_body_in_order() {
    let mut file = mthd(1, 2, 24);
    file.extend(mtrk(&[0x00, 0xFF, 0x2F, 0x00]));
    file.extend(mtrk(&[0x01, 0x90, 60, 100]));
    let tracks = split_tracks(&file, 2).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0], &[0x00, 0xFF, 0x2F, 0x00]);
    assert_eq!(tracks[1], &[0x01, 0x90, 60, 100]);
}

#[test]
fn zero_tracks_is_rejected() {
    let file = mthd(0, 0, 24);
    assert_eq!(split_tracks(&file, 0).unwrap_err(), MidiError::NoTracks);
}

#[test]
fn bad_track_magic_is_rejected() {
    let mut file = mthd(0, 1, 24);
    file.extend(b"XTRK");
    file.extend(4u32.to_be_bytes());
    assert_eq!(
        split_tracks(&file, 1).unwrap_err(),
        MidiError::BadTrackMagic
    );
}

#[test]
fn truncated_track_body_is_rejected() {
    let mut file = mthd(0, 1, 24);
    file.extend(b"MTrk");
    file.extend(100u32.to_be_bytes()); // declares far more than is present
    assert_eq!(split_tracks(&file, 1).unwrap_err(), MidiError::Truncated);
}
