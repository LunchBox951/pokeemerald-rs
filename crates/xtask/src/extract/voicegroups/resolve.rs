//! Links parsed voicegroups together: resolves every `voice_keysplit`/
//! `voice_keysplit_all` reference a top-level group carries into another
//! [`ResolvedVoiceGroup`] (cycle-safe, and rejecting a second level of
//! indirection -- see [`resolve_voice_groups_with_link_successors`]), derives stable
//! [`pack_format`] ids for the samples and child groups each slot
//! references, and normalizes every group to exactly
//! [`super::VOICE_SLOT_COUNT`] (128) slots (see [`pad_to_128`]).
//!
//! # Sample id scheme
//!
//! A [`super::parser::RawSlot::DirectSound`]/[`super::parser::RawSlot::ProgrammableWave`]
//! slot names its sample by the upstream linker symbol
//! (`DirectSoundWaveData_sc88pro_flute`, `ProgrammableWaveData_1`) --
//! neither payload is extracted here (that's `#183`'s job), only the id
//! `#183`'s own extraction pass emits for that payload. The two schemes
//! must agree exactly or every reference here dangles, so this module
//! mirrors `xtask::extract::audio_samples`'s ids verbatim:
//!
//! - `DirectSoundWaveData_<name>` -> `audio/sample/direct-sound/<name>`
//!   (the symbol's own suffix, already a stable, `snake_case` name that
//!   matches the `sound/direct_sound_samples/<name>.wav` source file --
//!   e.g. `DirectSoundWaveData_sc88pro_flute` ->
//!   `audio/sample/direct-sound/sc88pro_flute`).
//! - `ProgrammableWaveData_<n>` -> `audio/sample/programmable-wave/<nn>`,
//!   where `<nn>` is `<n>` re-formatted zero-padded to two digits, matching
//!   the `sound/programmable_wave_samples/<nn>.pcm` source file `#183`
//!   reads (e.g. `ProgrammableWaveData_1` ->
//!   `audio/sample/programmable-wave/01`). A suffix that is not a number
//!   fails closed as
//!   [`super::parser::VoiceGroupError::MalformedProgrammableWaveIndex`]
//!   rather than being pasted verbatim into an id no sample entry will
//!   ever carry.
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
//!
//! # Link adjacency (issue #201)
//!
//! A `.inc` source under `sound/voicegroups/` need not declare all 128
//! addressable slots -- `title.inc` declares only 89. Upstream's mixer
//! (`ply_voice`, `src/m4a_1.s`) fetches `voicegroup + voice * 12` with no
//! bounds check at all, so a song that selects an undeclared slot (e.g.
//! `mus_title.mid` selecting 127 on one channel -- see `super`'s module
//! docs) does not read silence: it reads whatever bytes the assembler
//! happened to place right after `voicegroup_title`'s own table.
//! `sound/voice_groups.inc:66-67` links `intro.inc` immediately after
//! `title.inc` (89 entries = 1068 bytes, already 4-aligned, so no `.align 2`
//! padding intervenes -- see `asm/macros/m4a.inc`'s `voice_group` macro),
//! and `intro.inc` alone declares all 128 of its own slots, so slot 127
//! resolves to `voicegroup_intro`'s entry 38: a real, playable
//! `voice_square_1 60, 0, 0, 2, 0, 0, 15, 0`.
//!
//! [`resolve_one`] models this for the single top-level group a song
//! references directly (never for a key-split/rhythm child -- see its
//! `is_indirection_target` gate): [`collect_link_adjacency_overflow`] pulls
//! the group's undeclared tail from `super::link_order_successors`'s own
//! answer for "what does `sound/voice_groups.inc` link right after this
//! file", running each pulled entry through the exact same conversion
//! [`resolve_one`]'s own declared-slot loop uses (so a borrowed
//! key-split/rhythm entry resolves and, if new, emits its child exactly as
//! a declared one would). If the top-level group is last in that linked
//! order, or its successors run out before the tail is filled, the
//! shortfall stays [`VoiceSlot::Empty`] -- this pipeline has no way to know
//! what bytes, if any, genuinely follow in the real linked binary, so
//! silence is the fail-closed choice there, not a guess.
//!
//! A key-split/rhythm child's *own* under/over-range access (e.g. a
//! rhythm's raw played key falling outside its child group's declared
//! entries, or a key-split table index past its target's declared count) is
//! a different mechanism -- the keysplit table itself, not file-adjacency --
//! and stays out of scope here: those slots keep the plain
//! [`pad_to_128`] `Empty` padding they always had.

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
/// (this crate never depends on `crates/assets` -- see `pack_format`'s
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
        .map(|name| format!("audio/sample/direct-sound/{name}"))
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: DIRECT_SOUND_SAMPLE_PREFIX,
        })
}

/// The stable pack id a [`super::parser::RawSlot::ProgrammableWave`] wave
/// symbol maps to. See the module docs' "Sample id scheme".
fn programmable_wave_sample_id(symbol: &str, group: &str) -> Result<String, VoiceGroupError> {
    let suffix = symbol
        .strip_prefix(PROGRAMMABLE_WAVE_SAMPLE_PREFIX)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: PROGRAMMABLE_WAVE_SAMPLE_PREFIX,
        })?;
    // The suffix is a sample *number*, not a name: `#183` writes the entry
    // under the zero-padded two-digit form its `<nn>.pcm` source file uses,
    // so re-format rather than pasting the symbol's own spelling through.
    let index: u32 =
        suffix
            .parse()
            .map_err(|_| VoiceGroupError::MalformedProgrammableWaveIndex {
                group: group.to_owned(),
                reference: symbol.to_owned(),
            })?;
    Ok(format!("audio/sample/programmable-wave/{index:02}"))
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
/// No link-adjacency modeling: `top_label`'s tail is padded straight to
/// [`VOICE_SLOT_COUNT`] with [`VoiceSlot::Empty`], the same as any other
/// group. Callers that want issue #201's modeled overflow read (see
/// [`resolve_voice_groups_with_link_successors`]) must go through that
/// function instead; this one stays the plain entry point every synthetic
/// fixture in this module's tests already uses.
///
/// # Errors
///
/// See [`resolve_voice_groups_with_link_successors`].
#[cfg(test)]
pub(super) fn resolve_voice_groups(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    resolve_voice_groups_with_link_successors(top_label, raw_groups, keysplit_tables, &[])
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
/// `link_successors` is `super::link_order_successors`'s own output: the
/// labels `sound/voice_groups.inc` links immediately after `top_label`'s own
/// file, in order (empty if `top_label` is last in that order, or isn't
/// linked at all). Only `top_label` itself -- never a key-split/rhythm
/// child reached through it -- has its undeclared tail materialized from
/// these successors instead of left `Empty`; see [`resolve_one`]'s
/// `is_indirection_target` gate and the module docs' "Link adjacency"
/// section.
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
pub(super) fn resolve_voice_groups_with_link_successors(
    top_label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    link_successors: &[String],
) -> Result<Vec<ResolvedVoiceGroup>, VoiceGroupError> {
    let mut resolving: Vec<String> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut resolved: HashMap<String, ResolvedVoiceGroup> = HashMap::new();
    resolve_one(
        top_label,
        raw_groups,
        keysplit_tables,
        link_successors,
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
    link_successors: &[String],
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
        link_successors,
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

/// Materializes the tail of the single top-level group a song references
/// directly (`resolve_one`'s `!is_indirection_target` branch only) from the
/// linker's own contiguous concatenation, instead of leaving it
/// [`VoiceSlot::Empty`] -- issue #201's modeled link-adjacency read. See the
/// module docs' "Link adjacency" section for the upstream mechanism this
/// mirrors.
///
/// Walks `link_successors` (already in the linker's own order -- see
/// `super::link_order_successors`) in turn, taking each successor's raw
/// slots from its own index `0` until `needed` entries have been collected
/// or every successor is exhausted. Continuing into a second successor if
/// the first runs out first is exactly what the real unchecked byte fetch
/// would do -- it has no notion of a file boundary, only of "the next
/// `.4byte`/`.byte`s in the section" -- though the pinned reference
/// checkout never needs more than one (`intro.inc` alone declares all 128
/// of its own slots, far more than `title`'s 39-slot gap).
///
/// Each pulled entry goes through the exact same leaf/indirection dispatch
/// [`resolve_one`]'s own loop uses, so a key-split/rhythm entry borrowed
/// this way resolves (and, if new, emits) its child exactly as a declared
/// slot's reference would. A borrowed *sample* reference is different: the
/// sample pass (`crate::extract::audio_samples`) runs first, from its own
/// hand-maintained list, so a borrowed `DirectSound` slot can name a
/// sample that pass never extracted -- which is why that list carries the
/// overflow's additions (currently `sc88pro_xylophone`, borrowed slot 102)
/// and the real-pack closure test
/// (`crates/pokeemerald-rs/src/voicegroup_pack_tests.rs`) walks every
/// referenced id against the pack. If every successor's entries run out
/// before `needed` is met (or `link_successors` is empty -- the group is
/// last in the linker's own order, or its physical neighbour is a foreign
/// table, see `parser::LinkOrderItem::Foreign`), the shortfall is left for
/// [`pad_to_128`]'s ordinary trailing `Empty` pad: this pipeline cannot
/// know what bytes truly follow in the real linked binary, so silence is
/// the fail-closed choice, not a guess.
///
/// Two latent edges, unmodeled by design: a borrowed entry referencing the
/// borrower's own label would trip the shared `resolving` cycle guard and
/// abort extraction where the hardware would just read bytes (no upstream
/// group does this); and a top-level group with a nonzero `starting_note`
/// gets no *predecessor*-adjacency modeling for its leading bias slots,
/// which physically overlap the previous file's tail (moot upstream: every
/// biased group is a drumset reached only as a child).
///
/// # Errors
///
/// [`VoiceGroupError::DanglingVoiceGroupReference`] if `link_successors`
/// names a label `raw_groups` has no entry for (would only fire on a bug in
/// `super::link_order_successors`' own path-to-label mapping, since every
/// real linked file is already indexed); any error
/// [`resolve_indirection_slot`]/[`convert_leaf_slot`] can return for the
/// successor's own borrowed slot content.
#[allow(clippy::too_many_arguments)]
fn collect_link_adjacency_overflow(
    borrower_label: &str,
    needed: usize,
    link_successors: &[String],
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    resolving: &mut Vec<String>,
    order: &mut Vec<String>,
    resolved: &mut HashMap<String, ResolvedVoiceGroup>,
) -> Result<Vec<VoiceSlot>, VoiceGroupError> {
    let mut out = Vec::with_capacity(needed);
    for successor_label in link_successors {
        if out.len() >= needed {
            break;
        }
        let successor = raw_groups.get(successor_label).ok_or_else(|| {
            VoiceGroupError::DanglingVoiceGroupReference {
                referrer: borrower_label.to_owned(),
                target: successor_label.clone(),
            }
        })?;
        for raw_slot in &successor.slots {
            if out.len() >= needed {
                break;
            }
            // These entries occupy `borrower_label`'s own depth (they are,
            // physically, part of what upstream reads as `borrower_label`'s
            // table), never a nested indirection target -- `false` mirrors
            // `resolve_one`'s `is_indirection_target` for the borrower
            // itself. Errors attribute to `successor_label`, the file the
            // bytes actually come from, not the borrower.
            let resolved_slot = match raw_slot {
                RawSlot::KeySplit {
                    child_label,
                    table_label,
                } => resolve_indirection_slot(
                    successor_label,
                    false,
                    child_label,
                    Some(table_label),
                    raw_groups,
                    keysplit_tables,
                    link_successors,
                    resolving,
                    order,
                    resolved,
                )?,
                RawSlot::Rhythm { child_label } => resolve_indirection_slot(
                    successor_label,
                    false,
                    child_label,
                    None,
                    raw_groups,
                    keysplit_tables,
                    link_successors,
                    resolving,
                    order,
                    resolved,
                )?,
                leaf => convert_leaf_slot(leaf, successor_label)?,
            };
            out.push(resolved_slot);
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    label: &str,
    raw_groups: &HashMap<String, RawVoiceGroup>,
    keysplit_tables: &HashMap<String, RawKeySplitTable>,
    link_successors: &[String],
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
    // `ply_note` never recurses through (see `VoiceSlot`'s module docs). The
    // same depth-1 test also gates link-adjacency overflow below: only the
    // one group a song references directly is ever borrowed into by the
    // linker's own contiguous layout (see `collect_link_adjacency_overflow`).
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
                link_successors,
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
                link_successors,
                resolving,
                order,
                resolved,
            )?,
            leaf => convert_leaf_slot(leaf, &raw.label)?,
        };
        slots.push(resolved_slot);
    }

    if !is_indirection_target {
        let leading = usize::from(raw.starting_note);
        let needed = VOICE_SLOT_COUNT.saturating_sub(leading + slots.len());
        if needed > 0 && !link_successors.is_empty() {
            let overflow = collect_link_adjacency_overflow(
                &raw.label,
                needed,
                link_successors,
                raw_groups,
                keysplit_tables,
                resolving,
                order,
                resolved,
            )?;
            slots.extend(overflow);
        }
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
