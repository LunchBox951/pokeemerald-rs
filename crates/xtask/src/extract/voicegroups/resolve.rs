//! Links parsed voicegroups together: resolves every `voice_keysplit`/
//! `voice_keysplit_all` reference a top-level group carries into another
//! [`ResolvedVoiceGroup`] (cycle-safe, and rejecting a second level of
//! indirection -- see [`resolve_voice_groups`]), derives stable
//! [`crate::extract::pack`] ids for the samples and child groups each slot
//! references, and normalizes every group to exactly
//! [`super::VOICE_SLOT_COUNT`] (128) slots (see [`pad_to_128`]).
//!
//! # Sample id scheme
//!
//! A [`super::parser::RawSlot::DirectSound`]/[`super::parser::RawSlot::ProgrammableWave`]
//! slot names its sample by the upstream linker symbol
//! (`DirectSoundWaveData_sc88pro_flute`, `ProgrammableWaveData_1`) --
//! neither payload is extracted here (that's `#183`'s job), only a stable
//! id a future sample-extraction pass is expected to produce:
//!
//! - `DirectSoundWaveData_<name>` -> `audio/sample/<name>` (the symbol's own
//!   suffix, already a stable, `snake_case` name -- e.g.
//!   `DirectSoundWaveData_sc88pro_flute` -> `audio/sample/sc88pro_flute`).
//! - `ProgrammableWaveData_<n>` -> `audio/sample/programmable_wave_<n>`
//!   (matches the naming `crates/assets/src/audio/voicegroup.rs`'s own
//!   test fixtures already use, e.g. `audio/sample/programmable_wave_2`).
//!
//! # Voicegroup id scheme
//!
//! Every resolved group -- the top-level group a song's `VOICE` command
//! selects, and every key-split/rhythm child transitively reachable from
//! it -- gets its own pack entry id `audio/voicegroup/<label>` (`<label>`
//! is the upstream `voice_group`/`voicegroup_*` label with no further
//! transformation -- already a stable, `snake_case` name). Matches the
//! scheme `crates/assets/src/audio/voicegroup.rs`'s own tests already use
//! (e.g. `audio/voicegroup/trumpet_keysplit`).

use std::collections::HashMap;

use super::parser::{
    DirectSoundMode, Envelope, RawKeySplitTable, RawSlot, RawVoiceGroup, VoiceGroupError,
};
use super::VOICE_SLOT_COUNT;

const DIRECT_SOUND_SAMPLE_PREFIX: &str = "DirectSoundWaveData_";
const PROGRAMMABLE_WAVE_SAMPLE_PREFIX: &str = "ProgrammableWaveData_";

/// One fully-resolved voicegroup slot: every reference (sample, child
/// group) has been turned into a stable pack id, and the shape otherwise
/// mirrors `crates/assets/src/audio/voicegroup.rs`'s `VoiceEntry` exactly
/// (this crate never depends on `crates/assets` -- see `crate::extract::pack`'s
/// module docs on why the two crates stay decoupled -- so the shape is
/// duplicated rather than shared, the same way the pack container format
/// itself already is).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VoiceSlot {
    DirectSound {
        base_key: u8,
        pan: Option<u8>,
        sample_id: String,
        envelope: Envelope,
        mode: DirectSoundMode,
    },
    Square1 {
        base_key: u8,
        length: u8,
        sweep: u8,
        duty: u8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    Square2 {
        base_key: u8,
        length: u8,
        duty: u8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    ProgrammableWave {
        base_key: u8,
        length: u8,
        wave_id: String,
        envelope: Envelope,
        fixed_rate: bool,
    },
    Noise {
        base_key: u8,
        length: u8,
        period: u8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    KeySplit {
        starting_note: u8,
        table: Vec<u8>,
        children_id: String,
    },
    Rhythm {
        children_id: String,
    },
    /// Positionally-preserved unused slot -- either a genuine trailing pad
    /// up to [`VOICE_SLOT_COUNT`], or a `starting_note` bias's leading gap
    /// (see [`pad_to_128`]). Mirrors `VoiceEntry::Empty`'s own rationale:
    /// a slot's index is its meaning, so it must keep its place.
    Empty,
}

/// One fully-resolved voicegroup: always exactly [`VOICE_SLOT_COUNT`]
/// slots (see [`pad_to_128`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedVoiceGroup {
    pub label: String,
    pub slots: Vec<VoiceSlot>,
}

/// The stable pack id a [`super::parser::RawSlot::DirectSound`] sample
/// symbol maps to. See the module docs' "Sample id scheme".
fn direct_sound_sample_id(symbol: &str, group: &str) -> Result<String, VoiceGroupError> {
    symbol
        .strip_prefix(DIRECT_SOUND_SAMPLE_PREFIX)
        .filter(|name| !name.is_empty())
        .map(|name| format!("audio/sample/{name}"))
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: DIRECT_SOUND_SAMPLE_PREFIX,
        })
}

/// The stable pack id a [`super::parser::RawSlot::ProgrammableWave`] wave
/// symbol maps to. See the module docs' "Sample id scheme".
fn programmable_wave_sample_id(symbol: &str, group: &str) -> Result<String, VoiceGroupError> {
    symbol
        .strip_prefix(PROGRAMMABLE_WAVE_SAMPLE_PREFIX)
        .filter(|name| !name.is_empty())
        .map(|name| format!("audio/sample/programmable_wave_{name}"))
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: PROGRAMMABLE_WAVE_SAMPLE_PREFIX,
        })
}

/// The stable pack id a resolved voicegroup is emitted under. See the
/// module docs' "Voicegroup id scheme".
pub(super) fn voice_group_pack_id(label: &str) -> String {
    format!("audio/voicegroup/{label}")
}

/// Pad `slots` out to exactly [`VOICE_SLOT_COUNT`]: `starting_note`
/// [`VoiceSlot::Empty`] entries first (the drumsets' bias -- see
/// `pokeemerald/sound/voicegroups/drumsets/rs.inc`, where the declared
/// entries start at real MIDI note 36, not 0), then the parsed entries,
/// then trailing [`VoiceSlot::Empty`] entries up to the full 128 --
/// including the "effective slot 127" every voicegroup this pipeline emits
/// must explicitly represent, whether or not the source `.inc` file
/// happened to declare that many entries (issue #182's own "128-slot
/// normalization").
fn pad_to_128(
    group: &str,
    starting_note: u8,
    mut slots: Vec<VoiceSlot>,
) -> Result<Vec<VoiceSlot>, VoiceGroupError> {
    let leading = usize::from(starting_note);
    let total = leading
        .checked_add(slots.len())
        .filter(|&total| total <= VOICE_SLOT_COUNT);
    if total.is_none() {
        return Err(VoiceGroupError::TooManySlots {
            group: group.to_owned(),
            starting_note,
            slot_count: slots.len(),
        });
    }
    let mut out = Vec::with_capacity(VOICE_SLOT_COUNT);
    out.extend(std::iter::repeat_n(VoiceSlot::Empty, leading));
    out.append(&mut slots);
    out.resize_with(VOICE_SLOT_COUNT, || VoiceSlot::Empty);
    Ok(out)
}

/// Resolve `top_label` and every key-split/rhythm child it transitively
/// references, returning one [`ResolvedVoiceGroup`] per distinct label
/// reached (`top_label` itself included).
///
/// `raw_groups` and `keysplit_tables` are the already-parsed contents of
/// every `.inc` file under `sound/voicegroups/` and of
/// `keysplit_tables.inc` respectively (see `super`'s `build_label_index`) --
/// this function does no filesystem access of its own.
///
/// # Errors
///
/// [`VoiceGroupError::DanglingVoiceGroupReference`] /
/// [`VoiceGroupError::DanglingKeySplitTableReference`] if a reference names
/// a label with no matching source; [`VoiceGroupError::Cycle`] if
/// resolving a reference revisits a group already being resolved (checked
/// before any recursive call, so this is reachable regardless of slot
/// order or reference depth); [`VoiceGroupError::NestedIndirection`] if a
/// group referenced *as* a key-split/rhythm child itself carries a
/// key-split/rhythm slot (upstream's single-level indirection limit -- see
/// [`VoiceSlot`]'s module docs); [`VoiceGroupError::TooManySlots`] /
/// [`VoiceGroupError::KeySplitTableTooLong`] if a group or key-split table
/// would exceed [`VOICE_SLOT_COUNT`]; any [`VoiceGroupError`] variant
/// [`direct_sound_sample_id`]/[`programmable_wave_sample_id`] can return
/// for a malformed sample symbol.
pub(super) fn resolve_voice_groups(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    let mut resolving: Vec<String> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut resolved: HashMap<String, ResolvedVoiceGroup> = HashMap::new();
    resolve_one(
        top_label,
        raw_groups,
        keysplit_tables,
        &mut resolving,
        &mut order,
        &mut resolved,
    )?;
    Ok(order
        .into_iter()
        .map(|label| {
            resolved
                .remove(&label)
                .expect("every label pushed to `order` was inserted into `resolved` first")
        })
        .collect())
}

/// Convert one leaf (non-indirection) slot, deriving its sample id(s).
///
/// # Panics
///
/// Never: callers only pass the five leaf [`RawSlot`] variants (the two
/// indirection variants, `KeySplit`/`Rhythm`, are handled by
/// [`resolve_indirection_slot`] before this function is ever reached).
fn convert_leaf_slot(raw_slot: &RawSlot, group_label: &str) -> Result<VoiceSlot, VoiceGroupError> {
    Ok(match raw_slot {
        RawSlot::DirectSound {
            base_key,
            pan,
            sample_symbol,
            envelope,
            mode,
        } => VoiceSlot::DirectSound {
            base_key: *base_key,
            pan: *pan,
            sample_id: direct_sound_sample_id(sample_symbol, group_label)?,
            envelope: *envelope,
            mode: *mode,
        },
        RawSlot::Square1 {
            base_key,
            length,
            sweep,
            duty,
            envelope,
            fixed_rate,
        } => VoiceSlot::Square1 {
            base_key: *base_key,
            length: *length,
            sweep: *sweep,
            duty: *duty,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::Square2 {
            base_key,
            length,
            duty,
            envelope,
            fixed_rate,
        } => VoiceSlot::Square2 {
            base_key: *base_key,
            length: *length,
            duty: *duty,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::ProgrammableWave {
            base_key,
            length,
            wave_symbol,
            envelope,
            fixed_rate,
        } => VoiceSlot::ProgrammableWave {
            base_key: *base_key,
            length: *length,
            wave_id: programmable_wave_sample_id(wave_symbol, group_label)?,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::Noise {
            base_key,
            length,
            period,
            envelope,
            fixed_rate,
        } => VoiceSlot::Noise {
            base_key: *base_key,
            length: *length,
            period: *period,
            envelope: *envelope,
            fixed_rate: *fixed_rate,
        },
        RawSlot::KeySplit { .. } | RawSlot::Rhythm { .. } => {
            unreachable!("resolve_one dispatches indirection slots to resolve_indirection_slot")
        }
    })
}

/// Resolve one `KeySplit`/`Rhythm` slot: reject it outright if this group is
/// itself an indirection target (upstream's single-level limit -- see
/// [`VoiceSlot`]'s module docs), then recursively resolve (and emit) the
/// referenced child group, returning the linked [`VoiceSlot`].
/// `table_label` is `Some` for a key-split slot, `None` for a rhythm slot.
#[allow(clippy::too_many_arguments)]
fn resolve_indirection_slot(
    parent_label: &str,
    is_indirection_target: bool,
    child_label: &str,
    table_label: Option<&str>,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    resolving: &mut Vec<String>,
    order: &mut Vec<String>,
    resolved: &mut HashMap<String, ResolvedVoiceGroup>,
) -> Result<VoiceSlot, VoiceGroupError> {
    if is_indirection_target {
        return Err(VoiceGroupError::NestedIndirection {
            parent: parent_label.to_owned(),
            child: child_label.to_owned(),
        });
    }
    let table = table_label
        .map(|table_label| {
            keysplit_tables.get(table_label).ok_or_else(|| {
                VoiceGroupError::DanglingKeySplitTableReference {
                    referrer: parent_label.to_owned(),
                    target: table_label.to_owned(),
                }
            })
        })
        .transpose()?;

    resolve_one(
        child_label,
        raw_groups,
        keysplit_tables,
        resolving,
        order,
        resolved,
    )?;
    let children_id = voice_group_pack_id(child_label);
    Ok(match table {
        Some(table) => VoiceSlot::KeySplit {
            starting_note: table.starting_note,
            table: table.table.clone(),
            children_id,
        },
        None => VoiceSlot::Rhythm { children_id },
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    resolving: &mut Vec<String>,
    order: &mut Vec<String>,
    resolved: &mut HashMap<String, ResolvedVoiceGroup>,
) -> Result<(), VoiceGroupError> {
    if resolved.contains_key(label) {
        return Ok(());
    }
    if resolving.iter().any(|seen| seen == label) {
        let mut path = resolving.clone();
        path.push(label.to_owned());
        return Err(VoiceGroupError::Cycle(path));
    }
    let raw =
        raw_groups
            .get(label)
            .ok_or_else(|| VoiceGroupError::DanglingVoiceGroupReference {
                referrer: resolving
                    .last()
                    .cloned()
                    .unwrap_or_else(|| label.to_owned()),
                target: label.to_owned(),
            })?;

    resolving.push(label.to_owned());
    // Depth 1 (just this push) is the top-level group a song references
    // directly -- it may carry key-split/rhythm slots. Any deeper level is
    // a group already reached *as* a key-split/rhythm child, which upstream's
    // `ply_note` never recurses through (see `VoiceSlot`'s module docs).
    let is_indirection_target = resolving.len() > 1;

    let mut slots = Vec::with_capacity(raw.slots.len());
    for raw_slot in &raw.slots {
        let resolved_slot = match raw_slot {
            RawSlot::KeySplit {
                child_label,
                table_label,
            } => resolve_indirection_slot(
                &raw.label,
                is_indirection_target,
                child_label,
                Some(table_label),
                raw_groups,
                keysplit_tables,
                resolving,
                order,
                resolved,
            )?,
            RawSlot::Rhythm { child_label } => resolve_indirection_slot(
                &raw.label,
                is_indirection_target,
                child_label,
                None,
                raw_groups,
                keysplit_tables,
                resolving,
                order,
                resolved,
            )?,
            leaf => convert_leaf_slot(leaf, &raw.label)?,
        };
        slots.push(resolved_slot);
    }

    let padded = pad_to_128(&raw.label, raw.starting_note, slots)?;
    resolving.pop();
    order.push(raw.label.clone());
    resolved.insert(
        raw.label.clone(),
        ResolvedVoiceGroup {
            label: raw.label.clone(),
            slots: padded,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests;
