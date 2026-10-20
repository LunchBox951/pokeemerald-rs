//! The battle-animation catalog: `battle-anim/catalog`.
//!
//! `data/battle_anim_scripts.s` assembles to command streams the interpreter
//! walks byte by byte (`src/battle_anim.c:118-171`). The ROM holds the four
//! selector tables as pointer arrays, each pointing at a stream. This reader
//! decodes every program the profile records from its address through its
//! first terminator, resolves the template, task, and branch operands the
//! profile names, and hands the lot to [`BattleAnimationCatalog`], which
//! validates the whole and owns the canonical encoding, so the checkout
//! extractor and this reader cannot drift on the wire format.
//!
//! A program ends at its first command that never falls through, wherever
//! that falls: bytes after it belong to the next label, not to this program.

mod command;

use assets::{
    AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog, BattleAnimationProgram,
    ProgramId,
};
use pack_format::{raw_entry, PackWriter};

use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{BattleAnimationRoots, Roots};
pub use command::{decode_program as decode_battle_anim_program, AnimPointers, DecodedProgram};

/// The pack id this domain produces.
pub const PACK_ID: &str = "battle-anim/catalog";
/// One selector-table entry is a `const u8 *`.
const POINTER_BYTES: usize = 4;

/// Write the battle-animation catalog, if the profile records one.
///
/// A profile with no battle-animation roots writes nothing, like any other
/// domain with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// As [`decode_battle_anim_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.battle_anims;
    if *family == BattleAnimationRoots::NONE {
        return Ok(());
    }
    let payload = decode_battle_anim_payload(&rom.reader(), family)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `battle-anim/catalog` payload one family of roots yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack, as it does for
/// battle messages.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the roots are not a consistent family
/// (a selector count that is not the tables', a selector or symbol that
/// names nothing, a repeated address, a template or task name bound twice),
/// a table slot does not point at its program, or a program's terminator is
/// not where its root says;
/// [`ImportError::PointerOutOfRange`], [`ImportError::Truncated`],
/// [`ImportError::AnimationOpcode`], or [`ImportError::AnimationPointer`] if
/// a stream cannot be followed; [`ImportError::BattleAnimation`] if the
/// schema rejects what the ROM yields.
pub fn decode_battle_anim_payload(
    reader: &RomReader<'_>,
    family: &BattleAnimationRoots,
) -> Result<Vec<u8>, ImportError> {
    let lookup = Lookup::new(family)?;
    let mut programs = Vec::with_capacity(family.programs.len());
    for root in family.programs {
        let decoded = command::decode_program(
            reader,
            root.addr,
            usize::from(root.len),
            &mut lookup.pointers(),
        )?;
        if decoded.len != usize::from(root.len) {
            return Err(mismatch("program length"));
        }
        programs.push(
            AnimSymbol::new(root.symbol)
                .and_then(|symbol| BattleAnimationProgram::new(symbol, decoded.commands))
                .map_err(schema)?,
        );
    }

    let mut selectors = Vec::with_capacity(family.selectors.len());
    let mut slots = family.selectors.iter();
    for (table, base) in AnimationTable::ALL.into_iter().zip(family.tables) {
        for index in 0..table.len() {
            let program = *slots.next().ok_or_else(|| mismatch("selector count"))?;
            let root = family
                .programs
                .get(usize::from(program))
                .ok_or_else(|| mismatch("selector program"))?;
            let entry = base
                .offset()
                .saturating_add(usize::from(index).saturating_mul(POINTER_BYTES));
            if reader.ptr(entry)? != root.addr {
                return Err(mismatch("selector table"));
            }
            selectors.push((
                AnimationSelector::new(table, index).map_err(schema)?,
                ProgramId::new(program),
            ));
        }
    }
    BattleAnimationCatalog::new(programs, selectors)
        .map(|catalog| catalog.encode())
        .map_err(schema)
}

fn mismatch(field: &'static str) -> ImportError {
    ImportError::StructMismatch {
        root: PACK_ID,
        field,
    }
}

fn schema(source: assets::BattleAnimationError) -> ImportError {
    ImportError::BattleAnimation {
        id: PACK_ID,
        source,
    }
}

/// The addresses a family's operands resolve through.
struct Lookup {
    programs: Vec<(u32, ProgramId)>,
    templates: Vec<(u32, AnimSymbol)>,
    tasks: Vec<(u32, AnimSymbol)>,
}

impl Lookup {
    /// Index the family, refusing roots that cannot resolve an operand to one
    /// identity.
    fn new(family: &BattleAnimationRoots) -> Result<Self, ImportError> {
        let selector_count: usize = AnimationTable::ALL
            .iter()
            .map(|table| usize::from(table.len()))
            .sum();
        if family.selectors.len() != selector_count {
            return Err(mismatch("selector count"));
        }
        let mut programs: Vec<(u32, ProgramId)> = Vec::with_capacity(family.programs.len());
        for (position, root) in family.programs.iter().enumerate() {
            let id = u16::try_from(position)
                .map(ProgramId::new)
                .map_err(|_| mismatch("program count"))?;
            // Labels sharing an address read the same stream, so a branch to
            // it resolves to the first.
            if !programs.iter().any(|(addr, _)| *addr == root.addr.raw()) {
                programs.push((root.addr.raw(), id));
            }
        }
        programs.sort_unstable_by_key(|(addr, _)| *addr);
        Ok(Self {
            programs,
            templates: symbols(family.templates, "template roots")?,
            tasks: symbols(family.tasks, "task roots")?,
        })
    }

    fn pointers(&self) -> Resolver<'_> {
        Resolver { lookup: self }
    }
}

fn symbols(
    roots: &[crate::roots::AnimSymbolRoot],
    field: &'static str,
) -> Result<Vec<(u32, AnimSymbol)>, ImportError> {
    let mut found: Vec<(u32, AnimSymbol)> = roots
        .iter()
        .map(|root| {
            Ok((
                root.addr.raw(),
                AnimSymbol::new(root.symbol).map_err(schema)?,
            ))
        })
        .collect::<Result<_, ImportError>>()?;
    // One address names one symbol, and one symbol has one address: a name
    // bound twice would resolve both addresses to it and hide the error.
    let mut names: Vec<&str> = roots.iter().map(|root| root.symbol).collect();
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(mismatch(field));
    }
    found.sort_unstable_by_key(|(addr, _)| *addr);
    if found.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(mismatch(field));
    }
    Ok(found)
}

/// Resolves operand addresses through a [`Lookup`].
struct Resolver<'a> {
    lookup: &'a Lookup,
}

impl AnimPointers for Resolver<'_> {
    fn template(&mut self, at: usize, ptr: GbaPtr) -> Result<AnimSymbol, ImportError> {
        find(&self.lookup.templates, ptr)
            .cloned()
            .ok_or(ImportError::AnimationPointer {
                at,
                what: "a template",
                ptr,
            })
    }

    fn task(&mut self, at: usize, ptr: GbaPtr) -> Result<AnimSymbol, ImportError> {
        find(&self.lookup.tasks, ptr)
            .cloned()
            .ok_or(ImportError::AnimationPointer {
                at,
                what: "a task",
                ptr,
            })
    }

    fn program(&mut self, at: usize, ptr: GbaPtr) -> Result<ProgramId, ImportError> {
        find(&self.lookup.programs, ptr)
            .copied()
            .ok_or(ImportError::AnimationPointer {
                at,
                what: "a program",
                ptr,
            })
    }
}

/// The value recorded at `ptr`, in a list sorted by address.
fn find<T>(sorted: &[(u32, T)], ptr: GbaPtr) -> Option<&T> {
    sorted
        .binary_search_by_key(&ptr.raw(), |(addr, _)| *addr)
        .ok()
        .map(|index| &sorted[index].1)
}

#[cfg(test)]
mod tests;
