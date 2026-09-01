pub(super) use super::error::VoiceGroupError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Envelope {
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DirectSoundMode {
    Resampled,
    Fixed,
    Reverse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RawSlot {
    DirectSound {
        base_key: u8,
        pan: Option<u8>,
        sample_symbol: String,
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
        wave_symbol: String,
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
        child_label: String,
        table_label: String,
    },
    Rhythm {
        child_label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawVoiceGroup {
    pub label: String,
    pub starting_note: u8,
    pub slots: Vec<RawSlot>,
}

/// An expanded key-split table whose entry `i` selects a child slot for note
/// `starting_note + i`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawKeySplitTable {
    pub starting_note: u8,
    pub table: Vec<u8>,
}

struct MacroInvocation<'a> {
    name: &'a str,
    operands: Vec<&'a str>,
}

fn parse_macro_invocation(line: &str) -> Option<MacroInvocation<'_>> {
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
    Some(MacroInvocation { name, operands })
}

struct VoiceGroupDeclaration {
    label: String,
    starting_note: u8,
}

fn parse_voice_group_declaration(line: &str) -> Result<VoiceGroupDeclaration, VoiceGroupError> {
    let invocation =
        parse_macro_invocation(line).ok_or(VoiceGroupError::MissingVoiceGroupDeclaration)?;
    if invocation.name != "voice_group" {
        return Err(VoiceGroupError::MissingVoiceGroupDeclaration);
    }
    let [label, optional_operands @ ..] = invocation.operands.as_slice() else {
        return Err(VoiceGroupError::MissingVoiceGroupDeclaration);
    };
    if label.is_empty() {
        return Err(VoiceGroupError::MissingVoiceGroupDeclaration);
    }
    let starting_note = match optional_operands.first() {
        Some(operand) => operand
            .parse::<u8>()
            .map_err(|_| VoiceGroupError::InvalidVoiceGroupStartingNote)?,
        None => 0,
    };
    Ok(VoiceGroupDeclaration {
        label: (*label).to_owned(),
        starting_note,
    })
}

fn malformed_voice_slot(group: &str, line: &str) -> VoiceGroupError {
    VoiceGroupError::MalformedVoiceSlot {
        group: group.to_owned(),
        line: line.to_owned(),
    }
}

fn parse_byte(operand: &str, group: &str, line: &str) -> Result<u8, VoiceGroupError> {
    operand
        .parse::<u8>()
        .map_err(|_| malformed_voice_slot(group, line))
}

fn parse_optional_pan(
    operand: &str,
    group: &str,
    line: &str,
) -> Result<Option<u8>, VoiceGroupError> {
    const NO_PAN_OVERRIDE: u8 = 0;
    let pan = parse_byte(operand, group, line)?;
    Ok((pan != NO_PAN_OVERRIDE).then_some(pan))
}

fn parse_envelope(
    [attack, decay, sustain, release]: [&str; 4],
    group: &str,
    line: &str,
) -> Result<Envelope, VoiceGroupError> {
    Ok(Envelope {
        attack: parse_byte(attack, group, line)?,
        decay: parse_byte(decay, group, line)?,
        sustain: parse_byte(sustain, group, line)?,
        release: parse_byte(release, group, line)?,
    })
}

fn parse_prefixed_label(
    symbol: &str,
    prefix: &'static str,
    group: &str,
) -> Result<String, VoiceGroupError> {
    symbol
        .strip_prefix(prefix)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| VoiceGroupError::MalformedReference {
            group: group.to_owned(),
            reference: symbol.to_owned(),
            expected_prefix: prefix,
        })
}

fn parse_slot_line(line: &str, group: &str) -> Result<RawSlot, VoiceGroupError> {
    let invocation =
        parse_macro_invocation(line).ok_or_else(|| malformed_voice_slot(group, line))?;
    let operands = invocation.operands.as_slice();

    match invocation.name {
        "voice_directsound" | "voice_directsound_no_resample" | "voice_directsound_alt" => {
            let [base_key, pan, sample_symbol, attack, decay, sustain, release, ..] = operands
            else {
                return Err(malformed_voice_slot(group, line));
            };
            let mode = match invocation.name {
                "voice_directsound" => DirectSoundMode::Resampled,
                "voice_directsound_no_resample" => DirectSoundMode::Fixed,
                "voice_directsound_alt" => DirectSoundMode::Reverse,
                _ => unreachable!(),
            };
            Ok(RawSlot::DirectSound {
                base_key: parse_byte(base_key, group, line)?,
                pan: parse_optional_pan(pan, group, line)?,
                sample_symbol: (*sample_symbol).to_owned(),
                envelope: parse_envelope([attack, decay, sustain, release], group, line)?,
                mode,
            })
        }
        "voice_square_1" | "voice_square_1_alt" => {
            let [base_key, length, sweep, duty, attack, decay, sustain, release, ..] = operands
            else {
                return Err(malformed_voice_slot(group, line));
            };
            Ok(RawSlot::Square1 {
                base_key: parse_byte(base_key, group, line)?,
                length: parse_byte(length, group, line)?,
                sweep: parse_byte(sweep, group, line)?,
                duty: parse_byte(duty, group, line)?,
                envelope: parse_envelope([attack, decay, sustain, release], group, line)?,
                fixed_rate: invocation.name.ends_with("_alt"),
            })
        }
        "voice_square_2" | "voice_square_2_alt" => {
            let [base_key, length, duty, attack, decay, sustain, release, ..] = operands else {
                return Err(malformed_voice_slot(group, line));
            };
            Ok(RawSlot::Square2 {
                base_key: parse_byte(base_key, group, line)?,
                length: parse_byte(length, group, line)?,
                duty: parse_byte(duty, group, line)?,
                envelope: parse_envelope([attack, decay, sustain, release], group, line)?,
                fixed_rate: invocation.name.ends_with("_alt"),
            })
        }
        "voice_programmable_wave" | "voice_programmable_wave_alt" => {
            let [base_key, length, wave_symbol, attack, decay, sustain, release, ..] = operands
            else {
                return Err(malformed_voice_slot(group, line));
            };
            Ok(RawSlot::ProgrammableWave {
                base_key: parse_byte(base_key, group, line)?,
                length: parse_byte(length, group, line)?,
                wave_symbol: (*wave_symbol).to_owned(),
                envelope: parse_envelope([attack, decay, sustain, release], group, line)?,
                fixed_rate: invocation.name.ends_with("_alt"),
            })
        }
        "voice_noise" | "voice_noise_alt" => {
            let [base_key, length, period, attack, decay, sustain, release, ..] = operands else {
                return Err(malformed_voice_slot(group, line));
            };
            Ok(RawSlot::Noise {
                base_key: parse_byte(base_key, group, line)?,
                length: parse_byte(length, group, line)?,
                period: parse_byte(period, group, line)?,
                envelope: parse_envelope([attack, decay, sustain, release], group, line)?,
                fixed_rate: invocation.name.ends_with("_alt"),
            })
        }
        "voice_keysplit" => {
            let [voice_group_symbol, key_split_symbol, ..] = operands else {
                return Err(malformed_voice_slot(group, line));
            };
            Ok(RawSlot::KeySplit {
                child_label: parse_prefixed_label(voice_group_symbol, "voicegroup_", group)?,
                table_label: parse_prefixed_label(key_split_symbol, "keysplit_", group)?,
            })
        }
        "voice_keysplit_all" => {
            let [voice_group_symbol, ..] = operands else {
                return Err(malformed_voice_slot(group, line));
            };
            Ok(RawSlot::Rhythm {
                child_label: parse_prefixed_label(voice_group_symbol, "voicegroup_", group)?,
            })
        }
        macro_name => Err(VoiceGroupError::UnknownVoiceMacro {
            group: group.to_owned(),
            macro_name: macro_name.to_owned(),
        }),
    }
}

pub(super) fn parse_voice_group(text: &str) -> Result<RawVoiceGroup, VoiceGroupError> {
    let mut lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('@'));
    let declaration_line = lines
        .next()
        .ok_or(VoiceGroupError::MissingVoiceGroupDeclaration)?;
    let declaration = parse_voice_group_declaration(declaration_line)?;

    let mut slots = Vec::new();
    for line in lines {
        slots.push(parse_slot_line(line, &declaration.label)?);
    }
    Ok(RawVoiceGroup {
        label: declaration.label,
        starting_note: declaration.starting_note,
        slots,
    })
}

struct KeySplitBuilder {
    label: String,
    starting_note: u8,
    next_note: u8,
    table: Vec<u8>,
}

impl KeySplitBuilder {
    fn parse_declaration(operands: &[&str]) -> Result<Self, VoiceGroupError> {
        let [label, optional_operands @ ..] = operands else {
            return Err(VoiceGroupError::MissingKeySplitLabel);
        };
        if label.is_empty() {
            return Err(VoiceGroupError::MissingKeySplitLabel);
        }
        let starting_note = match optional_operands.first() {
            Some(operand) => operand
                .parse::<u8>()
                .map_err(|_| VoiceGroupError::InvalidKeySplitStartingNote)?,
            None => 0,
        };
        Ok(Self {
            label: (*label).to_owned(),
            starting_note,
            next_note: starting_note,
            table: Vec::new(),
        })
    }

    fn append_split(&mut self, split: &KeySplitRange) -> Result<(), VoiceGroupError> {
        if split.exclusive_end_note < self.next_note {
            return Err(VoiceGroupError::SplitOutOfOrder {
                table: self.label.clone(),
            });
        }
        let note_count = usize::from(split.exclusive_end_note - self.next_note);
        self.table
            .extend(std::iter::repeat_n(split.child_slot, note_count));
        self.next_note = split.exclusive_end_note;
        Ok(())
    }
}

struct KeySplitRange {
    child_slot: u8,
    exclusive_end_note: u8,
}

fn parse_key_split_range(operands: &[&str], table: &str) -> Result<KeySplitRange, VoiceGroupError> {
    let invalid_operands = || VoiceGroupError::InvalidSplitOperands {
        table: table.to_owned(),
    };
    let [child_slot, exclusive_end_note, ..] = operands else {
        return Err(invalid_operands());
    };
    Ok(KeySplitRange {
        child_slot: child_slot.parse::<u8>().map_err(|_| invalid_operands())?,
        exclusive_end_note: exclusive_end_note
            .parse::<u8>()
            .map_err(|_| invalid_operands())?,
    })
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
            expanded_len: builder.table.len(),
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
        return Err(VoiceGroupError::DuplicateKeySplitTable {
            label: builder.label,
        });
    }
    Ok(())
}

pub(super) fn parse_keysplit_tables(
    text: &str,
) -> Result<std::collections::HashMap<String, RawKeySplitTable>, VoiceGroupError> {
    let mut out = std::collections::HashMap::new();
    let mut current: Option<KeySplitBuilder> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('@') {
            continue;
        }
        let Some(invocation) = parse_macro_invocation(line) else {
            continue;
        };
        match invocation.name {
            "keysplit" => {
                finish_keysplit_block(current.take(), &mut out)?;
                current = Some(KeySplitBuilder::parse_declaration(&invocation.operands)?);
            }
            "split" => {
                let builder = current
                    .as_mut()
                    .ok_or(VoiceGroupError::SplitBeforeKeySplit)?;
                let split = parse_key_split_range(&invocation.operands, &builder.label)?;
                builder.append_split(&split)?;
            }
            macro_name => {
                return Err(VoiceGroupError::UnknownKeySplitMacro {
                    macro_name: macro_name.to_owned(),
                });
            }
        }
    }
    finish_keysplit_block(current, &mut out)?;
    Ok(out)
}

fn parse_include_path(line: &str) -> Option<&str> {
    let operands = line.trim().strip_prefix(".include")?.trim_start();
    let quoted_path = operands.strip_prefix('"')?;
    let closing_quote = quoted_path.find('"')?;
    Some(&quoted_path[..closing_quote])
}

pub(super) fn parse_link_order(text: &str) -> Vec<LinkOrderItem> {
    const VOICEGROUP_INCLUDE_PREFIX: &str = "sound/voicegroups/";
    text.lines()
        .filter_map(parse_include_path)
        .map(|path| match path.strip_prefix(VOICEGROUP_INCLUDE_PREFIX) {
            Some(relative) => LinkOrderItem::VoiceGroup(relative.to_owned()),
            None => LinkOrderItem::Foreign,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LinkOrderItem {
    VoiceGroup(String),
    /// A non-voicegroup include whose bytes break voicegroup adjacency.
    Foreign,
}

#[cfg(test)]
mod tests;
