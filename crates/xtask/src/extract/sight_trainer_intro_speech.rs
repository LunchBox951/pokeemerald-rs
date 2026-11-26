//! Sight-trainer intro speeches (upstream `data/maps/Route103/scripts.inc` and
//! `data/text/trainers.inc`).
//!
//! Emits one raw pack entry, `sight-trainer-intro-speech/table`, in the schema
//! [`assets::pack::SightTrainerIntroSpeechError`] guards: the intro text of each
//! of Route 103's nine sight-trainer object-event scripts. Each script's first
//! command is a `trainerbattle_single` or `trainerbattle_double`, whose second
//! operand names the intro text; that association is read from the script, not
//! derived from the label's spelling. The texts are assembler `.string` labels,
//! mapped to charmap bytes through the checkout's own `charmap.txt` the way
//! `tools/preproc` maps them, and the `$` ends each string. Nothing is compiled
//! or executed, and no other script or label is read into the pack.

use std::path::Path;

use assets::pack::SightTrainerIntroSpeechError;
use assets::{
    SightTrainerIntroSpeechTable, SIGHT_TRAINER_INTRO_COUNT, SIGHT_TRAINER_INTRO_SCRIPTS,
};
use pack_format::PackWriter;

use super::text_source::{parse_assembly_strings, Charmap, Fault};
use super::{read_text, ExtractError};

const PACK_ID: &str = "sight-trainer-intro-speech/table";
const CHARMAP_PATH: &str = "charmap.txt";
const TRAINERS_PATH: &str = "data/text/trainers.inc";
const ROUTE_103_PATH: &str = "data/maps/Route103/scripts.inc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum File {
    Charmap,
    Trainers,
    Route103,
}

impl File {
    const fn path(self) -> &'static str {
        match self {
            Self::Charmap => CHARMAP_PATH,
            Self::Trainers => TRAINERS_PATH,
            Self::Route103 => ROUTE_103_PATH,
        }
    }
}

type Located<T> = Result<T, (File, Fault)>;

fn at(file: File) -> impl FnOnce(Fault) -> (File, Fault) {
    move |fault| (file, fault)
}

/// Extracts the sight-trainer intro-speech table from the checkout.
pub(super) fn extract_sight_trainer_intro_speech(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let charmap = read_text(&upstream.join(CHARMAP_PATH))?;
    let trainers = read_text(&upstream.join(TRAINERS_PATH))?;
    let route_103 = read_text(&upstream.join(ROUTE_103_PATH))?;
    let bytes = build(&charmap, &trainers, &route_103).map_err(|(file, fault)| {
        ExtractError::SightTrainerIntroSpeechSource {
            path: upstream.join(file.path()),
            line: fault.line,
            detail: fault.detail,
        }
    })?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

/// The intro-text label each inventory script names, in inventory order.
fn intro_labels(route_103: &str) -> Result<[&str; SIGHT_TRAINER_INTRO_COUNT], Fault> {
    let mut labels: [Option<&str>; SIGHT_TRAINER_INTRO_COUNT] = [None; SIGHT_TRAINER_INTRO_COUNT];
    let mut defined = [false; SIGHT_TRAINER_INTRO_COUNT];
    let mut current: Option<usize> = None;
    for (index, raw) in route_103.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.split('@').next().unwrap_or_default().trim();
        if let Some(label) = trimmed
            .strip_suffix("::")
            .or_else(|| trimmed.strip_suffix(':'))
        {
            current = SIGHT_TRAINER_INTRO_SCRIPTS
                .iter()
                .position(|script| *script == label);
            if let Some(position) = current {
                if std::mem::replace(&mut defined[position], true) {
                    return Err(Fault::new(line, format!("`{label}` is defined twice")));
                }
            }
            continue;
        }
        let Some(position) = current else { continue };
        if trimmed.is_empty() || labels[position].is_some() {
            continue;
        }
        labels[position] = Some(intro_operand(
            trimmed,
            SIGHT_TRAINER_INTRO_SCRIPTS[position],
            line,
        )?);
    }
    let mut out = [""; SIGHT_TRAINER_INTRO_COUNT];
    for (position, script) in SIGHT_TRAINER_INTRO_SCRIPTS.iter().enumerate() {
        if !defined[position] {
            return Err(Fault::new(0, format!("`{script}` is not defined")));
        }
        out[position] =
            labels[position].ok_or_else(|| Fault::new(0, format!("`{script}` has no command")))?;
    }
    Ok(out)
}

/// The intro-text operand of a script's first command, which must be a
/// `trainerbattle_single` (trainer, intro, defeat, ...) or `trainerbattle_double`
/// (trainer, intro, defeat, not-enough-Pokemon, ...).
fn intro_operand<'a>(command: &'a str, script: &str, line: usize) -> Result<&'a str, Fault> {
    let (name, operands) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    let (min, max) = match name {
        "trainerbattle_single" => (3, 5),
        "trainerbattle_double" => (4, 6),
        _ => {
            return Err(Fault::new(
                line,
                format!("`{script}` does not start with a supported trainerbattle command"),
            ))
        }
    };
    let operands: Vec<&str> = operands.split(',').map(str::trim).collect();
    let malformed = || Fault::new(line, format!("`{script}` has malformed `{name}` operands"));
    if !(min..=max).contains(&operands.len()) || operands.iter().any(|operand| operand.is_empty()) {
        return Err(malformed());
    }
    let intro = operands[1];
    if intro.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(intro)
    } else {
        Err(malformed())
    }
}

fn build(charmap: &str, trainers: &str, route_103: &str) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(charmap).map_err(at(File::Charmap))?;
    let labels = intro_labels(route_103).map_err(at(File::Route103))?;
    let texts = parse_assembly_strings(trainers, &charmap, &labels).map_err(at(File::Trainers))?;
    let streams = SIGHT_TRAINER_INTRO_SCRIPTS
        .into_iter()
        .zip(labels)
        .map(|(script, label)| {
            let stream = texts
                .get(label)
                .expect("the parser returns every requested label");
            (script, stream.as_slice())
        });
    let table = SightTrainerIntroSpeechTable::from_entries(streams)
        .map_err(|e: SightTrainerIntroSpeechError| (File::Trainers, Fault::new(0, e)))?;
    Ok(table.encode())
}

#[cfg(test)]
mod tests;
