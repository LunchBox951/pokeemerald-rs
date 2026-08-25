//! A tolerant reader for GNU `ld`'s `.map` output.
//!
//! Optional, and deliberately so: nobody needs a built decomp to generate a
//! profile, and the generator's own signature matching is the primary
//! evidence. A map turns that evidence into a second, independent one. If
//! the owner ever builds `pokeemerald` with `agbcc`, `--map` says whether
//! every address the generator derived is a symbol's own address rather
//! than a coincidence somewhere inside one.
//!
//! The check runs by address, not by name. Making it by name would mean
//! carrying a table of a few hundred upstream symbol names in the
//! generator, which is exactly the hand-maintained knowledge this whole
//! design exists to avoid. Instead each generated address is looked up in
//! the map, and the symbols found there are reported.
//!
//! The parser only recognises the one line shape that matters -- an address
//! and a name, alone on a line -- and ignores everything else, because the
//! map's other sections vary between `ld` versions and none of them carry
//! symbol addresses.

use std::collections::BTreeMap;
use std::path::Path;

use super::error::GenRomProfileError;

/// Every symbol a map file names, indexed by address.
#[derive(Debug, Default, Clone)]
pub struct SymbolMap {
    by_addr: BTreeMap<u32, Vec<String>>,
}

impl SymbolMap {
    /// Read and parse the map at `path`.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::MapUnreadable`] if the file cannot be read, or
    /// if it names no symbol at all (which means the shape assumed here is
    /// not the shape the file has, and a silent pass would be worse than a
    /// failure).
    pub fn load(path: &Path) -> Result<Self, GenRomProfileError> {
        let text =
            std::fs::read_to_string(path).map_err(|err| GenRomProfileError::MapUnreadable {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        let parsed = Self::parse(&text);
        if parsed.is_empty() {
            return Err(GenRomProfileError::MapUnreadable {
                path: path.to_path_buf(),
                reason: "no `0x<address> <symbol>` lines found".to_owned(),
            });
        }
        Ok(parsed)
    }

    /// Parse map text.
    pub fn parse(text: &str) -> Self {
        let mut by_addr: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for line in text.lines() {
            let Some((addr, name)) = symbol_line(line) else {
                continue;
            };
            by_addr.entry(addr).or_default().push(name.to_owned());
        }
        for names in by_addr.values_mut() {
            names.sort();
            names.dedup();
        }
        Self { by_addr }
    }

    /// Whether the map names nothing.
    pub fn is_empty(&self) -> bool {
        self.by_addr.is_empty()
    }

    /// The symbols defined at `addr`, if any.
    pub fn symbols_at(&self, addr: u32) -> &[String] {
        self.by_addr.get(&addr).map_or(&[], Vec::as_slice)
    }
}

/// Recognise a line that is exactly an address and a symbol name.
fn symbol_line(line: &str) -> Option<(u32, &str)> {
    let mut tokens = line.split_whitespace();
    let addr = tokens.next()?;
    let name = tokens.next()?;
    if tokens.next().is_some() {
        return None;
    }
    let digits = addr.strip_prefix("0x")?;
    // `ld` writes 64-bit addresses on some hosts; the low 32 bits are the
    // cartridge address either way.
    let value = u64::from_str_radix(digits, 16).ok()?;
    if !is_symbol_name(name) {
        return None;
    }
    u32::try_from(value & 0xFFFF_FFFF)
        .ok()
        .map(|addr| (addr, name))
}

/// Whether `name` looks like a linker symbol rather than a section or a
/// fragment of some other line.
fn is_symbol_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'))
}

#[cfg(test)]
mod tests {
    use super::SymbolMap;

    /// A synthetic excerpt in the shapes `ld` actually emits: a section
    /// line, a 64-bit-formatted symbol line, a 32-bit one, an aliased
    /// address, and lines that must be ignored.
    const EXCERPT: &str = "\
Memory Configuration

 .rodata.gTileset_General
                0x00000000083df704       0x18 build/emerald/src/data/tilesets/headers.o
                0x00000000083df704                gTileset_General
                0x083df71c                gTileset_Petalburg
                0x083df71c                gTilesetAlias_Petalburg
 *(.rodata)
                0x0864c2e4       0x8000 build/emerald/src/graphics.o
 LOAD build/emerald/src/main.o
                0x08000000                . = ALIGN (0x4)
";

    #[test]
    fn symbol_lines_are_recognised_in_both_address_widths() {
        let map = SymbolMap::parse(EXCERPT);
        assert_eq!(map.symbols_at(0x083D_F704), ["gTileset_General"]);
        assert_eq!(
            map.symbols_at(0x083D_F71C),
            ["gTilesetAlias_Petalburg", "gTileset_Petalburg"]
        );
        assert_eq!(map.symbols_at(0x0000_0000).len(), 0);
    }

    #[test]
    fn non_symbol_lines_are_ignored() {
        let map = SymbolMap::parse(EXCERPT);
        // A section header with a size and an object file has three tokens
        // after the address, so it is not a symbol.
        assert!(map.symbols_at(0x0864_C2E4).is_empty());
        // `. = ALIGN (0x4)` is not a name.
        assert!(map.symbols_at(0x0800_0000).is_empty());
    }

    #[test]
    fn an_address_with_no_symbol_reports_nothing() {
        let map = SymbolMap::parse(EXCERPT);
        assert!(map.symbols_at(0x0812_3456).is_empty());
        assert!(!map.is_empty());
    }

    #[test]
    fn a_map_with_no_symbols_parses_to_nothing() {
        let map = SymbolMap::parse("Memory Configuration\nName Origin Length\n");
        assert!(map.is_empty());
    }
}
