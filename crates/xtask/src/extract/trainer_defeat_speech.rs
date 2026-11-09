//! Trainer defeat speeches (upstream `data/text/trainers.inc` and
//! `data/maps/Route103/scripts.inc`).
//!
//! Emits one raw pack entry, `trainer-defeat-speech/table`, in the schema
//! [`assets::pack::TrainerDefeatSpeechError`] guards: the defeat text of each
//! [`TrainerDefeatSpeechContext`]. The texts are assembler `.string` labels,
//! mapped to charmap bytes through the checkout's own `charmap.txt` the way
//! `tools/preproc` maps them. The `$` ends each string, so no terminator is
//! appended. Nothing is compiled or executed, and no other label in either
//! file is read into the pack.

use std::path::Path;

use assets::pack::TrainerDefeatSpeechError;
use assets::{TrainerDefeatSpeechContext, TrainerDefeatSpeechTable};
use pack_format::PackWriter;

use super::text_source::{parse_assembly_strings, Charmap, Fault};
use super::{read_text, ExtractError};

const PACK_ID: &str = "trainer-defeat-speech/table";
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

/// The file defining a context's label: the two rival texts are script-local.
const fn source_of(context: TrainerDefeatSpeechContext) -> File {
    match context {
        TrainerDefeatSpeechContext::Route103May | TrainerDefeatSpeechContext::Route103Brendan => {
            File::Route103
        }
        _ => File::Trainers,
    }
}

/// Extracts the trainer defeat-speech table from the checkout.
pub(super) fn extract_trainer_defeat_speech(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let charmap = read_text(&upstream.join(CHARMAP_PATH))?;
    let trainers = read_text(&upstream.join(TRAINERS_PATH))?;
    let route_103 = read_text(&upstream.join(ROUTE_103_PATH))?;
    let bytes = build(&charmap, &trainers, &route_103).map_err(|(file, fault)| {
        ExtractError::TrainerDefeatSpeechSource {
            path: upstream.join(file.path()),
            line: fault.line,
            detail: fault.detail,
        }
    })?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

fn build(charmap: &str, trainers: &str, route_103: &str) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(charmap).map_err(at(File::Charmap))?;
    let labels_in = |file: File| -> Vec<&'static str> {
        TrainerDefeatSpeechContext::ALL
            .into_iter()
            .filter(|context| source_of(*context) == file)
            .map(TrainerDefeatSpeechContext::symbol)
            .collect()
    };
    let from_trainers = parse_assembly_strings(trainers, &charmap, &labels_in(File::Trainers))
        .map_err(at(File::Trainers))?;
    let from_route_103 = parse_assembly_strings(route_103, &charmap, &labels_in(File::Route103))
        .map_err(at(File::Route103))?;

    let mut streams = Vec::with_capacity(TrainerDefeatSpeechContext::ALL.len());
    for context in TrainerDefeatSpeechContext::ALL {
        let source = if source_of(context) == File::Route103 {
            &from_route_103
        } else {
            &from_trainers
        };
        let stream = source
            .get(context.symbol())
            .expect("the parser returns every requested label");
        streams.push((context.id(), stream.as_slice()));
    }
    let table = TrainerDefeatSpeechTable::from_entries(streams).map_err(|e| {
        let file = match e {
            TrainerDefeatSpeechError::MalformedString { id, .. } => TrainerDefeatSpeechContext::ALL
                .get(usize::from(id))
                .map_or(File::Trainers, |context| source_of(*context)),
            _ => File::Trainers,
        };
        (file, Fault::new(0, e))
    })?;
    Ok(table.encode())
}

#[cfg(test)]
mod tests;
