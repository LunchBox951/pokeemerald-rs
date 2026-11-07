//! The emitted `summary_moves` root family.

use std::fmt::Write as _;

use super::gba_ptr_expression;
use crate::gen_rom_profile::plan::SummaryMovePlan;

/// Writes the `summary_moves: SummaryMoveRoots { .. }` field of the root table.
pub(super) fn summary_move_roots(out: &mut String, moves: &SummaryMovePlan) {
    let _ = writeln!(out, "    summary_moves: SummaryMoveRoots {{");
    for (field, addr) in [
        ("descriptions", moves.descriptions),
        ("contest_moves", moves.contest_moves),
        ("contest_effects", moves.contest_effects),
        ("effect_descriptions", moves.effect_descriptions),
    ] {
        let _ = writeln!(out, "        {field}: {},", gba_ptr_expression(addr));
    }
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
mod tests {
    use super::summary_move_roots;
    use crate::gen_rom_profile::plan::SummaryMovePlan;

    #[test]
    fn the_four_tables_render_one_address_per_line() {
        let mut out = String::new();
        summary_move_roots(
            &mut out,
            &SummaryMovePlan {
                descriptions: 0x0800_1000,
                contest_moves: 0x0800_2000,
                contest_effects: 0x0800_3000,
                effect_descriptions: 0x0800_4000,
            },
        );
        assert_eq!(
            out,
            "    summary_moves: SummaryMoveRoots {\n\
             \x20       descriptions: GbaPtr::at(0x0800_1000),\n\
             \x20       contest_moves: GbaPtr::at(0x0800_2000),\n\
             \x20       contest_effects: GbaPtr::at(0x0800_3000),\n\
             \x20       effect_descriptions: GbaPtr::at(0x0800_4000),\n\
             \x20   },\n"
        );
    }
}
