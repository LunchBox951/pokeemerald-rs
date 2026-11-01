//! Parses the attached `mid2agb` flags in `sound/songs/midi/midi.cfg`.
//!
//! Assembly labels and assembly compression do not affect normalized song
//! data, so their flags are accepted but not stored.

use super::error::MidiError;

const DEFAULT_MASTER_VOLUME: u8 = 127;
const DEFAULT_CLOCKS_PER_BEAT: u8 = 1;
const DOUBLE_CLOCKS_PER_BEAT: u8 = 2;
const EXACT_GATE_TIME_FLAG: char = 'E';
const VOICEGROUP_FLAG: char = 'G';
const ASSEMBLY_OUTPUT_LABEL_FLAG: char = 'L';
const DISABLE_ASSEMBLY_COMPRESSION_FLAG: char = 'N';
const PRIORITY_FLAG: char = 'P';
const REVERB_FLAG: char = 'R';
const MASTER_VOLUME_FLAG: char = 'V';
const DOUBLE_CLOCKS_PER_BEAT_FLAG: char = 'X';

/// Compile settings read from one `midi.cfg` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MidiCfgEntry {
    /// Voicegroup label without leading underscores.
    pub(super) voicegroup_label: String,
    pub(super) priority: u8,
    /// `None` inherits the master reverb setting.
    pub(super) reverb: Option<u8>,
    pub(super) master_volume: u8,
    pub(super) exact_gate_time: bool,
    /// `1` selects 24 clocks per beat; `2` selects 48.
    pub(super) clocks_per_beat: u8,
}

impl Default for MidiCfgEntry {
    fn default() -> Self {
        Self {
            voicegroup_label: String::new(),
            priority: 0,
            reverb: None,
            master_volume: DEFAULT_MASTER_VOLUME,
            exact_gate_time: false,
            clocks_per_beat: DEFAULT_CLOCKS_PER_BEAT,
        }
    }
}

fn parse_u8_operand(token: &str, value: &str) -> Result<u8, MidiError> {
    value
        .parse()
        .map_err(|_| MidiError::CfgMalformedFlag(token.to_owned()))
}

fn apply_flag(entry: &mut MidiCfgEntry, token: &str) -> Result<(), MidiError> {
    let Some(flag) = token.strip_prefix('-') else {
        return Err(MidiError::CfgMalformedFlag(token.to_owned()));
    };
    let Some(letter) = flag.chars().next() else {
        return Err(MidiError::CfgMalformedFlag(token.to_owned()));
    };
    let value = &flag[letter.len_utf8()..];
    let letter = letter.to_ascii_uppercase();
    let is_switch = [
        EXACT_GATE_TIME_FLAG,
        DOUBLE_CLOCKS_PER_BEAT_FLAG,
        DISABLE_ASSEMBLY_COMPRESSION_FLAG,
    ]
    .contains(&letter);
    if is_switch && !value.is_empty() {
        return Err(MidiError::CfgMalformedFlag(token.to_owned()));
    }
    match letter {
        EXACT_GATE_TIME_FLAG => entry.exact_gate_time = true,
        DOUBLE_CLOCKS_PER_BEAT_FLAG => entry.clocks_per_beat = DOUBLE_CLOCKS_PER_BEAT,
        ASSEMBLY_OUTPUT_LABEL_FLAG | DISABLE_ASSEMBLY_COMPRESSION_FLAG => {}
        VOICEGROUP_FLAG if value.trim_start_matches('_').is_empty() => {
            return Err(MidiError::CfgMalformedFlag(token.to_owned()));
        }
        VOICEGROUP_FLAG => value
            .trim_start_matches('_')
            .clone_into(&mut entry.voicegroup_label),
        PRIORITY_FLAG => entry.priority = parse_u8_operand(token, value)?,
        REVERB_FLAG => entry.reverb = Some(parse_u8_operand(token, value)?),
        MASTER_VOLUME_FLAG => entry.master_volume = parse_u8_operand(token, value)?,
        _ => return Err(MidiError::CfgMalformedFlag(token.to_owned())),
    }
    Ok(())
}

/// Parses the flags for `filename` without requiring a `-G` flag: an entry that omits it keeps
/// an empty [`MidiCfgEntry::voicegroup_label`], while a `-G` whose operand names nothing is still a
/// [`MidiError::CfgMalformedFlag`].
///
/// # Errors
///
/// Returns [`MidiError::CfgEntryMissing`], [`MidiError::CfgEntryDuplicate`], or
/// [`MidiError::CfgMalformedFlag`].
pub(super) fn parse_entry_flags(text: &str, filename: &str) -> Result<MidiCfgEntry, MidiError> {
    let prefix = format!("{filename}:");
    let mut lines = text
        .lines()
        .map(str::trim_start)
        .filter_map(|line| line.strip_prefix(&prefix));
    let rest = lines
        .next()
        .ok_or_else(|| MidiError::CfgEntryMissing(filename.to_owned()))?;
    if lines.next().is_some() {
        return Err(MidiError::CfgEntryDuplicate(filename.to_owned()));
    }

    let mut entry = MidiCfgEntry::default();
    for token in rest.split_whitespace() {
        apply_flag(&mut entry, token)?;
    }
    Ok(entry)
}

/// Parses the flags for `filename`, which must carry a `-G` flag.
///
/// # Errors
///
/// Returns the errors of [`parse_entry_flags`], or [`MidiError::CfgMissingVoiceGroup`].
pub(super) fn parse_entry_for(text: &str, filename: &str) -> Result<MidiCfgEntry, MidiError> {
    let entry = parse_entry_flags(text, filename)?;
    if entry.voicegroup_label.is_empty() {
        return Err(MidiError::CfgMissingVoiceGroup);
    }
    Ok(entry)
}

#[cfg(test)]
mod tests;
