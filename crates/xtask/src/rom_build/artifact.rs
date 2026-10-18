//! The compiled-artifact report `rom-build` hands to downstream extraction.
//!
//! It carries bytes and linker facts as the upstream build emitted them. It
//! assigns no asset IDs and interprets no addresses: a consumer decides what a
//! symbol address means.

use std::fmt;

use crate::gen_rom_profile::SymbolMap;

/// Where a [`CompiledArtifact`] came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// The full pinned upstream revision the build tree was materialised at
    /// and verified against.
    pub pinned_revision: String,
    /// The program and arguments that ran the upstream build.
    pub build_command: String,
}

/// The outputs of one verified upstream build.
///
/// Private to the developer tooling: it holds the cartridge image, which is
/// never written into the repository.
pub struct CompiledArtifact {
    provenance: Provenance,
    rom: Vec<u8>,
    symbols: SymbolMap,
}

impl CompiledArtifact {
    pub(super) fn new(provenance: Provenance, rom: Vec<u8>, symbols: SymbolMap) -> Self {
        Self {
            provenance,
            rom,
            symbols,
        }
    }

    /// Where this artifact came from.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The cartridge image bytes the upstream `rom` rule emitted.
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// The linker map's symbols, indexed by exact start address.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read by the downstream extractor, not by the command"
        )
    )]
    pub fn symbols(&self) -> &SymbolMap {
        &self.symbols
    }
}

impl fmt::Debug for CompiledArtifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The image is deliberately not printed.
        f.debug_struct("CompiledArtifact")
            .field("provenance", &self.provenance)
            .field("rom_len", &self.rom.len())
            .field("symbols", &self.symbols)
            .finish()
    }
}
