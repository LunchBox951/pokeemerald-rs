//! Party codec and merge tests, split by concern.

// The retained-record bodies spell `super::encode_attacks`.
use super::encode_attacks;

mod active_selection;
mod codec;
mod common;
mod create_mon;
mod loaded_lead;
mod merge_hp;
mod merge_stats;
mod retained_record;
mod stats;

#[test]
fn slot_zero_synchronize_nature_reads_slot_zero_even_when_fainted() {
    let dex = battle::Dex::new();
    let ivs = battle::Ivs {
        hp: 0,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    };
    let mut mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        ivs,
        0,
        vec![assets::MoveId::POUND],
    )
    .unwrap();
    mew.apply_damage(u32::MAX);
    let sync = super::to_save_pokemon(&dex, &mew);
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[sync, plain]),
        Some(battle::Nature::Hardy)
    );
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[plain, sync]),
        None
    );
    assert_eq!(super::slot_zero_synchronize_nature(&dex, &[]), None);
}

#[test]
fn slot_zero_synchronize_nature_ignores_an_interior_empty_move() {
    let dex = battle::Dex::new();
    let mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        battle::Ivs::default(),
        0,
        vec![
            assets::MoveId::POUND,
            assets::MoveId::POUND,
            assets::MoveId::TACKLE,
        ],
    )
    .unwrap();
    let mut sync = super::to_save_pokemon(&dex, &mew);
    let mut subs = sync.box_data.substructures().unwrap();
    subs.attacks[2..4].fill(0);
    sync.box_data.set_substructures(&subs);
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    let party = [sync, plain];
    assert_eq!(super::select_active_battler(&dex, &party).unwrap().0, 1);
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &party),
        Some(battle::Nature::Hardy)
    );
}
