//! Runtime requirements from the supported importer's compiled asset inventory.
//! Addresses and ROM bytes are never read while checking an installed pack.

use std::path::Path;

use assets::pack::{PackReadinessError, RequiredAsset};
use assets::{AssetPack, EntryKind};
use rom_import::{BlobRoot, ImageRoot, PaletteRoot, Roots};

/// Load the selected pack only if its format and game content are compatible.
///
/// Application release versions do not affect compatibility. Extra entries
/// are allowed and supported payload schemas are checked throughout the pack.
///
/// # Errors
/// Returns the first [`PackReadinessError`] with the pack path and extraction
/// guidance. The pack and saved game remain untouched.
pub fn load_ready_pack(path: &Path) -> Result<AssetPack, PackReadinessError> {
    AssetPack::load_ready(path, &Requirements::for_game().0)
}

struct Requirements(Vec<RequiredAsset>);

impl Requirements {
    fn for_game() -> Self {
        let mut required = Self(Vec::new());
        required.add_roots(&rom_import::EMERALD_US_REV0.roots);
        required
    }

    fn add_roots(&mut self, roots: &Roots) {
        for image in roots
            .title_screen
            .images
            .iter()
            .chain(roots.sprites.sheets)
            .chain(roots.text_window.images)
            .chain(roots.bag.images)
        {
            self.image(image);
        }
        for palette in roots
            .title_screen
            .palettes
            .iter()
            .chain(roots.sprites.palettes)
            .chain(roots.text_window.palettes)
            .chain(roots.interface.palettes)
            .chain(roots.bag.palettes)
        {
            self.palette(palette);
        }
        for blob in roots.title_screen.tilemaps.iter().chain(roots.bag.raws) {
            self.blob(blob);
        }
        for tileset in roots.tilesets {
            self.image(&tileset.tiles);
            for palette in tileset.palettes {
                self.palette(palette);
            }
            self.blob(&tileset.metatiles);
            self.blob(&tileset.metatile_attributes);
            for animation in tileset.anims {
                for frame in animation.frames {
                    self.image(frame);
                }
            }
        }
        for layout in roots.layouts {
            self.blob(&layout.map);
            self.blob(&layout.border);
        }
        for font in roots.fonts {
            self.0.push(RequiredAsset {
                id: font.id.into(),
                kind: EntryKind::Image {
                    width: font.width,
                    height: font.height,
                    bit_depth: font.bit_depth,
                },
                byte_len: None,
            });
        }
        for battler in roots.battlers.species {
            self.image(&battler.front);
            self.image(&battler.back);
            self.palette(&battler.normal);
            self.palette(&battler.shiny);
        }
        for song in roots.audio.songs {
            self.raw(song.id);
        }
        for voicegroup in roots.audio.voicegroups {
            self.raw(voicegroup.id);
        }
        for sample in roots
            .audio
            .direct_sound
            .iter()
            .chain(roots.audio.programmable_wave)
        {
            self.raw(sample.id);
        }
        self.raw(rom_import::BATTLE_MESSAGE_PACK_ID);
    }

    fn image(&mut self, root: &ImageRoot) {
        self.0.push(RequiredAsset {
            id: root.id.into(),
            kind: EntryKind::Image {
                width: root.width,
                height: root.height,
                bit_depth: root.pack_bit_depth,
            },
            byte_len: None,
        });
    }

    fn palette(&mut self, root: &PaletteRoot) {
        self.0.push(RequiredAsset {
            id: root.id.into(),
            kind: EntryKind::Palette {
                color_count: root.color_count,
            },
            byte_len: None,
        });
    }

    fn blob(&mut self, root: &BlobRoot) {
        self.0.push(RequiredAsset {
            id: root.id.into(),
            kind: EntryKind::Raw,
            byte_len: Some(u64::from(root.len)),
        });
    }

    fn raw(&mut self, id: &str) {
        self.0.push(RequiredAsset {
            id: id.into(),
            kind: EntryKind::Raw,
            byte_len: None,
        });
    }
}

#[cfg(test)]
mod tests;
