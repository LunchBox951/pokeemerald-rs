//! Key-split slots: a table of per-note child indices over a child voicegroup.

use assets::{KeySplitVoice, VoiceEntry, VoiceGroupId};

use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::roots::{AudioRoots, KeysplitRoot};

use super::VOICE_NOTE_COUNT;

/// A key-split slot: the table pointer sits where a leaf keeps its
/// envelope, and the table itself is read from the note the profile says
/// it starts at.
pub(super) fn key_split(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &'static str,
    index: usize,
    base: usize,
    children: VoiceGroupId,
) -> Result<VoiceEntry, ImportError> {
    let table = key_split_root(audio, root, index, reader.ptr(base + 8)?)?;
    let (starting_note, entries) = key_split_entries(reader, audio, table)?;
    let voice = KeySplitVoice::new(starting_note, entries, children)
        .map_err(|source| ImportError::Audio { id: root, source })?;
    Ok(VoiceEntry::KeySplit(voice))
}

/// The profile's key-split table whose label sits at `ptr`.
pub(super) fn key_split_root<'a>(
    audio: &'a AudioRoots,
    root: &'static str,
    index: usize,
    ptr: GbaPtr,
) -> Result<&'a KeysplitRoot, ImportError> {
    audio
        .keysplits
        .iter()
        .find(|table| table.addr == ptr)
        .ok_or(ImportError::UnresolvedPointer {
            root,
            slot: index,
            what: "a key-split table",
            ptr,
        })
}

/// The first note and per-note child indices of `table`, widened to every
/// note whose byte is a recorded key-split table's declared byte.
///
/// The label sits `starting_note` bytes before the declared data, so note `n`
/// reads `addr[n]` whatever the table declares: a note below the declared
/// range reads the preceding table's final bytes, which is how the title's
/// key 33 on the strings split reaches child 3
/// (`pokeemerald/asm/macros/m4a.inc:25-32`,
/// `pokeemerald/sound/keysplit_tables.inc:13-22`).
pub(super) fn key_split_entries(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    table: &KeysplitRoot,
) -> Result<(u8, Vec<u8>), ImportError> {
    let is_known = |note: usize| {
        let offset = table.addr.offset().saturating_add(note);
        audio.keysplits.iter().any(|candidate| {
            let start = candidate.addr.offset() + usize::from(candidate.starting_note);
            (start..start + usize::from(candidate.len)).contains(&offset)
        })
    };
    let declared_first = usize::from(table.starting_note);
    let declared_end = declared_first + usize::from(table.len);
    let mut first = declared_first;
    while first > 0 && is_known(first - 1) {
        first -= 1;
    }
    let mut end = declared_end;
    while end < VOICE_NOTE_COUNT && is_known(end) {
        end += 1;
    }
    let entries = reader
        .slice_at(table.addr.offset() + first, end - first)?
        .to_vec();
    let first = u8::try_from(first).expect("a key-split note is below 128");
    Ok((first, entries))
}
