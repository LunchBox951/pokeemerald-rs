//! Tests for the closure grammar.

use super::*;

#[test]
fn valid_closure_builds_the_payload() {
    let data = fixture().parse().unwrap();
    assert_eq!(data.entries().len(), 387);
    let bulbasaur = data.entry(NationalDexNo::new(1).unwrap()).unwrap();
    assert_eq!(bulbasaur.category, "SEED");
    assert_eq!(bulbasaur.description, "Line one\nline \"two\".");
    assert_eq!(bulbasaur.pokemon_scale, 356);
    assert_eq!(bulbasaur.pokemon_offset, -3);
    assert_eq!(bulbasaur.trainer_offset, 2);
    assert_eq!(data.orders().alphabetical().len(), 411);
    assert_eq!(data.orders().weight().len(), 386);
    assert_eq!(data.orders().height()[0].get(), 386);
    let round_trip = PokedexData::decode(&data.encode()).unwrap();
    assert_eq!(round_trip, data);
}
