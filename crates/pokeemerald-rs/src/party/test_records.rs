//! Builders for party records the screen tests drive.

use assets::items::ItemId;
use engine::save::{Pokemon, PokemonSubstructures};

use super::records::{PartyRecords, PARTY_SIZE};

const SPECIES: std::ops::Range<usize> = 0..2;
const HELD_ITEM: std::ops::Range<usize> = 2..4;
const PP_BONUSES: usize = 8;
const IV_WORD: std::ops::Range<usize> = 4..8;
const EGG_BIT: u32 = 1 << 30;

/// A record under construction: a species, four moves, and the party fields.
pub(crate) struct Mon {
    record: Pokemon,
    substructures: PokemonSubstructures,
}

impl Mon {
    /// A healthy level-5 record of `species` with a personality unique to it.
    pub(crate) fn new(species: u16) -> Self {
        let mut substructures = PokemonSubstructures::default();
        substructures.growth[SPECIES].copy_from_slice(&species.to_le_bytes());
        let record = Pokemon {
            box_data: engine::save::BoxPokemon::new(0x1000 + u32::from(species), 0x1234_5678),
            level: 5,
            hp: 20,
            max_hp: 20,
            ..Pokemon::default()
        };
        Self {
            record,
            substructures,
        }
    }

    pub(crate) fn moves(mut self, ids: [u16; 4], pp: [u8; 4]) -> Self {
        for (slot, id) in ids.iter().enumerate() {
            self.substructures.attacks[slot * 2..slot * 2 + 2].copy_from_slice(&id.to_le_bytes());
        }
        self.substructures.attacks[8..12].copy_from_slice(&pp);
        self
    }

    pub(crate) fn pp_bonuses(mut self, bonuses: u8) -> Self {
        self.substructures.growth[PP_BONUSES] = bonuses;
        self
    }

    pub(crate) fn held(mut self, item: u16) -> Self {
        self.substructures.growth[HELD_ITEM].copy_from_slice(&ItemId(item).0.to_le_bytes());
        self
    }

    pub(crate) fn egg(mut self) -> Self {
        let word = u32::from_le_bytes(self.substructures.misc[IV_WORD].try_into().unwrap());
        self.substructures.misc[IV_WORD].copy_from_slice(&(word | EGG_BIT).to_le_bytes());
        self
    }

    pub(crate) fn fainted(mut self) -> Self {
        self.record.hp = 0;
        self
    }

    pub(crate) fn build(mut self) -> Pokemon {
        self.record.box_data.set_substructures(&self.substructures);
        self.record
    }
}

/// A party of `mons`; the rest of the six records stay default.
pub(crate) fn party(mons: Vec<Mon>) -> PartyRecords {
    let count = mons.len();
    let mut slots = [Pokemon::default(); PARTY_SIZE];
    for (slot, mon) in slots.iter_mut().zip(mons) {
        *slot = mon.build();
    }
    PartyRecords::new(&slots, count).expect("a test party fits six records")
}

/// A pad that keeps its button state between frames, as the platform does.
pub(crate) struct Pad {
    state: platform::ButtonState,
}

impl Pad {
    pub(crate) fn new() -> Self {
        Self {
            state: platform::ButtonState::new(),
        }
    }

    /// The state for a frame in which exactly `held` is down.
    pub(crate) fn frame(&mut self, held: platform::Buttons) -> platform::ButtonState {
        self.state.update(held);
        self.state
    }
}
