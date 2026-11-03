//! Battle-animation catalog (upstream `data/battle_anim_scripts.s`, S-4).
//!
//! Emits one raw pack entry, `battle-anim/catalog`, in the schema
//! [`assets::BattleAnimationCatalog`] encodes: the four selector tables
//! (`gBattleAnims_Moves`, `gBattleAnims_StatusConditions`,
//! `gBattleAnims_General`, `gBattleAnims_Special`) in source slot order and
//! every labelled program they reach, directly or through `call`, `goto`, and
//! the conditional jumps. Nothing is assembled or executed.
//!
//! The script is read the way the assembler reads it: the wrapper macros of
//! `asm/macros/battle_anim_script.inc` are expanded textually, and operands
//! are integer expressions over the constants of the headers the script
//! includes (`.ifdef UBFIX` is false, as in a stock build).
//!
//! A program is the commands from its label through the first `end`, `return`,
//! `goto`, or `choosetwoturnanim`, wherever that falls: execution runs through
//! an intervening label, and commands after the terminator are discarded.

mod commands;
mod expr;
mod macros;
mod script;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use assets::{
    AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog, BattleAnimationProgram,
    ProgramId,
};
use pack_format::PackWriter;

use super::{read_text, ExtractError};
use expr::Constants;
use macros::Macros;
use script::Script;

const PACK_ID: &str = "battle-anim/catalog";
const SCRIPT_PATH: &str = "data/battle_anim_scripts.s";
const MACROS_PATH: &str = "asm/macros/battle_anim_script.inc";
/// The headers the script includes, which define every constant it names.
const HEADER_PATHS: [&str; 5] = [
    "include/constants/battle.h",
    "include/constants/battle_anim.h",
    "include/constants/rgb.h",
    "include/constants/songs.h",
    "include/constants/sound.h",
];
/// The assembler constants the script and its macros name (`TRUE`, `BLDCNT_*`).
const ASSEMBLER_PATHS: [&str; 2] = ["constants/global.inc", "constants/gba_constants.inc"];

/// A parse failure inside one source file.
#[derive(Debug)]
struct Fault {
    file: String,
    line: usize,
    detail: String,
}

impl Fault {
    fn new(file: &str, line: usize, detail: String) -> Self {
        Self {
            file: file.to_owned(),
            line,
            detail,
        }
    }
}

/// The upstream sources the catalog is built from, as `(path, text)`.
struct Sources<'a> {
    script: &'a str,
    macros: &'a str,
    headers: Vec<(&'a str, &'a str)>,
    assembler: Vec<(&'a str, &'a str)>,
}

/// Extracts the battle-animation catalog from the checkout.
///
/// Returns the catalog it queued, so a later extractor takes its inventory from
/// the same programs without parsing the script twice.
pub(super) fn extract_battle_anim(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<BattleAnimationCatalog, ExtractError> {
    let read = |paths: &[&'static str]| -> Result<Vec<(&'static str, String)>, ExtractError> {
        paths
            .iter()
            .map(|path| Ok((*path, read_text(&upstream.join(path))?)))
            .collect()
    };
    let script = read_text(&upstream.join(SCRIPT_PATH))?;
    let macros = read_text(&upstream.join(MACROS_PATH))?;
    let headers = read(&HEADER_PATHS)?;
    let assembler = read(&ASSEMBLER_PATHS)?;
    let catalog = build(&Sources {
        script: &script,
        macros: &macros,
        headers: borrowed(&headers),
        assembler: borrowed(&assembler),
    })
    .map_err(|fault| ExtractError::BattleAnimationSource {
        path: upstream.join(fault.file),
        line: fault.line,
        detail: fault.detail,
    })?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), catalog.encode()));
    Ok(catalog)
}

fn borrowed<'a>(files: &'a [(&'static str, String)]) -> Vec<(&'a str, &'a str)> {
    files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect()
}

fn build(src: &Sources<'_>) -> Result<BattleAnimationCatalog, Fault> {
    let mut constants = Constants::default();
    for (path, text) in &src.headers {
        constants.load_header(path, text)?;
    }
    for (path, text) in &src.assembler {
        constants.load_assembler(path, text)?;
    }
    let macros = Macros::parse(MACROS_PATH, src.macros)?;
    let script = Script::parse(SCRIPT_PATH, src.script, &constants, &macros)?;
    link(&script).map_err(|(line, detail)| Fault::new(SCRIPT_PATH, line, detail))
}

/// Resolves the reachable programs and builds the catalog.
fn link(script: &Script) -> Result<BattleAnimationCatalog, (usize, String)> {
    script.check_targets()?;
    let mut reachable = BTreeSet::new();
    let mut queue: Vec<usize> = Vec::new();
    for table in AnimationTable::ALL {
        for (label, line) in script.table(table) {
            queue.push(script.label_index(label, *line)?);
        }
    }
    while let Some(label) = queue.pop() {
        if !reachable.insert(label) {
            continue;
        }
        for item in script.program(label)? {
            for target in &item.pending.targets {
                queue.push(script.label_index(target, item.line)?);
            }
        }
    }

    let ids: HashMap<usize, ProgramId> = reachable
        .iter()
        .enumerate()
        .map(|(position, label)| {
            let id = u16::try_from(position).map_err(|_| (0, "too many programs".to_owned()))?;
            Ok((*label, ProgramId::new(id)))
        })
        .collect::<Result<_, (usize, String)>>()?;

    let mut programs = Vec::with_capacity(reachable.len());
    for &label in &reachable {
        let (name, line) = script.label(label);
        let commands = script
            .program(label)?
            .iter()
            .map(|item| {
                let targets = item
                    .pending
                    .targets
                    .iter()
                    .map(|t| script.label_index(t, item.line).map(|index| ids[&index]))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(item.pending.link(&targets))
            })
            .collect::<Result<Vec<_>, (usize, String)>>()?;
        let symbol = AnimSymbol::new(name).map_err(|e| (line, e.to_string()))?;
        programs.push(
            BattleAnimationProgram::new(symbol, commands)
                .map_err(|e| (line, format!("program `{name}`: {e}")))?,
        );
    }

    let mut selectors = Vec::new();
    for table in AnimationTable::ALL {
        for (index, (label, line)) in (0..).zip(script.table(table)) {
            let selector =
                AnimationSelector::new(table, index).map_err(|e| (*line, e.to_string()))?;
            selectors.push((selector, ids[&script.label_index(label, *line)?]));
        }
    }
    BattleAnimationCatalog::new(programs, selectors).map_err(|e| (0, e.to_string()))
}
