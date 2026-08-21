//! Line-level parsing of upstream voicegroup sources: one `.inc` file under
//! `pokeemerald/sound/voicegroups/` (a single `voice_group` declaration
//! followed by its slots, one `voice_*` macro invocation per line — see
//! `pokeemerald/asm/macros/music_voice.inc`), and
//! `pokeemerald/sound/keysplit_tables.inc` (several `keysplit`/`split`
//! blocks — see that file's own header comment).
//!
//! Pure text in, structured data out — no filesystem access here (that's
//! `super`'s job, mirroring [`super::super::jasc_pal`]/[`super::super::layouts_json`]'s
//! split). Nothing here resolves a `voice_keysplit`/`voice_keysplit_all`
//! reference into another group's content, derives a pack id, or knows
//! about the 128-slot bound — that linking work is [`super::resolve`]'s job.

pub(crate) use super::error::VoiceGroupError;

/// Every value operand this parser reads is a plain `u8` (a MIDI key, a
/// pan/length byte, an envelope component, ...) — matching
/// `asm/macros/music_voice.inc`'s own operand shapes.
type RawU8 = u8;

/// An instrument's attack/decay/sustain/release envelope — the trailing
/// four operands every leaf `voice_*` macro shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Envelope {
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
}

/// Which `voice_directsound*` macro produced a [`RawSlot::DirectSound`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectSoundMode {
    Resampled,
    Fixed,
    Reverse,
}

/// One parsed `.inc` body line, before any cross-file linking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RawSlot {
    DirectSound {
        base_key: RawU8,
        /// `None` when the source's `pan` operand is literally `0` (no
        /// override) — see `crates/assets/src/audio/voicegroup.rs`'s
        /// `DirectSoundVoice::pan` docs for why `0` is the sentinel.
        pan: Option<RawU8>,
        /// The raw `DirectSoundWaveData_*` symbol name, unresolved.
        sample_symbol: String,
        envelope: Envelope,
        mode: DirectSoundMode,
    },
    Square1 {
        base_key: RawU8,
        length: RawU8,
        sweep: RawU8,
        duty: RawU8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    Square2 {
        base_key: RawU8,
        length: RawU8,
        duty: RawU8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    ProgrammableWave {
        base_key: RawU8,
        length: RawU8,
        /// The raw `ProgrammableWaveData_*` symbol name, unresolved.
        wave_symbol: String,
        envelope: Envelope,
        fixed_rate: bool,
    },
    Noise {
        base_key: RawU8,
        length: RawU8,
        period: RawU8,
        envelope: Envelope,
        fixed_rate: bool,
    },
    /// `voice_keysplit voicegroup_X, keysplit_Y` — `child_label` and
    /// `table_label` are already stripped of their `voicegroup_`/`keysplit_`
    /// symbol prefixes (e.g. `X`, `Y`), not yet resolved to another group's
    /// content or a table's bytes.
    KeySplit {
        child_label: String,
        table_label: String,
    },
    /// `voice_keysplit_all voicegroup_X` — `child_label` likewise stripped
    /// of its `voicegroup_` prefix.
    Rhythm { child_label: String },
}

/// One fully-parsed `.inc` file: its declared label, `starting_note` bias
/// (`0` unless the `voice_group` line carries a second operand — the
/// drumsets' convention for aligning a rhythm group to real MIDI note
/// numbers, see `pokeemerald/sound/voicegroups/drumsets/rs.inc`), and its
/// slots in source order (*not* yet padded to 128 — see [`super::resolve`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawVoiceGroup {
    pub label: String,
    pub starting_note: u8,
    pub slots: Vec<RawSlot>,
}

/// One parsed `keysplit <label>[, <starting_note>]` block from
/// `keysplit_tables.inc`: `table[i]` selects a child slot index for note
/// `starting_note + i`, built by expanding each `split index, ending_note`
/// line's `.rept` per that file's own header comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawKeySplitTable {
    pub starting_note: u8,
    pub table: Vec<u8>,
}

/// Split one macro-invocation line into `(macro_name, comma_separated_operands)`.
/// `None` for a line with no macro name at all (blank after trimming --
/// callers already filter those out, so this is just defence in depth).
fn split_macro_line(line: &str) -> Option<(&str, Vec<&str>)> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let name = parts.next()?;
    if name.is_empty() {
        return None;
    }
    let rest = parts.next().unwrap_or("").trim();
    let operands = if rest.is_empty() {
        Vec::new()
    } else {
        rest.split(',').map(str::trim).collect()
    };
    Some((name, operands))
}

/// Parse a `voice_group <label>[, <starting_note>]` header line.
fn parse_header(line: &str) -> Result<(String, u8), VoiceGroupError> {
    let (name, ops) = split_macro_line(line).ok_or(VoiceGroupError::MissingDeclaration)?;
    if name != "voice_group" {
        return Err(VoiceGroupError::MissingDeclaration);
    }
    let label = ops
        .first()
        .filter(|l| !l.is_empty())
        .ok_or(VoiceGroupError::MissingDeclaration)?
        .to_string();
    let starting_note = match ops.get(1) {
        Some(s) => s
            .parse::<u8>()
            .map_err(|_| VoiceGroupError::BadStartingNote)?,
        None => 0,
    };
    Ok((label, starting_note))
}

fn op_u8(ops: &[&str], index: usize, group: &str, line: &str) -> Result<u8, VoiceGroupError> {
    ops.get(index)
        .and_then(|s| s.parse::<u8>().ok())
        .ok_or_else(|| VoiceGroupError::MalformedLine {
            group: group.to_owned(),
            line: line.to_owned(),
        })
}

fn op_str(ops: &[&str], index: usize, group: &str, line: &str) -> Result<String, VoiceGroupError> {
    ops.get(index)
        .map(|s| (*s).to_owned())
        .ok_or_else(|| VoiceGroupError::MalformedLine {
            group: group.to_owned(),
            line: line.to_owned(),
        })
}

/// A `pan` operand: `0` is the wire's own "no override" sentinel (see
/// `crates/assets/src/audio/voicegroup.rs`'s `write_pan`/`read_pan`), so
/// this is the one place that reading turns into `None`.
fn op_pan(
    ops: &[&str],
    index: usize,
    group: &str,
    line: &str,
) -> Result<Option<u8>, VoiceGroupError> {
    let value = op_u8(ops, index, group, line)?;
    Ok((value != 0).then_some(value))
}

fn op_envelope(
    ops: &[&str],
    start: usize,
    group: &str,
    line: &str,
) -> Result<Envelope, VoiceGroupError> {
    Ok(Envelope {
        attack: op_u8(ops, start, group, line)?,
        decay: op_u8(ops, start + 1, group, line)?,
        sustain: op_u8(ops, start + 2, group, line)?,
        release: op_u8(ops, start + 3, group, line)?,
    })
}

/// Strip a `voicegroup_`/`keysplit_` symbol prefix off a `voice_keysplit`/
/// `voice_keysplit_all` operand, leaving the bare label another `.inc`
/// file's own `voice_group`/`keysplit` declaration uses.
fn strip_symbol_prefix(
    text: &str,
    prefix: &'static str,
    group: &str,
) -> Result<String, VoiceGroupError> {
    text.strip_prefix(prefix)
        .filter(|rest| !rest.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: text.to_owned(),
            expected_prefix: prefix,
        })
}

fn parse_slot_line(line: &str, group: &str) -> Result<RawSlot, VoiceGroupError> {
    let malformed = || VoiceGroupError::MalformedLine {
        group: group.to_owned(),
        line: line.to_owned(),
    };
    let (name, ops) = split_macro_line(line).ok_or_else(malformed)?;

    match name {
        "voice_directsound" | "voice_directsound_no_resample" | "voice_directsound_alt" => {
            let mode = match name {
                "voice_directsound" => DirectSoundMode::Resampled,
                "voice_directsound_no_resample" => DirectSoundMode::Fixed,
                _ => DirectSoundMode::Reverse,
            };
            Ok(RawSlot::DirectSound {
                base_key: op_u8(&ops, 0, group, line)?,
                pan: op_pan(&ops, 1, group, line)?,
                sample_symbol: op_str(&ops, 2, group, line)?,
                envelope: op_envelope(&ops, 3, group, line)?,
                mode,
            })
        }
        "voice_square_1" | "voice_square_1_alt" => Ok(RawSlot::Square1 {
            base_key: op_u8(&ops, 0, group, line)?,
            length: op_u8(&ops, 1, group, line)?,
            sweep: op_u8(&ops, 2, group, line)?,
            duty: op_u8(&ops, 3, group, line)?,
            envelope: op_envelope(&ops, 4, group, line)?,
            fixed_rate: name.ends_with("_alt"),
        }),
        "voice_square_2" | "voice_square_2_alt" => Ok(RawSlot::Square2 {
            base_key: op_u8(&ops, 0, group, line)?,
            length: op_u8(&ops, 1, group, line)?,
            duty: op_u8(&ops, 2, group, line)?,
            envelope: op_envelope(&ops, 3, group, line)?,
            fixed_rate: name.ends_with("_alt"),
        }),
        "voice_programmable_wave" | "voice_programmable_wave_alt" => {
            Ok(RawSlot::ProgrammableWave {
                base_key: op_u8(&ops, 0, group, line)?,
                length: op_u8(&ops, 1, group, line)?,
                wave_symbol: op_str(&ops, 2, group, line)?,
                envelope: op_envelope(&ops, 3, group, line)?,
                fixed_rate: name.ends_with("_alt"),
            })
        }
        "voice_noise" | "voice_noise_alt" => Ok(RawSlot::Noise {
            base_key: op_u8(&ops, 0, group, line)?,
            length: op_u8(&ops, 1, group, line)?,
            period: op_u8(&ops, 2, group, line)?,
            envelope: op_envelope(&ops, 3, group, line)?,
            fixed_rate: name.ends_with("_alt"),
        }),
        "voice_keysplit" => {
            let voice_group_operand = op_str(&ops, 0, group, line)?;
            let table_operand = op_str(&ops, 1, group, line)?;
            Ok(RawSlot::KeySplit {
                child_label: strip_symbol_prefix(&voice_group_operand, "voicegroup_", group)?,
                table_label: strip_symbol_prefix(&table_operand, "keysplit_", group)?,
            })
        }
        "voice_keysplit_all" => {
            let voice_group_operand = op_str(&ops, 0, group, line)?;
            Ok(RawSlot::Rhythm {
                child_label: strip_symbol_prefix(&voice_group_operand, "voicegroup_", group)?,
            })
        }
        _ => Err(VoiceGroupError::UnknownMacro {
            group: group.to_owned(),
            macro_name: name.to_owned(),
        }),
    }
}

/// Parse one voicegroup `.inc` file's full text.
///
/// # Errors
///
/// See [`VoiceGroupError`]'s variants.
pub(crate) fn parse_voice_group(text: &str) -> Result<RawVoiceGroup, VoiceGroupError> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('@'));
    let header = lines.next().ok_or(VoiceGroupError::MissingDeclaration)?;
    let (label, starting_note) = parse_header(header)?;

    let mut slots = Vec::new();
    for line in lines {
        slots.push(parse_slot_line(line, &label)?);
    }
    Ok(RawVoiceGroup {
        label,
        starting_note,
        slots,
    })
}

/// Accumulates one `keysplit <label>[, <starting_note>]` block's `split`
/// lines while [`parse_keysplit_tables`] walks the file.
struct KeySplitBuilder {
    label: String,
    last_note: u8,
    starting_note: u8,
    table: Vec<u8>,
}

fn finish_keysplit_block(
    current: Option<KeySplitBuilder>,
    out: &mut std::collections::HashMap<String, RawKeySplitTable>,
) -> Result<(), VoiceGroupError> {
    let Some(builder) = current else {
        return Ok(());
    };
    if builder.table.len() > super::VOICE_SLOT_COUNT {
        return Err(VoiceGroupError::KeySplitTableTooLong {
            label: builder.label,
            len: builder.table.len(),
        });
    }
    let previous = out.insert(
        builder.label.clone(),
        RawKeySplitTable {
            starting_note: builder.starting_note,
            table: builder.table,
        },
    );
    if previous.is_some() {
        return Err(VoiceGroupError::DuplicateKeySplitTable(builder.label));
    }
    Ok(())
}

/// Parse `keysplit_tables.inc`'s full text into every `keysplit` block,
/// keyed by label. See [`RawKeySplitTable`] and that file's own header
/// comment for the `starting_note` bias this reconstructs.
///
/// # Errors
///
/// See [`VoiceGroupError`]'s variants.
pub(crate) fn parse_keysplit_tables(
    text: &str,
) -> Result<std::collections::HashMap<String, RawKeySplitTable>, VoiceGroupError> {
    let mut out = std::collections::HashMap::new();
    let mut current: Option<KeySplitBuilder> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('@') {
            continue;
        }
        let Some((name, ops)) = split_macro_line(line) else {
            continue;
        };
        match name {
            "keysplit" => {
                finish_keysplit_block(current.take(), &mut out)?;
                let label = ops
                    .first()
                    .filter(|l| !l.is_empty())
                    .ok_or(VoiceGroupError::MalformedKeySplitTableHeader)?
                    .to_string();
                let starting_note = match ops.get(1) {
                    Some(s) => s
                        .parse::<u8>()
                        .map_err(|_| VoiceGroupError::BadKeySplitStartingNote)?,
                    None => 0,
                };
                current = Some(KeySplitBuilder {
                    label,
                    last_note: starting_note,
                    starting_note,
                    table: Vec::new(),
                });
            }
            "split" => {
                let builder = current
                    .as_mut()
                    .ok_or(VoiceGroupError::SplitOutsideKeySplit)?;
                let index = ops
                    .first()
                    .and_then(|s| s.parse::<u8>().ok())
                    .ok_or_else(|| VoiceGroupError::MalformedSplitLine(builder.label.clone()))?;
                let ending_note = ops
                    .get(1)
                    .and_then(|s| s.parse::<u8>().ok())
                    .ok_or_else(|| VoiceGroupError::MalformedSplitLine(builder.label.clone()))?;
                if ending_note < builder.last_note {
                    return Err(VoiceGroupError::SplitOutOfOrder(builder.label.clone()));
                }
                let count = usize::from(ending_note - builder.last_note);
                builder.table.extend(std::iter::repeat_n(index, count));
                builder.last_note = ending_note;
            }
            // Fail closed rather than skipping, exactly as `parse_slot_line`
            // does: a macro this parser doesn't recognize is a table entry
            // it would silently drop, and a keysplit table's contents *are*
            // its note-to-slot mapping. Only `keysplit`/`split` appear in
            // the real `keysplit_tables.inc`, so this is not expected to
            // fire against the pinned reference checkout.
            other => return Err(VoiceGroupError::UnknownKeySplitMacro(other.to_owned())),
        }
    }
    finish_keysplit_block(current, &mut out)?;
    Ok(out)
}

/// Parse `sound/voice_groups.inc`'s own `.include "sound/voicegroups/<relative>"`
/// lines, in file order -- the assembler's actual concatenation order for
/// every voicegroup `.inc` file's byte table (see `super`'s "Modeled
/// link-adjacency read" module docs). Returns each line's `<relative>` path
/// (e.g. `"title.inc"`, `"drumsets/rs.inc"`), unresolved to a label -- that's
/// [`super::link_order_successors`]'s job, once filesystem access is
/// available to match a path back to the label its `.inc` file actually
/// declares (a `voice_group` label need not match its filename -- e.g.
/// `drumsets/rs.inc` declares `rs_drumset`, not `rs`).
///
/// An `.include` line naming a path outside `sound/voicegroups/` (e.g.
/// `sound/voice_groups.inc:136`'s `sound/cry_tables.inc`) becomes
/// [`LinkOrderItem::Foreign`] rather than being dropped: physical
/// adjacency is *bytes*, so the group preceding a foreign include is
/// followed in memory by that table's data, not by the next voicegroup
/// file -- collapsing the two would fabricate a successor
/// ([`super::link_order_successors`] stops at the barrier and fail-closes
/// to the same `Empty` padding as a last-in-order group). A non-`.include`
/// line (blank, an `@` comment) contributes nothing -- this reads an
/// assembler include list, not a voicegroup source, so it has no macro
/// grammar of its own to fail closed on.
pub(crate) fn parse_link_order(text: &str) -> Vec<LinkOrderItem> {
    const PREFIX: &str = "sound/voicegroups/";
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(".include")?.trim_start();
            let quoted = rest.strip_prefix('"')?;
            let end = quoted.find('"')?;
            let path = &quoted[..end];
            Some(match path.strip_prefix(PREFIX) {
                Some(relative) => LinkOrderItem::VoiceGroup(relative.to_owned()),
                None => LinkOrderItem::Foreign,
            })
        })
        .collect()
}

/// One `.include` line of `sound/voice_groups.inc`, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkOrderItem {
    /// A `sound/voicegroups/<relative>` include -- carries `<relative>`.
    VoiceGroup(String),
    /// Any other include (e.g. `sound/cry_tables.inc`): an adjacency
    /// barrier -- the preceding group's overflow bytes are this table's
    /// data, which this pipeline cannot (and must not) model as voices.
    Foreign,
}

#[cfg(test)]
mod tests;
