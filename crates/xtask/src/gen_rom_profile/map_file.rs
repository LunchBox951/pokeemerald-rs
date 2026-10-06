use std::collections::BTreeMap;
use std::path::Path;

use super::error::GenRomProfileError;

/// Symbol names indexed by exact start address.
#[derive(Debug, Default, Clone)]
pub struct SymbolMap {
    by_addr: BTreeMap<u32, Vec<String>>,
}

impl SymbolMap {
    /// Loads a map containing at least one supported symbol line.
    ///
    /// # Errors
    ///
    /// [`GenRomProfileError::MapUnreadable`] if the file cannot be read as text
    /// or contains no supported symbol lines.
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

    /// Accepts standalone `0x<address> <symbol>` lines whose addresses fit `u32`.
    /// Unsupported lines are ignored; an input with no symbols yields an empty map.
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

    /// Whether no symbol was parsed.
    pub fn is_empty(&self) -> bool {
        self.by_addr.is_empty()
    }

    /// Returns sorted, distinct symbol names starting exactly at `addr`.
    /// Returns an empty slice if no symbol starts there.
    pub fn symbols_at(&self, addr: u32) -> &[String] {
        self.by_addr.get(&addr).map_or(&[], Vec::as_slice)
    }
}

fn symbol_line(line: &str) -> Option<(u32, &str)> {
    let mut tokens = line.split_whitespace();
    let addr = tokens.next()?;
    let name = tokens.next()?;
    if tokens.next().is_some() {
        return None;
    }
    let digits = addr.strip_prefix("0x")?;
    let value = u64::from_str_radix(digits, 16).ok()?;
    if !is_symbol_name(name) {
        return None;
    }
    u32::try_from(value).ok().map(|addr| (addr, name))
}

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

    const SYNTHETIC_LD_MAP_EXCERPT: &str = "\
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
        let map = SymbolMap::parse(SYNTHETIC_LD_MAP_EXCERPT);
        assert_eq!(map.symbols_at(0x083D_F704), ["gTileset_General"]);
        assert_eq!(
            map.symbols_at(0x083D_F71C),
            ["gTilesetAlias_Petalburg", "gTileset_Petalburg"]
        );
        assert_eq!(map.symbols_at(0x0000_0000).len(), 0);
    }

    #[test]
    fn an_address_with_nonzero_high_bits_is_rejected_not_masked() {
        let map = SymbolMap::parse(
            "                0x00000001083df704                gBogusOutOfRange\n",
        );
        assert!(map.symbols_at(0x083D_F704).is_empty());
        assert!(map.is_empty());
    }

    #[test]
    fn non_symbol_lines_are_ignored() {
        let map = SymbolMap::parse(SYNTHETIC_LD_MAP_EXCERPT);
        assert!(map.symbols_at(0x0864_C2E4).is_empty());
        assert!(map.symbols_at(0x0800_0000).is_empty());
    }

    #[test]
    fn an_address_with_no_symbol_reports_nothing() {
        let map = SymbolMap::parse(SYNTHETIC_LD_MAP_EXCERPT);
        assert!(map.symbols_at(0x0812_3456).is_empty());
        assert!(!map.is_empty());
    }

    #[test]
    fn a_map_with_no_symbols_parses_to_nothing() {
        let map = SymbolMap::parse("Memory Configuration\nName Origin Length\n");
        assert!(map.is_empty());
    }
}
