use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, BattleRng, Dex, Ivs, Nature, MAX_IV};

pub struct SequenceRng {
    scripted_values: Vec<u16>,
    draw_count: usize,
}

impl SequenceRng {
    pub fn new(scripted_values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            scripted_values: scripted_values.into_iter().collect(),
            draw_count: 0,
        }
    }

    pub fn draws(&self) -> usize {
        self.draw_count
    }
}

impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let value = self
            .scripted_values
            .get(self.draw_count)
            .copied()
            .unwrap_or_else(|| panic!("SequenceRng exhausted after {} draws", self.draw_count));
        self.draw_count += 1;
        value
    }
}

pub const MAX_IVS: Ivs = Ivs {
    hp: MAX_IV,
    attack: MAX_IV,
    defense: MAX_IV,
    speed: MAX_IV,
    sp_attack: MAX_IV,
    sp_defense: MAX_IV,
};

pub fn max_iv_mon(
    dex: &Dex,
    species_id: u16,
    level: u8,
    known_moves: Vec<MoveId>,
) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        SpeciesId(species_id),
        level,
        MAX_IVS,
        u32::from(Nature::Hardy.id()),
        known_moves,
    )
    .unwrap()
}
