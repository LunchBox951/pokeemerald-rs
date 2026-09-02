use crate::damage::BattleRng;

pub(crate) struct SequenceRng {
    scripted_values: Vec<u16>,
    draw_count: usize,
}

impl SequenceRng {
    pub(crate) fn new(scripted_values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            scripted_values: scripted_values.into_iter().collect(),
            draw_count: 0,
        }
    }

    pub(crate) fn draws(&self) -> usize {
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
