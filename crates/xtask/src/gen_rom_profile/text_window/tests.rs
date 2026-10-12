//! The text-window palette table is accepted only in the importer's order.

use rom_import::fixture::RomFixture;

use super::{locate_window_palettes, WINDOW_BANKS};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::PalettePlan;
use crate::gen_rom_profile::tests::with_context;

const TABLE_OFFSET: usize = 0x50_0000;
const TABLE_ADDR: u32 = 0x0850_0000;
const BANK_BYTES: u32 = 32;
const BANK_LEN: usize = 32;

fn bank_bytes(seed: u8) -> Vec<u8> {
    (0..32u8).map(|byte| byte ^ seed).collect()
}

/// A ROM holding the message box's bank followed by four distinct banks.
fn table_rom() -> Vec<u8> {
    let mut fixture = RomFixture::new().emerald_header();
    for bank in 0..5u8 {
        let offset = TABLE_OFFSET + usize::from(bank) * BANK_LEN;
        let seed = if bank == 0 {
            0
        } else {
            bank.wrapping_mul(0x11)
        };
        fixture = fixture.write(offset, &bank_bytes(seed));
    }
    fixture.finish()
}

/// Plans placing `text_pal1..4` at the given table bank indices.
fn plans(banks: [u32; 4]) -> Vec<PalettePlan> {
    WINDOW_BANKS
        .iter()
        .zip(banks)
        .map(|(id, bank)| PalettePlan {
            id: (*id).to_owned(),
            addr: TABLE_ADDR + bank * BANK_BYTES,
            color_count: 16,
        })
        .collect()
}

fn locate(name: &str, banks: [u32; 4]) -> Result<u32, GenRomProfileError> {
    with_context(name, &table_rom(), Vec::new(), |ctx| {
        locate_window_palettes(ctx, &plans(banks), BANK_BYTES, &bank_bytes(0))
    })
}

#[test]
fn banks_in_table_order_derive_the_table() {
    assert_eq!(
        locate("tw-ordered", [1, 2, 3, 4]).expect("ordered"),
        TABLE_ADDR
    );
}

#[test]
fn swapped_second_and_third_banks_are_refused_by_name() {
    match locate("tw-swapped", [1, 3, 2, 4]).unwrap_err() {
        GenRomProfileError::StructMismatch { id, .. } => {
            assert_eq!(id, "text-window/palette/text_pal2");
        }
        other => panic!("expected StructMismatch, got {other}"),
    }
}
