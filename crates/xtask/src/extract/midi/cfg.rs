//! Parses `sound/songs/midi/midi.cfg`: one line per `.mid` source, naming
//! the `tools/mid2agb` compile flags its build rule passes
//! (`audio_rules.mk`'s `MID_RULE`/`MID_EXPANSION`: `$(MID) $< $@ $2`, where
//! `$2` is the line's own flag text). Generic over every entry in the file
//! (matching this crate's other upstream-manifest parsers — see
//! `super::super::layouts_json`) even though this slice only ever looks up
//! `mus_title.mid`'s own line, so a lookup miss is a clear
//! [`super::error::MidiError::CfgEntryMissing`] rather than an assumption
//! baked into a hardcoded constant.
//!
//! # Flag syntax
//!
//! Every flag in the real file is `-<letter><value>` with the value
//! attached directly (`-R50`, `-G_title`, `-V090` — never `-R 50`,
//! confirmed by inspecting every line of the real checkout's `midi.cfg`),
//! matching `tools/mid2agb/main.cpp:143-186`'s per-letter `switch`. This
//! parser only implements the attached form.

use super::error::MidiError;

/// The subset of a `midi.cfg` line's flags this compiler's pipeline needs.
/// Fields mirror `tools/mid2agb/main.cpp`'s globals of the same purpose
/// (`g_voiceGroup`, `g_priority`, `g_reverb`, `g_masterVolume`,
/// `g_exactGateTime`, `g_clocksPerBeat`) — `-L` (asm label) and `-N` (disable
/// compression) are parsed (so an unrecognized-flag error doesn't fire on
/// real entries that carry them) but not stored: this pipeline picks its own
/// stable pack id independent of `-L`, and compression is out of scope
/// regardless of `-N` (`super`'s module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MidiCfgEntry {
    /// The `-G` operand with its leading underscore stripped (`"_title"` ->
    /// `"title"`) — matching `xtask::extract::voicegroups`'s own label
    /// convention for the same voicegroup (`TOP_LEVEL_LABEL`).
    pub(super) voicegroup_label: String,
    pub(super) priority: u8,
    /// `None` when no `-R` flag is present (matching `g_reverb`'s `-1`
    /// sentinel — "song does not override the master reverb setting").
    pub(super) reverb: Option<u8>,
    pub(super) master_volume: u8,
    pub(super) exact_gate_time: bool,
    /// `1` (24 clocks/beat, the default) or `2` (48 clocks/beat, `-X`).
    pub(super) clocks_per_beat: u8,
}

impl Default for MidiCfgEntry {
    /// `tools/mid2agb/main.cpp:37-43`'s own defaults, before any flag is
    /// applied — every default except `g_voiceGroup`'s `"_dummy"`, which
    /// this parser instead requires an explicit `-G` for (a song with no
    /// real voicegroup has nothing for [`super::compile`] to link against).
    fn default() -> Self {
        Self {
            voicegroup_label: String::new(),
            priority: 0,
            reverb: None,
            master_volume: 127,
            exact_gate_time: false,
            clocks_per_beat: 1,
        }
    }
}

fn parse_u8_operand(token: &str, value: &str) -> Result<u8, MidiError> {
    value
        .parse()
        .map_err(|_| MidiError::CfgMalformedFlag(token.to_owned()))
}

/// Parse one flag token (e.g. `"-V090"`) into `entry`.
fn apply_flag(entry: &mut MidiCfgEntry, token: &str) -> Result<(), MidiError> {
    let Some(flag) = token.strip_prefix('-') else {
        return Err(MidiError::CfgMalformedFlag(token.to_owned()));
    };
    let Some(letter) = flag.chars().next() else {
        return Err(MidiError::CfgMalformedFlag(token.to_owned()));
    };
    let value = &flag[letter.len_utf8()..];
    match letter.to_ascii_uppercase() {
        'E' => entry.exact_gate_time = true,
        'X' => entry.clocks_per_beat = 2,
        'N' | 'L' => {} // parsed, deliberately not stored -- see the module docs
        'G' => value
            .trim_start_matches('_')
            .clone_into(&mut entry.voicegroup_label),
        'P' => entry.priority = parse_u8_operand(token, value)?,
        'R' => entry.reverb = Some(parse_u8_operand(token, value)?),
        'V' => entry.master_volume = parse_u8_operand(token, value)?,
        _ => return Err(MidiError::CfgMalformedFlag(token.to_owned())),
    }
    Ok(())
}

/// Find `filename`'s line in `midi.cfg`'s text and parse its flags.
///
/// # Errors
///
/// [`MidiError::CfgEntryMissing`] if no line starts with `filename` +
/// `":"`; [`MidiError::CfgMalformedFlag`] for an unrecognized flag letter or
/// a non-numeric operand where a number is expected;
/// [`MidiError::CfgMissingVoiceGroup`] if the line never sets `-G`.
pub(super) fn parse_entry_for(text: &str, filename: &str) -> Result<MidiCfgEntry, MidiError> {
    let prefix = format!("{filename}:");
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with(&prefix))
        .ok_or_else(|| MidiError::CfgEntryMissing(filename.to_owned()))?;
    let rest = line
        .trim_start()
        .strip_prefix(&prefix)
        .expect("just matched this exact prefix")
        .trim();

    let mut entry = MidiCfgEntry::default();
    for token in rest.split_whitespace() {
        apply_flag(&mut entry, token)?;
    }
    if entry.voicegroup_label.is_empty() {
        return Err(MidiError::CfgMissingVoiceGroup);
    }
    Ok(entry)
}

#[cfg(test)]
mod tests;
