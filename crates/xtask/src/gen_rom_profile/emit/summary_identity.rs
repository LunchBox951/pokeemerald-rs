//! The emitted `summary_identity` root family.

use std::fmt::Write as _;

use super::gba_ptr_expression;
use crate::gen_rom_profile::plan::SummaryIdentityPlan;

/// Writes the `summary_identity: SummaryIdentityRoots { .. }` field of the root table.
pub(super) fn summary_identity_roots(out: &mut String, identity: &SummaryIdentityPlan) {
    let _ = writeln!(out, "    summary_identity: SummaryIdentityRoots {{");
    for (field, addr) in [
        ("natures", identity.natures),
        ("region_map_entries", identity.region_map_entries),
        ("ferry", identity.ferry),
        ("secret_base", identity.secret_base),
        ("hideout", identity.hideout),
        ("species_to_national", identity.species_to_national),
    ] {
        let _ = writeln!(out, "        {field}: {},", gba_ptr_expression(addr));
    }
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
mod tests {
    use super::summary_identity_roots;
    use crate::gen_rom_profile::plan::SummaryIdentityPlan;

    #[test]
    fn the_six_roots_render_one_address_per_line() {
        let mut out = String::new();
        summary_identity_roots(
            &mut out,
            &SummaryIdentityPlan {
                natures: 0x0800_1000,
                region_map_entries: 0x0800_2000,
                ferry: 0x0800_3000,
                secret_base: 0x0800_3006,
                hideout: 0x0800_3012,
                species_to_national: 0x0800_4000,
            },
        );
        assert_eq!(
            out,
            "    summary_identity: SummaryIdentityRoots {\n\
             \x20       natures: GbaPtr::at(0x0800_1000),\n\
             \x20       region_map_entries: GbaPtr::at(0x0800_2000),\n\
             \x20       ferry: GbaPtr::at(0x0800_3000),\n\
             \x20       secret_base: GbaPtr::at(0x0800_3006),\n\
             \x20       hideout: GbaPtr::at(0x0800_3012),\n\
             \x20       species_to_national: GbaPtr::at(0x0800_4000),\n\
             \x20   },\n"
        );
    }
}
