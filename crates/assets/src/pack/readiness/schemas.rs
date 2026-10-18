//! Validate supported payload schemas and their asset references, not game fidelity.

use std::collections::BTreeSet;

use crate::audio::{
    CryTable, Sample, SampleId, Song, VoiceEntry, VoiceGroup, VoiceGroupId, CRY_TABLE_PACK_ID,
};
use crate::{BattleAnimationCatalog, PokedexData};

use super::{invalid, AssetPack, EntryKind, PackProblem};

impl AssetPack {
    pub(super) fn validate_schemas(&self) -> Result<(), PackProblem> {
        let mut groups = BTreeSet::new();
        for entry in &self.entries {
            let id = entry.id.as_str();
            if let EntryKind::Image { bit_depth, .. } = entry.kind {
                let max_pixel = u8::MAX >> (8 - bit_depth);
                if self.payload(entry).iter().any(|&pixel| pixel > max_pixel) {
                    return Err(invalid(id, "pixel index exceeds the image bit depth"));
                }
            }
            if id.starts_with("tileset/") && id.ends_with("/metatile-attributes") {
                let bytes = self.raw(id).map_err(|err| invalid(id, err))?;
                if !bytes.len().is_multiple_of(2) {
                    return Err(invalid(id, "partial metatile attribute"));
                }
                for attribute in
                    crate::metatile_attributes::MetatileAttributeTable::new(bytes).attributes()
                {
                    attribute.map_err(|err| invalid(id, err))?;
                }
            }
            if id.starts_with("audio/song/")
                || id.starts_with("audio/voicegroup/")
                || id.starts_with("audio/sample/")
                || matches!(
                    id,
                    CRY_TABLE_PACK_ID
                        | "pokedex/data"
                        | "battle-anim/catalog"
                        | "battle-message/table"
                )
            {
                self.validate_payload(id, &mut groups)?;
            }
        }
        let mut checked = BTreeSet::new();
        while let Some(id) = groups.pop_first() {
            if checked.insert(id.clone()) {
                self.require_raw_reference(&id)?;
                let group = self
                    .voicegroup(&VoiceGroupId(id.clone()))
                    .map_err(|err| invalid(&id, err))?;
                self.validate_voice_references(&id, &group, &mut groups)?;
            }
        }
        self.validate_graphics_groups()?;
        Ok(())
    }

    fn validate_payload(&self, id: &str, groups: &mut BTreeSet<String>) -> Result<(), PackProblem> {
        let bytes = self.raw(id).map_err(|err| invalid(id, err))?;
        if id.starts_with("audio/song/") {
            let song = Song::decode(bytes).map_err(|err| invalid(id, err))?;
            self.require_raw_reference(&song.voicegroup().0)?;
            groups.insert(song.voicegroup().0.clone());
        } else if id.starts_with("audio/voicegroup/") {
            groups.insert(id.to_owned());
        } else if id.starts_with("audio/sample/") {
            Sample::decode(bytes).map_err(|err| invalid(id, err))?;
        } else if id == CRY_TABLE_PACK_ID {
            let table = CryTable::decode(bytes).map_err(|err| invalid(id, err))?;
            for sample in table.samples() {
                self.require_sample(id, sample, false)?;
            }
        } else if id == "pokedex/data" {
            PokedexData::decode(bytes).map_err(|err| invalid(id, err))?;
        } else if id == "battle-anim/catalog" {
            BattleAnimationCatalog::decode(bytes).map_err(|err| invalid(id, err))?;
        } else if id == "battle-message/table" {
            self.battle_messages().map_err(|err| invalid(id, err))?;
        }
        Ok(())
    }

    fn require_raw_reference(&self, id: &str) -> Result<(), PackProblem> {
        self.raw(id).map(|_| ()).map_err(|err| match err {
            super::super::PackError::UnknownAsset(_) => PackProblem::MissingAsset(id.to_owned()),
            other => invalid(id, other),
        })
    }

    fn require_sample(&self, owner: &str, id: &SampleId, wave: bool) -> Result<(), PackProblem> {
        self.require_raw_reference(&id.0)?;
        let sample = self.sample(id).map_err(|err| invalid(&id.0, err))?;
        if matches!(sample, Sample::ProgrammableWave(_)) != wave {
            return Err(invalid(
                owner,
                format!("sample {:?} has the wrong audio kind", id.0),
            ));
        }
        Ok(())
    }

    fn validate_voice_references(
        &self,
        id: &str,
        group: &VoiceGroup,
        groups: &mut BTreeSet<String>,
    ) -> Result<(), PackProblem> {
        for voice in group.slots() {
            match voice {
                VoiceEntry::DirectSound(voice) => self.require_sample(id, &voice.sample, false)?,
                VoiceEntry::ProgrammableWave(voice) => {
                    self.require_sample(id, &voice.wave, true)?;
                }
                VoiceEntry::KeySplit(voice) => {
                    groups.insert(voice.children.0.clone());
                }
                VoiceEntry::Rhythm(voice) => {
                    groups.insert(voice.children.0.clone());
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_graphics_groups(&self) -> Result<(), PackProblem> {
        let has = |prefix: &str| {
            self.entries
                .iter()
                .any(|entry| entry.id.starts_with(prefix))
        };
        let problem = |err| match err {
            super::super::PackError::UnknownAsset(id) => PackProblem::MissingAsset(id),
            other => invalid("graphics", other),
        };
        if has("starter-chooser/") {
            self.starter_chooser().map_err(problem)?;
        }
        if has("pokedex-graphics/") {
            self.pokedex_graphics().map_err(problem)?;
        }
        if has("battle-interface/") {
            self.battle_interface().map_err(problem)?;
        }
        Ok(())
    }
}
