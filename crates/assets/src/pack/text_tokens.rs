//! Token-stream validation shared by the raw text schemas.
//!
//! A stream is one charmap token sequence ending in `EOS` (`0xFF`), as
//! upstream's `_()` strings assemble. Validation walks token boundaries so an
//! `0xFF` control argument is not mistaken for the terminator.

const EOS: u8 = 0xFF;

/// Argument bytes following an extended-control (`0xFC`) subcode, per the
/// charmap (`charmap.txt:413-436`); `None` for an unassigned subcode.
const fn ext_ctrl_args(sub: u8) -> Option<usize> {
    match sub {
        0x00 | 0x07 | 0x09 | 0x0A | 0x0F | 0x15..=0x18 => Some(0),
        0x01..=0x03 | 0x05 | 0x06 | 0x08 | 0x0C..=0x0E | 0x11..=0x14 => Some(1),
        0x0B | 0x10 => Some(2),
        0x04 => Some(3),
        _ => None,
    }
}

/// Walks token boundaries so an `0xFF` control argument is not mistaken for `EOS`.
pub(super) fn validate_string(stream: &[u8]) -> Result<(), &'static str> {
    // `encode` writes each length as a u32, so a longer stream could not round-trip.
    if u32::try_from(stream.len()).is_err() {
        return Err("stream is longer than the wire format can store");
    }
    let mut i = 0;
    while let Some(&byte) = stream.get(i) {
        i += match byte {
            EOS => {
                return if i + 1 == stream.len() {
                    Ok(())
                } else {
                    Err("bytes follow the terminator")
                };
            }
            // Placeholder, dynamic, keypad-icon, and extra-symbol leads take one byte.
            0xFD | 0xF7 | 0xF8 | 0xF9 => {
                if i + 1 >= stream.len() {
                    return Err("incomplete control sequence");
                }
                2
            }
            0xFC => {
                let sub = *stream.get(i + 1).ok_or("incomplete control sequence")?;
                let args = ext_ctrl_args(sub).ok_or("unknown extended control")?;
                if i + 2 + args > stream.len() {
                    return Err("incomplete control sequence");
                }
                2 + args
            }
            _ => 1,
        };
    }
    Err("missing terminator")
}
